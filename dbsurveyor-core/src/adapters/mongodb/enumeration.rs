//! MongoDB database and collection enumeration.
//!
//! This module provides functionality for listing databases and collections
//! on a MongoDB server.
//!
//! # Features
//! - List all accessible databases on a MongoDB server
//! - Filter system databases by default
//! - List collections within a database
//! - Get collection statistics (document count, size)

use crate::Result;
use mongodb::Client;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};

/// System databases that are excluded by default when listing databases.
pub const SYSTEM_DATABASES: &[&str] = &["admin", "config", "local"];

/// Information about a database on the MongoDB server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumeratedDatabase {
    /// Database name
    pub name: String,
    /// Database size in bytes (may be None if insufficient privileges)
    pub size_bytes: Option<u64>,
    /// Whether this is a system database
    pub is_system_database: bool,
    /// Whether the current user can access this database
    pub is_accessible: bool,
}

impl EnumeratedDatabase {
    /// Creates a new EnumeratedDatabase with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            size_bytes: None,
            is_system_database: false,
            is_accessible: true,
        }
    }

    /// Checks if this database is a known system database.
    pub fn check_is_system_database(name: &str) -> bool {
        SYSTEM_DATABASES.contains(&name)
    }
}

/// Information about a collection in a MongoDB database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumeratedCollection {
    /// Collection name
    pub name: String,
    /// Collection type (collection or view)
    pub collection_type: CollectionType,
    /// Estimated document count
    pub document_count: Option<u64>,
    /// Collection size in bytes
    pub size_bytes: Option<u64>,
    /// Average document size in bytes
    pub avg_document_size: Option<u64>,
    /// Number of indexes
    pub index_count: Option<u32>,
    /// Whether this is a capped collection
    pub is_capped: bool,
}

/// Type of MongoDB collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionType {
    /// Regular collection
    Collection,
    /// View (computed from other collections)
    View,
    /// Time series collection
    TimeSeries,
}

impl std::fmt::Display for CollectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionType::Collection => write!(f, "collection"),
            CollectionType::View => write!(f, "view"),
            CollectionType::TimeSeries => write!(f, "timeseries"),
        }
    }
}

/// Lists all databases on the MongoDB server.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `include_system` - If true, includes system databases (admin, config, local)
///
/// # Returns
/// A vector of `EnumeratedDatabase` structs
pub async fn list_databases(
    client: &Client,
    include_system: bool,
) -> Result<Vec<EnumeratedDatabase>> {
    tracing::debug!("Listing databases (include_system: {})", include_system);

    // List databases using the listDatabases command
    let databases = client.list_databases().await.map_err(|e| {
        tracing::error!("Failed to enumerate databases: {}", e);
        crate::error::DbSurveyorError::collection_failed("Failed to enumerate databases", e)
    })?;

    let mut result = Vec::with_capacity(databases.len());

    for db_spec in databases {
        let name = db_spec.name.clone();
        let is_system = EnumeratedDatabase::check_is_system_database(&name);

        // Skip system databases if not requested
        if !include_system && is_system {
            tracing::trace!("Skipping system database: {}", name);
            continue;
        }

        let db = EnumeratedDatabase {
            name: name.clone(),
            size_bytes: Some(db_spec.size_on_disk),
            is_system_database: is_system,
            is_accessible: true,
        };

        tracing::trace!(
            "Found database: {} (size: {} bytes, system: {})",
            db.name,
            db.size_bytes.unwrap_or(0),
            db.is_system_database
        );

        result.push(db);
    }

    tracing::info!(
        "Enumerated {} databases (include_system: {})",
        result.len(),
        include_system
    );

    Ok(result)
}

/// Lists only accessible (non-system) databases on the MongoDB server.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `include_system` - If true, includes system databases
///
/// # Returns
/// A vector of `EnumeratedDatabase` structs for accessible databases
pub async fn list_accessible_databases(
    client: &Client,
    include_system: bool,
) -> Result<Vec<EnumeratedDatabase>> {
    let all_databases = list_databases(client, include_system).await?;
    Ok(all_databases
        .into_iter()
        .filter(|db| db.is_accessible)
        .collect())
}

/// Lists all collections in a database.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `database_name` - Name of the database
///
/// # Returns
/// A vector of `EnumeratedCollection` structs
pub async fn list_collections(
    client: &Client,
    database_name: &str,
) -> Result<Vec<EnumeratedCollection>> {
    tracing::debug!("Listing collections in database: {}", database_name);

    let db = client.database(database_name);

    // Use listCollections command to get collection info
    let collections = db.list_collection_names().await.map_err(|e| {
        tracing::error!("Failed to list collections in {}: {}", database_name, e);
        crate::error::DbSurveyorError::collection_failed(
            format!("Failed to list collections in database '{}'", database_name),
            e,
        )
    })?;

    let mut result = Vec::with_capacity(collections.len());

    for collection_name in collections {
        // Skip system collections
        if collection_name.starts_with("system.") {
            tracing::trace!("Skipping system collection: {}", collection_name);
            continue;
        }

        // Get collection stats
        let stats = get_collection_stats(client, database_name, &collection_name).await;

        let collection = EnumeratedCollection {
            name: collection_name.clone(),
            collection_type: stats.as_ref().map_or(
                CollectionType::Collection,
                |(_, _, _, _, _, _, is_view)| {
                    if *is_view {
                        CollectionType::View
                    } else {
                        CollectionType::Collection
                    }
                },
            ),
            document_count: stats
                .as_ref()
                .ok()
                .and_then(|(count, _, _, _, _, _, _)| *count),
            size_bytes: stats
                .as_ref()
                .ok()
                .and_then(|(_, size, _, _, _, _, _)| *size),
            avg_document_size: stats.as_ref().ok().and_then(|(_, _, avg, _, _, _, _)| *avg),
            index_count: stats.as_ref().ok().and_then(|(_, _, _, idx, _, _, _)| *idx),
            is_capped: stats
                .as_ref()
                .is_ok_and(|(_, _, _, _, _, capped, _)| *capped),
        };

        tracing::trace!(
            "Found collection: {} (type: {}, docs: {:?})",
            collection.name,
            collection.collection_type,
            collection.document_count
        );

        result.push(collection);
    }

    tracing::info!(
        "Listed {} collections in database '{}'",
        result.len(),
        database_name
    );

    Ok(result)
}

/// Gets statistics for a collection.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `database_name` - Database name
/// * `collection_name` - Collection name
///
/// # Returns
/// Tuple of (document_count, size_bytes, avg_doc_size, index_count, total_index_size, is_capped, is_view)
async fn get_collection_stats(
    client: &Client,
    database_name: &str,
    collection_name: &str,
) -> Result<(
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<u32>,
    Option<u64>,
    bool,
    bool,
)> {
    let db = client.database(database_name);

    // Run collStats command
    let result = db
        .run_command(doc! { "collStats": collection_name })
        .await
        .map_err(|e| {
            tracing::debug!(
                "Failed to get stats for {}.{}: {}",
                database_name,
                collection_name,
                e
            );
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to get collection stats for '{}.{}'",
                    database_name, collection_name
                ),
                e,
            )
        })?;

    // Extract statistics from response
    // MongoDB may return count as i32 or i64 depending on collection size
    let count = result
        .get_i64("count")
        .ok()
        .map(|c| c as u64)
        .or_else(|| result.get_i32("count").ok().map(|c| c as u64));
    let size = result
        .get_i64("size")
        .ok()
        .map(|s| s as u64)
        .or_else(|| result.get_i32("size").ok().map(|s| s as u64));
    let avg_obj_size = result
        .get_i64("avgObjSize")
        .ok()
        .map(|a| a as u64)
        .or_else(|| result.get_i32("avgObjSize").ok().map(|a| a as u64));
    let num_indexes = result.get_i32("nindexes").ok().map(|n| n as u32);
    let total_index_size = result
        .get_i64("totalIndexSize")
        .ok()
        .map(|s| s as u64)
        .or_else(|| result.get_i32("totalIndexSize").ok().map(|s| s as u64));
    let is_capped = result.get_bool("capped").unwrap_or(false);

    // Check if it's a view (views don't have 'count' in collStats, they have 'ns' matching)
    let is_view = result.get_str("ns").is_err() && count.is_none();

    Ok((
        count,
        size,
        avg_obj_size,
        num_indexes,
        total_index_size,
        is_capped,
        is_view,
    ))
}

/// Gets the list of indexes for a collection.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `database_name` - Database name
/// * `collection_name` - Collection name
///
/// # Returns
/// A vector of index names
pub async fn list_indexes(
    client: &Client,
    database_name: &str,
    collection_name: &str,
) -> Result<Vec<String>> {
    let db = client.database(database_name);
    let collection = db.collection::<mongodb::bson::Document>(collection_name);

    let mut cursor = collection.list_indexes().await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed(
            format!(
                "Failed to list indexes for '{}.{}'",
                database_name, collection_name
            ),
            e,
        )
    })?;

    let mut indexes = Vec::new();
    while cursor.advance().await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed(
            format!(
                "Failed to iterate indexes for '{}.{}'",
                database_name, collection_name
            ),
            e,
        )
    })? {
        let index = cursor.deserialize_current().map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to deserialize index for '{}.{}'",
                    database_name, collection_name
                ),
                e,
            )
        })?;
        if let Some(name) = index.options.and_then(|o| o.name) {
            indexes.push(name);
        }
    }

    Ok(indexes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_database_detection() {
        assert!(EnumeratedDatabase::check_is_system_database("admin"));
        assert!(EnumeratedDatabase::check_is_system_database("config"));
        assert!(EnumeratedDatabase::check_is_system_database("local"));
        assert!(!EnumeratedDatabase::check_is_system_database("mydb"));
        assert!(!EnumeratedDatabase::check_is_system_database("test"));
    }

    #[test]
    fn test_enumerated_database_new() {
        let db = EnumeratedDatabase::new("testdb".to_string());
        assert_eq!(db.name, "testdb");
        assert!(db.size_bytes.is_none());
        assert!(!db.is_system_database);
        assert!(db.is_accessible);
    }

    #[test]
    fn test_collection_type_display() {
        assert_eq!(CollectionType::Collection.to_string(), "collection");
        assert_eq!(CollectionType::View.to_string(), "view");
        assert_eq!(CollectionType::TimeSeries.to_string(), "timeseries");
    }

    #[test]
    fn test_enumerated_database_serialization() {
        let db = EnumeratedDatabase {
            name: "testdb".to_string(),
            size_bytes: Some(1024 * 1024),
            is_system_database: false,
            is_accessible: true,
        };

        let json = serde_json::to_string(&db).unwrap();
        assert!(json.contains("\"name\":\"testdb\""));
        assert!(json.contains("\"is_accessible\":true"));

        let deserialized: EnumeratedDatabase = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, db.name);
        assert_eq!(deserialized.size_bytes, db.size_bytes);
    }

    #[test]
    fn test_enumerated_collection_serialization() {
        let collection = EnumeratedCollection {
            name: "users".to_string(),
            collection_type: CollectionType::Collection,
            document_count: Some(1000),
            size_bytes: Some(102400),
            avg_document_size: Some(102),
            index_count: Some(3),
            is_capped: false,
        };

        let json = serde_json::to_string(&collection).unwrap();
        assert!(json.contains("\"name\":\"users\""));
        assert!(json.contains("\"document_count\":1000"));

        let deserialized: EnumeratedCollection = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, collection.name);
        assert_eq!(deserialized.document_count, collection.document_count);
    }
}
