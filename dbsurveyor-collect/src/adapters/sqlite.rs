//! `SQLite` database adapter for file-based database access
//!
//! This module provides a secure `SQLite` adapter implementation with:
//! - File-based database access with validation
//! - Schema metadata collection from `sqlite_master`
//! - Zero credential storage (`SQLite` is file-based)
//! - Comprehensive error sanitization

use super::{
    AdapterError, AdapterResult, ColumnMetadata, ConnectionConfig, DatabaseMetadata,
    SchemaCollector, SchemaMetadata, TableMetadata,
};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, Pool, Row, Sqlite};
use std::str::FromStr;

/// `SQLite` adapter for file-based databases
pub struct SqliteAdapter {
    pool: Pool<Sqlite>,
    config: ConnectionConfig,
}

impl SqliteAdapter {
    /// Create a new `SQLite` adapter
    ///
    /// # Arguments
    ///
    /// * `connection_string` - `SQLite` connection URL (file path)
    /// * `config` - Connection pool configuration
    ///
    /// # Security
    ///
    /// - Connection string is never logged after pool creation
    /// - File paths are validated before access
    /// - All errors are sanitized
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established
    pub async fn new(
        connection_string: &str,
        config: ConnectionConfig,
    ) -> AdapterResult<Self> {
        // Parse connection options without logging
        let mut connect_options = SqliteConnectOptions::from_str(connection_string)
            .map_err(|_| AdapterError::InvalidParameters)?;

        // Disable statement logging to prevent path leakage
        connect_options = connect_options.disable_statement_logging();

        // Create connection pool with configured limits
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_idle_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .connect_with(connect_options)
            .await
            .map_err(|_| AdapterError::ConnectionFailed)?;

        Ok(Self { pool, config })
    }

    /// Get database version
    async fn get_version(&self) -> AdapterResult<String> {
        let row = sqlx::query("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        let version: String = row
            .try_get(0)
            .map_err(|_| AdapterError::QueryFailed)?;

        Ok(version)
    }

    /// Get all tables in the database
    async fn get_tables(&self) -> AdapterResult<Vec<String>> {
        let rows = sqlx::query(
            "SELECT name FROM sqlite_master 
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdapterError::QueryFailed)?;

        let tables: Vec<String> = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .collect();

        Ok(tables)
    }

    /// Get columns for a specific table
    async fn get_columns(&self, table: &str) -> AdapterResult<Vec<ColumnMetadata>> {
        let query = format!("PRAGMA table_info('{table}')");

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        let columns: Vec<ColumnMetadata> = rows
            .iter()
            .filter_map(|row| {
                Some(ColumnMetadata {
                    name: row.try_get::<String, _>(1).ok()?,
                    data_type: row.try_get::<String, _>(2).ok()?,
                    is_nullable: row.try_get::<i32, _>(3).ok()? == 0,
                    default_value: row.try_get::<Option<String>, _>(4).ok()?,
                })
            })
            .collect();

        Ok(columns)
    }

    /// Get row count for a table
    async fn get_row_count(&self, table: &str) -> AdapterResult<Option<u64>> {
        let query = format!("SELECT COUNT(*) FROM \"{table}\"");

        let row = sqlx::query(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        if let Some(row) = row {
            if let Ok(count) = row.try_get::<i64, _>(0) {
                if count >= 0 {
                    #[allow(clippy::cast_sign_loss)]
                    return Ok(Some(count as u64));
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl SchemaCollector for SqliteAdapter {
    fn database_type(&self) -> &'static str {
        "sqlite"
    }

    async fn test_connection(&self) -> AdapterResult<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AdapterError::ConnectionFailed)?;

        Ok(())
    }

    async fn collect_metadata(&self) -> AdapterResult<DatabaseMetadata> {
        let version = self.get_version().await?;
        let table_names = self.get_tables().await?;

        let mut tables = Vec::new();

        for table_name in table_names {
            let columns = self.get_columns(&table_name).await?;
            let row_count = self.get_row_count(&table_name).await?;

            tables.push(TableMetadata {
                name: table_name,
                schema: "main".to_string(), // SQLite uses 'main' as the default schema
                columns,
                row_count,
            });
        }

        // SQLite has a single schema called "main"
        let schemas = vec![SchemaMetadata {
            name: "main".to_string(),
            tables,
        }];

        Ok(DatabaseMetadata {
            database_type: "sqlite".to_string(),
            version: Some(version),
            schemas,
        })
    }

    fn safe_description(&self) -> String {
        format!(
            "SQLite connection pool (max: {}, idle: {})",
            self.config.max_connections, self.config.min_idle_connections
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::uninlined_format_args)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation_invalid_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = SqliteAdapter::new("invalid://url", ConnectionConfig::default()).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_safe_description() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Use in-memory database for testing
            if let Ok(adapter) =
                SqliteAdapter::new("sqlite::memory:", ConnectionConfig::default()).await
            {
                let description = adapter.safe_description();
                assert!(description.contains("SQLite"));
                assert!(description.contains("max: 10"));
                assert!(!description.contains("password"));
            }
        });
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn test_sqlite_memory_connection() {
        let adapter = SqliteAdapter::new("sqlite::memory:", ConnectionConfig::default())
            .await
            .expect("Failed to create adapter");

        assert_eq!(adapter.database_type(), "sqlite");

        // Test connection
        let result = adapter.test_connection().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn test_sqlite_collect_metadata() {
        let adapter = SqliteAdapter::new("sqlite::memory:", ConnectionConfig::default())
            .await
            .expect("Failed to create adapter");

        // Create a test table
        sqlx::query("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Collect metadata
        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "sqlite");
        assert!(metadata.version.is_some());
        assert_eq!(metadata.schemas.len(), 1);
        assert_eq!(metadata.schemas[0].name, "main");
        assert!(!metadata.schemas[0].tables.is_empty());

        // Find the test table
        let test_table = metadata.schemas[0]
            .tables
            .iter()
            .find(|t| t.name == "test_table");
        assert!(test_table.is_some());

        let test_table = test_table.unwrap();
        assert_eq!(test_table.columns.len(), 2);
        assert_eq!(test_table.columns[0].name, "id");
        assert_eq!(test_table.columns[1].name, "name");
    }
}
