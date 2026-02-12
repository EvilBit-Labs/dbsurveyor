//! MongoDB database adapter with schema inference and document sampling.
//!
//! # Module Structure
//! - `connection`: MongoDB client management and connection validation
//! - `type_mapping`: BSON to `UnifiedDataType` conversion
//! - `schema_inference`: Schema inference from document samples
//! - `sampling`: Document sampling utilities and ordering strategies
//! - `enumeration`: Database and collection enumeration
//!
//! # Schema Inference
//! MongoDB is a schemaless database, so this adapter infers schema by:
//! 1. Sampling a configurable number of documents from each collection
//! 2. Analyzing document structure to discover field names and types
//! 3. Tracking field frequency to determine nullability
//! 4. Handling nested documents and arrays
//!
//! # Security Guarantees
//! - All operations are read-only
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion

mod connection;
pub mod enumeration;
pub mod sampling;
pub mod schema_inference;
pub mod type_mapping;

#[cfg(test)]
mod tests;

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter, SamplingConfig};
use crate::Result;
use crate::models::*;
use async_trait::async_trait;
use mongodb::Client;
use mongodb::bson::doc;
use schema_inference::SchemaInferrer;

// Re-export public items from submodules
pub use enumeration::{
    CollectionType, EnumeratedCollection, EnumeratedDatabase, SYSTEM_DATABASES,
    list_accessible_databases, list_collections, list_databases, list_indexes,
};
pub use sampling::{detect_ordering_strategy, generate_sort_document, sample_collection};
pub use schema_inference::{InferredField, InferredSchema};
pub use type_mapping::{bson_type_name, map_bson_to_unified};

/// MongoDB database adapter with schema inference and document sampling.
///
/// # Example
/// ```rust,ignore
/// use dbsurveyor_core::adapters::mongodb::MongoAdapter;
///
/// let adapter = MongoAdapter::new("mongodb://localhost:27017/mydb").await?;
/// let schema = adapter.collect_schema().await?;
/// ```
pub struct MongoAdapter {
    /// MongoDB client
    pub client: Client,
    /// Connection configuration
    pub config: ConnectionConfig,
    /// Original connection URL (kept private to prevent credential exposure)
    connection_url: String,
}

impl std::fmt::Debug for MongoAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MongoAdapter")
            .field("config", &self.config)
            // Note: connection_url is intentionally omitted to prevent credential exposure
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl DatabaseAdapter for MongoAdapter {
    async fn test_connection(&self) -> Result<()> {
        self.test_connection_internal().await
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        self.collect_schema_internal().await
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

impl MongoAdapter {
    /// Collects the database schema by inferring it from document samples.
    async fn collect_schema_internal(&self) -> Result<DatabaseSchema> {
        let start_time = std::time::Instant::now();
        let mut warnings = Vec::new();

        tracing::info!(
            "Starting MongoDB schema collection for {}:{}",
            self.config.host,
            self.config.port.unwrap_or(27017)
        );

        // Determine the database to collect
        let database_name = self.config.database.clone().ok_or_else(|| {
            crate::error::DbSurveyorError::configuration(
                "No database specified in MongoDB connection string. \
                 Use mongodb://host:port/database_name format.",
            )
        })?;

        // Collect database information
        tracing::debug!("Collecting database information for: {}", database_name);
        let database_info = self.collect_database_info(&database_name).await?;

        // Enumerate collections
        tracing::debug!("Enumerating collections in database: {}", database_name);
        let collections = list_collections(&self.client, &database_name).await?;

        tracing::info!(
            "Found {} collections in database '{}'",
            collections.len(),
            database_name
        );

        // Collect schema for each collection
        let mut tables = Vec::new();
        let mut all_indexes = Vec::new();
        let sampling_config = SamplingConfig::default();

        for collection_info in &collections {
            // Skip views - they don't have their own schema
            if collection_info.collection_type == CollectionType::View {
                tracing::debug!("Skipping view: {}", collection_info.name);
                continue;
            }

            match self
                .collect_collection_schema(&database_name, &collection_info.name, &sampling_config)
                .await
            {
                Ok((table, indexes)) => {
                    tracing::debug!(
                        "Collected schema for collection '{}' with {} fields",
                        collection_info.name,
                        table.columns.len()
                    );
                    tables.push(table);
                    all_indexes.extend(indexes);
                }
                Err(e) => {
                    let warning = format!(
                        "Failed to collect schema for collection '{}': {}",
                        collection_info.name, e
                    );
                    tracing::warn!("{}", warning);
                    warnings.push(warning);
                }
            }
        }

        let collection_duration = start_time.elapsed();

        tracing::info!(
            "MongoDB schema collection completed in {:.2}s - found {} collections with {} total fields",
            collection_duration.as_secs_f64(),
            tables.len(),
            tables.iter().map(|t| t.columns.len()).sum::<usize>()
        );

        Ok(DatabaseSchema {
            format_version: FORMAT_VERSION.to_string(),
            database_info,
            tables,
            views: Vec::new(),
            indexes: all_indexes,
            constraints: Vec::new(),
            procedures: Vec::new(),
            functions: Vec::new(),
            triggers: Vec::new(),
            custom_types: Vec::new(),
            samples: None,
            quality_metrics: None,
            collection_metadata: CollectionMetadata {
                collected_at: chrono::Utc::now(),
                collection_duration_ms: collection_duration.as_millis() as u64,
                collector_version: env!("CARGO_PKG_VERSION").to_string(),
                warnings,
            },
        })
    }

    /// Collects database information for a MongoDB database.
    async fn collect_database_info(&self, database_name: &str) -> Result<DatabaseInfo> {
        let db = self.client.database(database_name);

        // Get database stats
        let stats = db.run_command(doc! { "dbStats": 1 }).await.map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to get database stats for '{}'", database_name),
                e,
            )
        })?;

        // Get server version
        let admin_db = self.client.database("admin");
        let build_info = admin_db.run_command(doc! { "buildInfo": 1 }).await.ok();

        let version = build_info
            .as_ref()
            .and_then(|info| info.get_str("version").ok().map(|s| s.to_string()));

        let size_bytes = stats.get_i64("dataSize").ok().map(|s| s as u64);

        // Check if this is a system database
        let is_system_database =
            enumeration::EnumeratedDatabase::check_is_system_database(database_name);

        Ok(DatabaseInfo {
            name: database_name.to_string(),
            version,
            size_bytes,
            encoding: Some("UTF-8".to_string()), // MongoDB uses UTF-8
            collation: None,
            owner: None,
            is_system_database,
            access_level: AccessLevel::Full,
            collection_status: CollectionStatus::Success,
        })
    }

    /// Collects schema for a single collection by inferring from document samples.
    async fn collect_collection_schema(
        &self,
        database_name: &str,
        collection_name: &str,
        sampling_config: &SamplingConfig,
    ) -> Result<(Table, Vec<Index>)> {
        let db = self.client.database(database_name);
        let collection = db.collection::<mongodb::bson::Document>(collection_name);

        // Get collection stats
        let stats = db
            .run_command(doc! { "collStats": collection_name })
            .await
            .ok();

        let row_count = stats
            .as_ref()
            .and_then(|s| s.get_i64("count").ok().map(|c| c as u64));

        // Sample documents to infer schema
        let mut inferrer = SchemaInferrer::new();

        let options = mongodb::options::FindOptions::builder()
            .limit(i64::from(sampling_config.sample_size))
            .build();

        let mut cursor = collection
            .find(doc! {})
            .with_options(options)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to sample documents from '{}.{}'",
                        database_name, collection_name
                    ),
                    e,
                )
            })?;

        while cursor.advance().await.map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to iterate cursor for '{}.{}'",
                    database_name, collection_name
                ),
                e,
            )
        })? {
            let doc = cursor.deserialize_current().map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to deserialize document from '{}.{}'",
                        database_name, collection_name
                    ),
                    e,
                )
            })?;
            inferrer.analyze_document(&doc);
        }

        // Finalize schema inference
        let inferred_schema = inferrer.finalize(collection_name.to_string());
        let columns = inferred_schema.to_columns();

        // Determine primary key (always _id in MongoDB)
        let primary_key = if columns.iter().any(|c| c.name == "_id") {
            Some(PrimaryKey {
                name: Some("_id_".to_string()),
                columns: vec!["_id".to_string()],
            })
        } else {
            None
        };

        // Collect indexes
        let indexes = self
            .collect_collection_indexes(database_name, collection_name)
            .await
            .unwrap_or_default();

        let table = Table {
            name: collection_name.to_string(),
            schema: Some(database_name.to_string()),
            columns,
            primary_key,
            foreign_keys: Vec::new(), // MongoDB doesn't have foreign keys
            indexes: indexes.clone(),
            constraints: Vec::new(),
            comment: Some(format!(
                "MongoDB collection (sampled {} documents)",
                inferred_schema.documents_sampled
            )),
            row_count,
        };

        Ok((table, indexes))
    }

    /// Collects indexes for a collection.
    async fn collect_collection_indexes(
        &self,
        database_name: &str,
        collection_name: &str,
    ) -> Result<Vec<Index>> {
        let db = self.client.database(database_name);
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
            let index_model = cursor.deserialize_current().map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to deserialize index for '{}.{}'",
                        database_name, collection_name
                    ),
                    e,
                )
            })?;

            let options = index_model.options;
            let name = options
                .as_ref()
                .and_then(|o| o.name.clone())
                .unwrap_or_else(|| "unnamed".to_string());
            let is_unique = options.as_ref().and_then(|o| o.unique).unwrap_or(false);

            // Parse index keys
            let columns: Vec<IndexColumn> = index_model
                .keys
                .iter()
                .map(|(key, value)| {
                    let sort_order = match value.as_i32() {
                        Some(1) => Some(SortDirection::Ascending),
                        Some(-1) => Some(SortDirection::Descending),
                        _ => None,
                    };
                    IndexColumn {
                        name: key.clone(),
                        sort_order,
                    }
                })
                .collect();

            let is_primary = name == "_id_";

            indexes.push(Index {
                name,
                table_name: collection_name.to_string(),
                schema: Some(database_name.to_string()),
                columns,
                is_unique,
                is_primary,
                index_type: Some("btree".to_string()), // MongoDB primarily uses B-tree indexes
            });
        }

        Ok(indexes)
    }

    /// Samples data from a collection.
    ///
    /// # Arguments
    /// * `database` - Database name
    /// * `collection` - Collection name
    /// * `config` - Sampling configuration
    ///
    /// # Returns
    /// A `TableSample` containing the sampled documents
    pub async fn sample_collection(
        &self,
        database: &str,
        collection: &str,
        config: &SamplingConfig,
    ) -> Result<TableSample> {
        sampling::sample_collection(&self.client, database, collection, config).await
    }

    /// Detects the ordering strategy for a collection.
    ///
    /// # Arguments
    /// * `database` - Database name
    /// * `collection` - Collection name
    ///
    /// # Returns
    /// The detected ordering strategy
    pub async fn detect_ordering_strategy(
        &self,
        database: &str,
        collection: &str,
    ) -> Result<OrderingStrategy> {
        sampling::detect_ordering_strategy(&self.client, database, collection).await
    }

    /// Lists all databases on the MongoDB server.
    ///
    /// # Arguments
    /// * `include_system` - If true, includes system databases (admin, config, local)
    ///
    /// # Returns
    /// A vector of `EnumeratedDatabase` structs
    pub async fn list_databases(&self, include_system: bool) -> Result<Vec<EnumeratedDatabase>> {
        enumeration::list_databases(&self.client, include_system).await
    }

    /// Lists collections in a database.
    ///
    /// # Arguments
    /// * `database` - Database name
    ///
    /// # Returns
    /// A vector of `EnumeratedCollection` structs
    pub async fn list_collections(&self, database: &str) -> Result<Vec<EnumeratedCollection>> {
        enumeration::list_collections(&self.client, database).await
    }
}
