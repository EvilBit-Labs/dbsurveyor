//! PostgreSQL database adapter with connection pooling and comprehensive schema collection.
//!
//! # Module Structure
//! - `connection`: Connection pool management and validation
//! - `type_mapping`: PostgreSQL to unified data type conversion
//! - `schema_collection`: Table, column, constraint, and index collection
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/DESCRIBE only)
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion
//! - Connection pooling with configurable limits

mod connection;
mod schema_collection;
mod type_mapping;

#[cfg(test)]
mod tests;

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{Result, models::*};
use async_trait::async_trait;
use sqlx::PgPool;

// Re-export public items from submodules
pub use type_mapping::{map_postgresql_type, map_referential_action};

/// PostgreSQL database adapter with connection pooling and comprehensive schema collection
pub struct PostgresAdapter {
    pub pool: PgPool,
    pub config: ConnectionConfig,
}

impl std::fmt::Debug for PostgresAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAdapter")
            .field("config", &self.config)
            .field("pool_size", &self.pool.size())
            .field("pool_idle", &self.pool.num_idle())
            .finish()
    }
}

#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn test_connection(&self) -> Result<()> {
        // Set up session security settings first
        self.setup_session().await?;

        // Test basic connectivity
        let connectivity_result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(crate::error::DbSurveyorError::connection_failed)?;

        if connectivity_result != 1 {
            return Err(crate::error::DbSurveyorError::configuration(
                "Basic connectivity test failed: unexpected result",
            ));
        }

        // Verify we can access information_schema (required for schema collection)
        let schema_access_test: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'information_schema'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::insufficient_privileges(
                format!("Cannot access information_schema: {}", e)
            )
        })?;

        if schema_access_test == 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "No access to information_schema tables",
            ));
        }

        Ok(())
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        schema_collection::collect_schema(self).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn supports_feature(&self, feature: AdapterFeature) -> bool {
        matches!(
            feature,
            AdapterFeature::SchemaCollection
                | AdapterFeature::DataSampling
                | AdapterFeature::MultiDatabase
                | AdapterFeature::ConnectionPooling
                | AdapterFeature::QueryTimeout
                | AdapterFeature::ReadOnlyMode
        )
    }

    fn connection_config(&self) -> ConnectionConfig {
        self.config.clone()
    }
}
