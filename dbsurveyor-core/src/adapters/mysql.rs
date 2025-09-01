//! MySQL database adapter (placeholder implementation).
//!
//! This module will be implemented in subsequent tasks.

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{Result, models::*};
use async_trait::async_trait;

/// MySQL database adapter (placeholder)
pub struct MySqlAdapter {
    config: ConnectionConfig,
}

impl MySqlAdapter {
    /// Creates a new MySQL adapter (placeholder)
    pub async fn new(_connection_string: &str) -> Result<Self> {
        Ok(Self {
            config: ConnectionConfig::default(),
        })
    }
}

#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn test_connection(&self) -> Result<()> {
        // Placeholder implementation
        Err(crate::error::DbSurveyorError::configuration(
            "MySQL adapter not yet implemented",
        ))
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        // Placeholder implementation
        let db_info = DatabaseInfo::new("placeholder".to_string());
        Ok(DatabaseSchema::new(db_info))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
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
