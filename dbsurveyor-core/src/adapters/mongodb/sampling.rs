//! MongoDB document sampling utilities.
//!
//! This module provides functionality for sampling documents from MongoDB
//! collections for schema inference and data analysis.
//!
//! # Sampling Strategies
//! - Most recent: Order by `_id` descending (ObjectId includes timestamp)
//! - Random: Use `$sample` aggregation stage
//! - Natural order: Use natural document order

use crate::Result;
use crate::adapters::config::SamplingConfig;
use crate::models::{OrderingStrategy, SamplingStrategy, SortDirection, TableSample};
use mongodb::Client;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use serde_json::Value as JsonValue;
use std::time::Duration;

/// Common timestamp field names used for ordering by "most recent".
const TIMESTAMP_FIELD_NAMES: &[&str] = &[
    "createdAt",
    "created_at",
    "updatedAt",
    "updated_at",
    "modifiedAt",
    "modified_at",
    "timestamp",
    "date",
    "insertedAt",
    "inserted_at",
];

/// Detects the best ordering strategy for sampling a collection.
///
/// MongoDB has some natural ordering options:
/// 1. `_id` field (ObjectId contains timestamp, so descending = most recent)
/// 2. Timestamp fields (createdAt, updatedAt, etc.)
/// 3. Natural order (order documents are stored on disk)
///
/// # Arguments
/// * `client` - MongoDB client
/// * `database` - Database name
/// * `collection` - Collection name
///
/// # Returns
/// The detected ordering strategy
pub async fn detect_ordering_strategy(
    client: &Client,
    database: &str,
    collection: &str,
) -> Result<OrderingStrategy> {
    let db = client.database(database);
    let coll = db.collection::<Document>(collection);

    // Get a sample document to analyze its structure
    let sample_doc = coll.find_one(doc! {}).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed(
            format!(
                "Failed to get sample document from '{}.{}'",
                database, collection
            ),
            e,
        )
    })?;

    let Some(doc) = sample_doc else {
        // Empty collection - use _id as default
        return Ok(OrderingStrategy::PrimaryKey {
            columns: vec!["_id".to_string()],
        });
    };

    // Check for common timestamp fields
    for field_name in TIMESTAMP_FIELD_NAMES {
        if doc.contains_key(*field_name) {
            tracing::debug!(
                "Detected timestamp ordering for {}.{}: {}",
                database,
                collection,
                field_name
            );
            return Ok(OrderingStrategy::Timestamp {
                column: (*field_name).to_string(),
                direction: SortDirection::Descending,
            });
        }
    }

    // Default to _id ordering (ObjectId contains timestamp)
    tracing::debug!(
        "Using _id ordering for {}.{} (ObjectId includes timestamp)",
        database,
        collection
    );
    Ok(OrderingStrategy::PrimaryKey {
        columns: vec!["_id".to_string()],
    })
}

/// Generates a MongoDB sort document for the given ordering strategy.
///
/// # Arguments
/// * `strategy` - The ordering strategy
/// * `descending` - If true, sort descending (most recent first)
///
/// # Returns
/// A BSON document for the sort operation
pub fn generate_sort_document(strategy: &OrderingStrategy, descending: bool) -> Document {
    let direction = if descending { -1 } else { 1 };

    match strategy {
        OrderingStrategy::PrimaryKey { columns } => {
            let mut sort_doc = Document::new();
            for col in columns {
                sort_doc.insert(col.as_str(), direction);
            }
            sort_doc
        }
        OrderingStrategy::Timestamp { column, .. } => {
            doc! { column: direction }
        }
        OrderingStrategy::AutoIncrement { column } => {
            doc! { column: direction }
        }
        OrderingStrategy::SystemRowId { column } => {
            doc! { column: direction }
        }
        OrderingStrategy::Unordered => {
            // For unordered, we'll use $sample in the aggregation pipeline
            Document::new()
        }
    }
}

/// Samples documents from a MongoDB collection.
///
/// # Arguments
/// * `client` - MongoDB client
/// * `database` - Database name
/// * `collection` - Collection name
/// * `config` - Sampling configuration
///
/// # Returns
/// A `TableSample` containing the sampled documents as JSON
pub async fn sample_collection(
    client: &Client,
    database: &str,
    collection: &str,
    config: &SamplingConfig,
) -> Result<TableSample> {
    let mut warnings = Vec::new();

    // Apply rate limiting delay if configured
    if let Some(throttle_ms) = config.throttle_ms {
        tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
    }

    // Detect ordering strategy
    let strategy = detect_ordering_strategy(client, database, collection).await?;

    // Determine sampling strategy based on ordering
    let (sampling_strategy, use_random) = match &strategy {
        OrderingStrategy::Unordered => {
            warnings.push("Using random sampling - results may not be reproducible".to_string());
            (
                SamplingStrategy::Random {
                    limit: config.sample_size,
                },
                true,
            )
        }
        _ => (
            SamplingStrategy::MostRecent {
                limit: config.sample_size,
            },
            false,
        ),
    };

    let db = client.database(database);
    let coll = db.collection::<Document>(collection);

    // Get estimated document count
    let estimated_count = coll.estimated_document_count().await.ok();

    let documents: Vec<Document> = if use_random {
        // Use $sample aggregation for random sampling
        sample_random(client, database, collection, config.sample_size).await?
    } else {
        // Use find with sort for ordered sampling
        let sort_doc = generate_sort_document(&strategy, true);
        let options = FindOptions::builder()
            .sort(sort_doc)
            .limit(i64::from(config.sample_size))
            .build();

        let mut cursor = coll
            .find(doc! {})
            .with_options(options)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to sample documents from '{}.{}'",
                        database, collection
                    ),
                    e,
                )
            })?;

        let mut docs = Vec::new();
        while cursor.advance().await.map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to iterate cursor for '{}.{}'", database, collection),
                e,
            )
        })? {
            let doc = cursor.deserialize_current().map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to deserialize document from '{}.{}'",
                        database, collection
                    ),
                    e,
                )
            })?;
            docs.push(doc);
        }
        docs
    };

    // Convert BSON documents to JSON
    let rows: Vec<JsonValue> = documents
        .into_iter()
        .map(|doc| bson_doc_to_json(&doc))
        .collect();

    let actual_sample_size = rows.len() as u32;

    if actual_sample_size < config.sample_size && !use_random {
        tracing::debug!(
            "Collection {}.{} has only {} documents, less than requested sample size of {}",
            database,
            collection,
            actual_sample_size,
            config.sample_size
        );
    }

    Ok(TableSample {
        table_name: collection.to_string(),
        schema_name: Some(database.to_string()),
        rows,
        sample_size: actual_sample_size,
        total_rows: estimated_count,
        sampling_strategy,
        collected_at: chrono::Utc::now(),
        warnings,
    })
}

/// Samples documents using the $sample aggregation stage for random sampling.
async fn sample_random(
    client: &Client,
    database: &str,
    collection: &str,
    sample_size: u32,
) -> Result<Vec<Document>> {
    let db = client.database(database);
    let coll = db.collection::<Document>(collection);

    // Use $sample aggregation stage
    let pipeline = vec![doc! { "$sample": { "size": sample_size as i64 } }];

    let mut cursor = coll.aggregate(pipeline).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed(
            format!(
                "Failed to sample documents from '{}.{}'",
                database, collection
            ),
            e,
        )
    })?;

    let mut docs = Vec::new();
    while cursor.advance().await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed(
            format!(
                "Failed to iterate sample cursor for '{}.{}'",
                database, collection
            ),
            e,
        )
    })? {
        let doc = cursor.deserialize_current().map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to deserialize sampled document from '{}.{}'",
                    database, collection
                ),
                e,
            )
        })?;
        docs.push(doc);
    }

    Ok(docs)
}

/// Converts a BSON document to a JSON value.
///
/// This handles special BSON types like ObjectId, DateTime, etc.
fn bson_doc_to_json(doc: &Document) -> JsonValue {
    // Use the built-in BSON to JSON conversion
    serde_json::to_value(doc).unwrap_or(JsonValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_sort_document_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["_id".to_string()],
        };

        let sort = generate_sort_document(&strategy, true);
        assert_eq!(sort.get_i32("_id"), Ok(-1));

        let sort = generate_sort_document(&strategy, false);
        assert_eq!(sort.get_i32("_id"), Ok(1));
    }

    #[test]
    fn test_generate_sort_document_timestamp() {
        let strategy = OrderingStrategy::Timestamp {
            column: "createdAt".to_string(),
            direction: SortDirection::Descending,
        };

        let sort = generate_sort_document(&strategy, true);
        assert_eq!(sort.get_i32("createdAt"), Ok(-1));
    }

    #[test]
    fn test_generate_sort_document_unordered() {
        let strategy = OrderingStrategy::Unordered;
        let sort = generate_sort_document(&strategy, true);
        assert!(sort.is_empty());
    }

    #[test]
    fn test_bson_doc_to_json() {
        let doc = doc! {
            "name": "John",
            "age": 30,
            "active": true
        };

        let json = bson_doc_to_json(&doc);
        assert!(json.is_object());
        assert_eq!(json["name"], "John");
        assert_eq!(json["age"], 30);
        assert_eq!(json["active"], true);
    }

    #[test]
    fn test_bson_doc_to_json_with_nested() {
        let doc = doc! {
            "profile": {
                "firstName": "John",
                "lastName": "Doe"
            },
            "tags": ["rust", "mongodb"]
        };

        let json = bson_doc_to_json(&doc);
        assert!(json["profile"].is_object());
        assert_eq!(json["profile"]["firstName"], "John");
        assert!(json["tags"].is_array());
    }
}
