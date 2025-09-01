//! SQL Server database adapter (placeholder implementation).
//!
//! This module will be implemented in subsequent tasks.

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{models::*, Result};
use async_trait::async_trait;

/// SQL Server database adapter (placeholder)
pub struct SqlServerAdapter {
    config: ConnectionConfig,
}

impl SqlServerAdapter {
    /// Creates a new SQL Server adapter (placeholder)
    pub async fn new(_connection_string: &str) -> Result<Self> {
        Ok(Self {
            config: ConnectionConfig::default(),
        })
    }
}

#[async_trait]
impl DatabaseAdapter for SqlServerAdapter {
    async fn test_connection(&self) -> Result<()> {
        // Placeholder implementation
        Err(crate::error::DbSurveyorError::configuration(
            "SQL Server adapter not yet implemented",
        ))
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        // Placeholder implementation
        let db_info = DatabaseInfo::new("placeholder".to_string());
        Ok(DatabaseSchema::new(db_info))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SqlServer
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
