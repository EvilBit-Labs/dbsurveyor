//! PostgreSQL database enumeration implementation.
//!
//! This module provides functionality for listing all databases on a PostgreSQL server,
//! including metadata such as owner, encoding, size, and accessibility status.
//!
//! # Features
//! - List all accessible databases on a PostgreSQL server
//! - Filter system databases (template0, template1) by default
//! - Check database accessibility using `has_database_privilege()`
//! - Get database size via `pg_database_size()` (requires appropriate privileges)
//!
//! # Security
//! - All operations are read-only
//! - Permission errors are handled gracefully
//! - Size retrieval fails gracefully for inaccessible databases

use crate::Result;
use crate::adapters::helpers::RowExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// System databases that are excluded by default when listing databases.
///
/// These are PostgreSQL template databases used for creating new databases
/// and are typically not useful for schema collection purposes.
pub const SYSTEM_DATABASES: &[&str] = &["template0", "template1"];

/// Information about a database on the PostgreSQL server.
///
/// This struct is used for database enumeration (listing databases)
/// and contains metadata useful for multi-database collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumeratedDatabase {
    /// Database name
    pub name: String,
    /// Owner role name
    pub owner: String,
    /// Database encoding (e.g., "UTF8")
    pub encoding: String,
    /// Database collation
    pub collation: String,
    /// Database size in bytes (may be None if inaccessible or insufficient privileges)
    pub size_bytes: Option<u64>,
    /// Whether this is a system/template database
    pub is_system_database: bool,
    /// Whether the current user can connect to this database
    pub is_accessible: bool,
}

impl EnumeratedDatabase {
    /// Creates a new EnumeratedDatabase with the given name.
    ///
    /// Other fields are set to default values and should be populated
    /// from database queries.
    pub fn new(name: String) -> Self {
        Self {
            name,
            owner: String::new(),
            encoding: String::new(),
            collation: String::new(),
            size_bytes: None,
            is_system_database: false,
            is_accessible: false,
        }
    }

    /// Checks if this database is a known system database.
    pub fn check_is_system_database(name: &str) -> bool {
        SYSTEM_DATABASES.contains(&name)
    }
}

/// Options for listing databases.
#[derive(Debug, Clone, Default)]
pub struct ListDatabasesOptions {
    /// Include system databases (template0, template1) in the listing
    pub include_system: bool,
}

impl ListDatabasesOptions {
    /// Creates new options with default values (exclude system databases).
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates options that include system databases.
    pub fn with_system_databases() -> Self {
        Self {
            include_system: true,
        }
    }
}

/// Lists all accessible databases on the PostgreSQL server.
///
/// This function queries `pg_database` to enumerate all databases and checks
/// each one for accessibility using `has_database_privilege()`.
///
/// # Arguments
///
/// * `pool` - Connection pool to use for queries
/// * `include_system` - If true, includes template0 and template1
///
/// # Returns
///
/// A vector of `EnumeratedDatabase` structs, sorted by database name.
///
/// # Errors
///
/// Returns an error if the query fails or if insufficient privileges
/// prevent access to `pg_database`.
///
/// # Example
///
/// ```rust,ignore
/// let databases = list_databases(&pool, false).await?;
/// for db in databases {
///     println!("Database: {} (owner: {})", db.name, db.owner);
/// }
/// ```
pub async fn list_databases(
    pool: &PgPool,
    include_system: bool,
) -> Result<Vec<EnumeratedDatabase>> {
    tracing::debug!("Listing databases (include_system: {})", include_system);

    // Query all databases with their metadata
    // Note: pg_database_size() may fail for some databases due to permissions,
    // so we handle this gracefully in a subquery with CASE
    let query = r#"
        SELECT
            d.datname::text as name,
            COALESCE(r.rolname, 'unknown')::text as owner,
            pg_encoding_to_char(d.encoding)::text as encoding,
            d.datcollate::text as collation,
            d.datistemplate as is_template,
            d.datallowconn as allows_connections,
            has_database_privilege(d.datname, 'CONNECT') as can_connect,
            CASE
                WHEN has_database_privilege(d.datname, 'CONNECT') THEN
                    (SELECT pg_database_size(d.datname))
                ELSE NULL
            END as size_bytes
        FROM pg_database d
        LEFT JOIN pg_roles r ON d.datdba = r.oid
        ORDER BY d.datname
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        tracing::error!("Failed to enumerate databases: {}", e);
        match &e {
            sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                crate::error::DbSurveyorError::insufficient_privileges(
                    "Cannot access pg_database - insufficient privileges for database enumeration",
                )
            }
            _ => crate::error::DbSurveyorError::collection_failed(
                "Failed to enumerate databases from pg_database",
                e,
            ),
        }
    })?;

    let mut databases = Vec::with_capacity(rows.len());

    for row in rows {
        let name: String = row.get_field("name", Some("pg_database"))?;
        let owner: String = row.get_field("owner", Some("pg_database"))?;
        let encoding: String = row.get_field("encoding", Some("pg_database"))?;
        let collation: String = row.get_field("collation", Some("pg_database"))?;
        let is_template: bool = row.get_field("is_template", Some("pg_database"))?;
        let allows_connections: bool = row.get_field("allows_connections", Some("pg_database"))?;
        let can_connect: bool = row.get_field("can_connect", Some("pg_database"))?;
        let size_bytes: Option<i64> = row.get_field("size_bytes", Some("pg_database"))?;

        // Determine if this is a system database
        let is_system_database = EnumeratedDatabase::check_is_system_database(&name) || is_template;

        // Skip system databases if not requested
        if !include_system && is_system_database {
            tracing::trace!("Skipping system database: {}", name);
            continue;
        }

        // Determine accessibility (must allow connections AND user has CONNECT privilege)
        let is_accessible = allows_connections && can_connect;

        let db = EnumeratedDatabase {
            name: name.clone(),
            owner,
            encoding,
            collation,
            size_bytes: size_bytes.map(|s| s as u64),
            is_system_database,
            is_accessible,
        };

        tracing::trace!(
            "Found database: {} (owner: {}, accessible: {}, system: {})",
            db.name,
            db.owner,
            db.is_accessible,
            db.is_system_database
        );

        databases.push(db);
    }

    tracing::info!(
        "Enumerated {} databases (include_system: {})",
        databases.len(),
        include_system
    );

    Ok(databases)
}

/// Lists only accessible databases on the PostgreSQL server.
///
/// This is a convenience function that filters out inaccessible databases.
///
/// # Arguments
///
/// * `pool` - Connection pool to use for queries
/// * `include_system` - If true, includes system databases
///
/// # Returns
///
/// A vector of `EnumeratedDatabase` structs for accessible databases only.
pub async fn list_accessible_databases(
    pool: &PgPool,
    include_system: bool,
) -> Result<Vec<EnumeratedDatabase>> {
    let all_databases = list_databases(pool, include_system).await?;
    Ok(all_databases
        .into_iter()
        .filter(|db| db.is_accessible)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_database_detection() {
        assert!(EnumeratedDatabase::check_is_system_database("template0"));
        assert!(EnumeratedDatabase::check_is_system_database("template1"));
        assert!(!EnumeratedDatabase::check_is_system_database("postgres"));
        assert!(!EnumeratedDatabase::check_is_system_database("mydb"));
        assert!(!EnumeratedDatabase::check_is_system_database("template2"));
    }

    #[test]
    fn test_enumerated_database_new() {
        let db = EnumeratedDatabase::new("testdb".to_string());
        assert_eq!(db.name, "testdb");
        assert!(db.owner.is_empty());
        assert!(db.encoding.is_empty());
        assert!(db.collation.is_empty());
        assert!(db.size_bytes.is_none());
        assert!(!db.is_system_database);
        assert!(!db.is_accessible);
    }

    #[test]
    fn test_list_databases_options_default() {
        let options = ListDatabasesOptions::new();
        assert!(!options.include_system);
    }

    #[test]
    fn test_list_databases_options_with_system() {
        let options = ListDatabasesOptions::with_system_databases();
        assert!(options.include_system);
    }

    #[test]
    fn test_enumerated_database_serialization() {
        let db = EnumeratedDatabase {
            name: "testdb".to_string(),
            owner: "postgres".to_string(),
            encoding: "UTF8".to_string(),
            collation: "en_US.UTF-8".to_string(),
            size_bytes: Some(1024 * 1024),
            is_system_database: false,
            is_accessible: true,
        };

        let json = serde_json::to_string(&db).unwrap();
        assert!(json.contains("\"name\":\"testdb\""));
        assert!(json.contains("\"owner\":\"postgres\""));
        assert!(json.contains("\"encoding\":\"UTF8\""));
        assert!(json.contains("\"is_accessible\":true"));

        let deserialized: EnumeratedDatabase = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, db.name);
        assert_eq!(deserialized.owner, db.owner);
        assert_eq!(deserialized.size_bytes, db.size_bytes);
    }
}
