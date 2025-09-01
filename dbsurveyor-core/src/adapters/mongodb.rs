//! MongoDB database adapter (placeholder implementation).
//!
//! This module will be implemented in subsequent tasks.

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{Result, models::*};
use async_trait::async_trait;

/// MongoDB database adapter (placeholder)
pub struct MongoAdapter {
    config: ConnectionConfig,
}

impl MongoAdapter {
    /// Creates a new MongoDB adapter (placeholder)
    pub async fn new(_connection_string: &str) -> Result<Self> {
        Ok(Self {
            config: ConnectionConfig::default(),
        })
    }
}

#[async_trait]
impl DatabaseAdapter for MongoAdapter {
    async fn test_connection(&self) -> Result<()> {
        // Placeholder implementation
        Err(crate::error::DbSurveyorError::configuration(
            "MongoDB adapter not yet implemented",
        ))
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        // Placeholder implementation
        let db_info = DatabaseInfo::new("placeholder".to_string());
        Ok(DatabaseSchema::new(db_info))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MongoDB
    }

    fn supports_feature(&self, feature: AdapterFeature) -> bool {
        matches!(
            feature,
            AdapterFeature::SchemaCollection
                | AdapterFeature::DataSampling
                | AdapterFeature::QueryTimeout
        )
    }

    fn connection_config(&self) -> ConnectionConfig {
        self.config.clone()
    }
}
