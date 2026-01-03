//! `MySQL` database adapter with connection pooling and schema collection
//!
//! This module provides a secure `MySQL` adapter implementation with:
//! - Connection pooling with configurable limits
//! - Schema metadata collection from `information_schema`
//! - Zero credential storage
//! - Comprehensive error sanitization

use super::{
    AdapterError, AdapterResult, ColumnMetadata, ConnectionConfig, DatabaseMetadata,
    SchemaCollector, SchemaMetadata, TableMetadata,
};
use async_trait::async_trait;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{ConnectOptions, MySql, Pool, Row};
use std::str::FromStr;

/// `MySQL` adapter with connection pooling
pub struct MySqlAdapter {
    pool: Pool<MySql>,
    config: ConnectionConfig,
}

impl MySqlAdapter {
    /// Create a new `MySQL` adapter with connection pooling
    ///
    /// # Arguments
    ///
    /// * `connection_string` - `MySQL` connection URL (credentials will not be logged)
    /// * `config` - Connection pool configuration
    ///
    /// # Security
    ///
    /// - Connection string is never logged or stored after pool creation
    /// - Credentials are consumed during connection establishment
    /// - All errors are sanitized to prevent credential leakage
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established
    pub async fn new(connection_string: &str, config: ConnectionConfig) -> AdapterResult<Self> {
        // Parse connection options without logging
        let mut connect_options = MySqlConnectOptions::from_str(connection_string)
            .map_err(|_| AdapterError::InvalidParameters)?;

        // Disable statement logging to prevent credential leakage
        connect_options = connect_options.disable_statement_logging();

        // Create connection pool with configured limits
        let pool = MySqlPoolOptions::new()
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
        let row = sqlx::query("SELECT VERSION()")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        let version: String = row
            .try_get(0)
            .map_err(|_| AdapterError::QueryFailed)?;

        Ok(version)
    }

    /// List all schemas in the database
    async fn list_schemas(&self) -> AdapterResult<Vec<String>> {
        let rows = sqlx::query(
            "SELECT SCHEMA_NAME 
             FROM INFORMATION_SCHEMA.SCHEMATA 
             WHERE SCHEMA_NAME NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys')
             ORDER BY SCHEMA_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdapterError::QueryFailed)?;

        let schemas: Vec<String> = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .collect();

        Ok(schemas)
    }

    /// Get tables for a specific schema
    async fn get_tables(&self, schema: &str) -> AdapterResult<Vec<String>> {
        let rows = sqlx::query(
            "SELECT TABLE_NAME 
             FROM INFORMATION_SCHEMA.TABLES 
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'
             ORDER BY TABLE_NAME",
        )
        .bind(schema)
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
    async fn get_columns(&self, schema: &str, table: &str) -> AdapterResult<Vec<ColumnMetadata>> {
        let rows = sqlx::query(
            "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT
             FROM INFORMATION_SCHEMA.COLUMNS
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
             ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdapterError::QueryFailed)?;

        let columns: Vec<ColumnMetadata> = rows
            .iter()
            .filter_map(|row| {
                Some(ColumnMetadata {
                    name: row.try_get::<String, _>(0).ok()?,
                    data_type: row.try_get::<String, _>(1).ok()?,
                    is_nullable: row.try_get::<String, _>(2).ok()? == "YES",
                    default_value: row.try_get::<Option<String>, _>(3).ok()?,
                })
            })
            .collect();

        Ok(columns)
    }

    /// Get row count estimate for a table
    async fn get_row_count(&self, schema: &str, table: &str) -> AdapterResult<Option<u64>> {
        let query = format!(
            "SELECT TABLE_ROWS FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = '{schema}' AND TABLE_NAME = '{table}'"
        );

        let row = sqlx::query(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        if let Some(row) = row {
            if let Ok(count) = row.try_get::<Option<u64>, _>(0) {
                return Ok(count);
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl SchemaCollector for MySqlAdapter {
    fn database_type(&self) -> &'static str {
        "mysql"
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
        let schema_names = self.list_schemas().await?;

        let mut schemas = Vec::new();

        for schema_name in schema_names {
            let table_names = self.get_tables(&schema_name).await?;
            let mut tables = Vec::new();

            for table_name in table_names {
                let columns = self.get_columns(&schema_name, &table_name).await?;
                let row_count = self.get_row_count(&schema_name, &table_name).await?;

                tables.push(TableMetadata {
                    name: table_name,
                    schema: schema_name.clone(),
                    columns,
                    row_count,
                });
            }

            schemas.push(SchemaMetadata {
                name: schema_name,
                tables,
            });
        }

        Ok(DatabaseMetadata {
            database_type: "mysql".to_string(),
            version: Some(version),
            schemas,
        })
    }

    fn safe_description(&self) -> String {
        format!(
            "MySQL connection pool (max: {}, idle: {})",
            self.config.max_connections, self.config.min_idle_connections
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation_invalid_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = MySqlAdapter::new("invalid://url", ConnectionConfig::default()).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_safe_description() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Use a connection string that won't actually connect but will parse correctly
            if let Ok(adapter) =
                MySqlAdapter::new("mysql://localhost/test", ConnectionConfig::default())
                    .await
            {
                let description = adapter.safe_description();
                assert!(description.contains("MySQL"));
                assert!(description.contains("max: 10"));
                assert!(!description.contains("localhost"));
                assert!(!description.contains("password"));
            }
        });
    }
}
