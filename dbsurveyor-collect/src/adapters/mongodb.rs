//! `MongoDB` database adapter for `NoSQL` document database access
//!
//! This module provides a secure `MongoDB` adapter implementation with:
//! - Document database connection with connection pooling
//! - Collection and field schema inference
//! - Zero credential storage
//! - Comprehensive error sanitization

use super::{
    AdapterError, AdapterResult, ColumnMetadata, ConnectionConfig, DatabaseMetadata,
    SchemaCollector, SchemaMetadata, TableMetadata,
};
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::{
    bson::{doc, Document},
    options::ClientOptions,
    Client,
};

/// `MongoDB` adapter for document database access
pub struct MongoAdapter {
    client: Client,
    config: ConnectionConfig,
    database_name: String,
}

impl MongoAdapter {
    /// Create a new `MongoDB` adapter
    ///
    /// # Arguments
    ///
    /// * `connection_string` - `MongoDB` connection URL (credentials will not be logged)
    /// * `config` - Connection configuration
    ///
    /// # Security
    ///
    /// - Connection string is never logged after client creation
    /// - Credentials are consumed during connection establishment
    /// - All errors are sanitized to prevent credential leakage
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established
    pub async fn new(
        connection_string: &str,
        config: ConnectionConfig,
    ) -> AdapterResult<Self> {
        // Parse connection options
        let mut client_options = ClientOptions::parse(connection_string)
            .await
            .map_err(|_| AdapterError::InvalidParameters)?;

        // Configure connection pool
        client_options.max_pool_size = Some(config.max_connections);
        client_options.min_pool_size = Some(config.min_idle_connections);
        client_options.connect_timeout = Some(config.connect_timeout);
        client_options.max_idle_time = Some(config.idle_timeout);

        // Extract database name from connection string or use default
        let database_name = client_options
            .default_database
            .clone()
            .unwrap_or_else(|| "test".to_string());

        // Create client
        let client = Client::with_options(client_options)
            .map_err(|_| AdapterError::ConnectionFailed)?;

        Ok(Self {
            client,
            config,
            database_name,
        })
    }

    /// Get database version
    async fn get_version(&self) -> AdapterResult<String> {
        let db = self.client.database("admin");
        let result = db
            .run_command(doc! { "buildInfo": 1 })
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        let version = result
            .get_str("version")
            .unwrap_or("unknown")
            .to_string();

        Ok(version)
    }

    /// List all collections in the database
    async fn list_collections(&self) -> AdapterResult<Vec<String>> {
        let db = self.client.database(&self.database_name);
        let collections = db
            .list_collection_names()
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        Ok(collections)
    }

    /// Infer schema for a collection by sampling documents
    async fn infer_schema(&self, collection_name: &str) -> AdapterResult<Vec<ColumnMetadata>> {
        let db = self.client.database(&self.database_name);
        let collection = db.collection::<Document>(collection_name);

        // Sample up to 100 documents to infer schema
        let cursor = collection
            .find(doc! {})
            .limit(100)
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        let documents: Vec<Document> = cursor
            .try_collect()
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        // Collect unique field names and types
        let mut fields = std::collections::HashMap::new();

        for doc in documents {
            for (key, value) in doc {
                let field_type = match value {
                    mongodb::bson::Bson::Double(_) => "double",
                    mongodb::bson::Bson::String(_) => "string",
                    mongodb::bson::Bson::Array(_) => "array",
                    mongodb::bson::Bson::Document(_) => "document",
                    mongodb::bson::Bson::Boolean(_) => "boolean",
                    mongodb::bson::Bson::Null => "null",
                    mongodb::bson::Bson::Int32(_) => "int32",
                    mongodb::bson::Bson::Int64(_) => "int64",
                    mongodb::bson::Bson::DateTime(_) => "datetime",
                    mongodb::bson::Bson::ObjectId(_) => "objectid",
                    _ => "unknown",
                };

                fields
                    .entry(key)
                    .or_insert_with(|| field_type.to_string());
            }
        }

        let mut columns: Vec<ColumnMetadata> = fields
            .into_iter()
            .map(|(name, data_type)| ColumnMetadata {
                name,
                data_type,
                is_nullable: true, // MongoDB fields are always nullable
                default_value: None,
            })
            .collect();

        // Sort by field name for consistency
        columns.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(columns)
    }

    /// Get document count for a collection
    async fn get_document_count(&self, collection_name: &str) -> AdapterResult<Option<u64>> {
        let db = self.client.database(&self.database_name);
        let collection = db.collection::<Document>(collection_name);

        let count = collection
            .estimated_document_count()
            .await
            .map_err(|_| AdapterError::QueryFailed)?;

        Ok(Some(count))
    }
}

#[async_trait]
impl SchemaCollector for MongoAdapter {
    fn database_type(&self) -> &'static str {
        "mongodb"
    }

    async fn test_connection(&self) -> AdapterResult<()> {
        let db = self.client.database("admin");
        db.run_command(doc! { "ping": 1 })
            .await
            .map_err(|_| AdapterError::ConnectionFailed)?;

        Ok(())
    }

    async fn collect_metadata(&self) -> AdapterResult<DatabaseMetadata> {
        let version = self.get_version().await?;
        let collection_names = self.list_collections().await?;

        let mut tables = Vec::new();

        for collection_name in collection_names {
            let columns = self.infer_schema(&collection_name).await?;
            let row_count = self.get_document_count(&collection_name).await?;

            tables.push(TableMetadata {
                name: collection_name,
                schema: self.database_name.clone(),
                columns,
                row_count,
            });
        }

        // MongoDB treats the database as a single schema
        let schemas = vec![SchemaMetadata {
            name: self.database_name.clone(),
            tables,
        }];

        Ok(DatabaseMetadata {
            database_type: "mongodb".to_string(),
            version: Some(version),
            schemas,
        })
    }

    fn safe_description(&self) -> String {
        format!(
            "MongoDB connection (max: {}, idle: {}, db: {})",
            self.config.max_connections, self.config.min_idle_connections, self.database_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation_invalid_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = MongoAdapter::new("invalid://url", ConnectionConfig::default()).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_safe_description() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Test with a syntactically valid MongoDB URL
            if let Ok(adapter) =
                MongoAdapter::new("mongodb://localhost/testdb", ConnectionConfig::default()).await
            {
                let description = adapter.safe_description();
                assert!(description.contains("MongoDB"));
                assert!(description.contains("max: 10"));
                assert!(description.contains("db: testdb"));
                assert!(!description.contains("password"));
            }
        });
    }
}
