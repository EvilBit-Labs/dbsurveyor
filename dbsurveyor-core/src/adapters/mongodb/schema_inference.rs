//! Schema inference from MongoDB documents.
//!
//! MongoDB is a schemaless database, so we must infer the schema by analyzing
//! sampled documents. This module provides functionality to:
//! - Analyze document structure to discover fields
//! - Track field types and frequencies
//! - Merge schema information from multiple documents
//! - Handle nested documents and arrays

use super::type_mapping::{bson_type_name, map_bson_to_unified};
use crate::models::{Column, UnifiedDataType};
use mongodb::bson::{Bson, Document};
use std::collections::HashMap;

/// Information about a discovered field in a MongoDB collection.
#[derive(Debug, Clone)]
pub struct InferredField {
    /// Field name (including dot notation for nested fields)
    pub name: String,
    /// Observed types for this field (there may be multiple if field has varying types)
    pub observed_types: Vec<String>,
    /// Primary unified data type (most common type observed)
    pub unified_type: UnifiedDataType,
    /// Number of documents where this field was present
    pub occurrence_count: u32,
    /// Whether this field is nullable (not present in some documents)
    pub is_nullable: bool,
    /// Position in the schema (order of first discovery)
    pub ordinal_position: u32,
}

/// Schema inference result for a MongoDB collection.
#[derive(Debug, Clone)]
pub struct InferredSchema {
    /// Collection name
    pub collection_name: String,
    /// Number of documents sampled
    pub documents_sampled: u32,
    /// Discovered fields with their types
    pub fields: Vec<InferredField>,
}

impl InferredSchema {
    /// Creates a new empty inferred schema.
    pub fn new(collection_name: String) -> Self {
        Self {
            collection_name,
            documents_sampled: 0,
            fields: Vec::new(),
        }
    }

    /// Converts the inferred schema to database columns.
    ///
    /// # Arguments
    /// * `total_documents` - Total documents sampled for computing nullability
    ///
    /// # Returns
    /// A vector of `Column` structs representing the collection's schema
    pub fn to_columns(&self) -> Vec<Column> {
        self.fields
            .iter()
            .map(|field| Column {
                name: field.name.clone(),
                data_type: field.unified_type.clone(),
                is_nullable: field.is_nullable,
                is_primary_key: field.name == "_id",
                is_auto_increment: field.name == "_id", // ObjectId is auto-generated
                default_value: None,
                comment: if field.observed_types.len() > 1 {
                    Some(format!("Mixed types: {}", field.observed_types.join(", ")))
                } else {
                    None
                },
                ordinal_position: field.ordinal_position,
            })
            .collect()
    }
}

/// Schema inferrer that analyzes MongoDB documents to discover schema.
#[derive(Debug)]
pub struct SchemaInferrer {
    /// Field name -> (type counts, first seen position)
    field_info: HashMap<String, FieldStats>,
    /// Next ordinal position
    next_position: u32,
    /// Number of documents analyzed
    document_count: u32,
}

/// Statistics about a field collected during schema inference.
#[derive(Debug, Clone)]
struct FieldStats {
    /// Count of each type observed
    type_counts: HashMap<String, u32>,
    /// Position when field was first seen
    first_seen_position: u32,
    /// Total occurrences
    total_occurrences: u32,
    /// Sample value for type determination
    sample_value: Option<Bson>,
}

impl Default for SchemaInferrer {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaInferrer {
    /// Creates a new schema inferrer.
    pub fn new() -> Self {
        Self {
            field_info: HashMap::new(),
            next_position: 1,
            document_count: 0,
        }
    }

    /// Analyzes a document to discover its schema.
    ///
    /// # Arguments
    /// * `doc` - The MongoDB document to analyze
    pub fn analyze_document(&mut self, doc: &Document) {
        self.document_count = self.document_count.saturating_add(1);
        self.analyze_document_fields(doc, "");
    }

    /// Recursively analyzes document fields.
    fn analyze_document_fields(&mut self, doc: &Document, prefix: &str) {
        for (key, value) in doc {
            let field_name = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            // Record the field type
            self.record_field(&field_name, value);

            // Recursively analyze nested documents (but not arrays of documents)
            if let Bson::Document(nested_doc) = value {
                self.analyze_document_fields(nested_doc, &field_name);
            }
        }
    }

    /// Records a field occurrence with its type.
    fn record_field(&mut self, field_name: &str, value: &Bson) {
        let type_name = bson_type_name(value).to_string();

        let stats = self
            .field_info
            .entry(field_name.to_string())
            .or_insert_with(|| {
                let pos = self.next_position;
                self.next_position = self.next_position.saturating_add(1);
                FieldStats {
                    type_counts: HashMap::new(),
                    first_seen_position: pos,
                    total_occurrences: 0,
                    sample_value: None,
                }
            });

        *stats.type_counts.entry(type_name).or_insert(0) = stats
            .type_counts
            .get(bson_type_name(value))
            .unwrap_or(&0)
            .saturating_add(1);
        stats.total_occurrences = stats.total_occurrences.saturating_add(1);

        // Keep a sample value for type inference (prefer non-null)
        if stats.sample_value.is_none() || !matches!(value, Bson::Null) {
            stats.sample_value = Some(value.clone());
        }
    }

    /// Finalizes the schema inference and returns the result.
    ///
    /// # Arguments
    /// * `collection_name` - Name of the collection
    ///
    /// # Returns
    /// The inferred schema with all discovered fields
    pub fn finalize(self, collection_name: String) -> InferredSchema {
        let mut fields: Vec<InferredField> = self
            .field_info
            .into_iter()
            .map(|(name, stats)| {
                // Determine the primary type (most common non-null type)
                let observed_types: Vec<String> = stats.type_counts.keys().cloned().collect();

                // Get the most common type for unified type mapping
                let primary_type = stats
                    .type_counts
                    .iter()
                    .filter(|(k, _)| *k != "null")
                    .max_by_key(|(_, count)| *count)
                    .map(|(k, _)| k.as_str())
                    .unwrap_or("null");

                // Use sample value to determine unified type
                let unified_type = stats
                    .sample_value
                    .as_ref()
                    .filter(|v| !matches!(v, Bson::Null))
                    .map(map_bson_to_unified)
                    .unwrap_or_else(|| {
                        // Fallback based on type name
                        type_name_to_unified(primary_type)
                    });

                InferredField {
                    name,
                    observed_types,
                    unified_type,
                    occurrence_count: stats.total_occurrences,
                    is_nullable: stats.total_occurrences < self.document_count
                        || stats.type_counts.contains_key("null"),
                    ordinal_position: stats.first_seen_position,
                }
            })
            .collect();

        // Sort by ordinal position
        fields.sort_by_key(|f| f.ordinal_position);

        InferredSchema {
            collection_name,
            documents_sampled: self.document_count,
            fields,
        }
    }
}

/// Converts a BSON type name string to a unified data type.
fn type_name_to_unified(type_name: &str) -> UnifiedDataType {
    match type_name {
        "string" => UnifiedDataType::String { max_length: None },
        "int32" => UnifiedDataType::Integer {
            bits: 32,
            signed: true,
        },
        "int64" => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },
        "double" => UnifiedDataType::Float {
            precision: Some(53),
        },
        "bool" => UnifiedDataType::Boolean,
        "date" | "timestamp" => UnifiedDataType::DateTime {
            with_timezone: true,
        },
        "binData" => UnifiedDataType::Binary { max_length: None },
        "objectId" => UnifiedDataType::String {
            max_length: Some(24),
        },
        "object" => UnifiedDataType::Json,
        "array" => UnifiedDataType::Array {
            element_type: Box::new(UnifiedDataType::Custom {
                type_name: "unknown".to_string(),
            }),
        },
        "decimal" => UnifiedDataType::Float {
            precision: Some(128),
        },
        _ => UnifiedDataType::Custom {
            type_name: type_name.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{doc, oid::ObjectId};

    #[test]
    fn test_schema_inferrer_basic() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "name": "John",
            "age": 30
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("users".to_string());

        assert_eq!(schema.collection_name, "users");
        assert_eq!(schema.documents_sampled, 1);
        assert_eq!(schema.fields.len(), 3);

        // Check that _id is marked as primary key
        let id_field = schema.fields.iter().find(|f| f.name == "_id");
        assert!(id_field.is_some());
    }

    #[test]
    fn test_schema_inferrer_multiple_documents() {
        let mut inferrer = SchemaInferrer::new();

        let doc1 = doc! {
            "_id": ObjectId::new(),
            "name": "John",
            "age": 30
        };

        let doc2 = doc! {
            "_id": ObjectId::new(),
            "name": "Jane",
            "email": "jane@example.com"
        };

        inferrer.analyze_document(&doc1);
        inferrer.analyze_document(&doc2);
        let schema = inferrer.finalize("users".to_string());

        assert_eq!(schema.documents_sampled, 2);
        assert_eq!(schema.fields.len(), 4); // _id, name, age, email

        // Age should be nullable (not in second doc)
        let age_field = schema.fields.iter().find(|f| f.name == "age");
        assert!(age_field.is_some());
        assert!(age_field.as_ref().map(|f| f.is_nullable).unwrap_or(false));

        // Email should be nullable (not in first doc)
        let email_field = schema.fields.iter().find(|f| f.name == "email");
        assert!(email_field.is_some());
        assert!(email_field.as_ref().map(|f| f.is_nullable).unwrap_or(false));
    }

    #[test]
    fn test_schema_inferrer_nested_document() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "profile": {
                "firstName": "John",
                "lastName": "Doe"
            }
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("users".to_string());

        // Should have _id, profile, profile.firstName, profile.lastName
        assert!(schema.fields.iter().any(|f| f.name == "profile"));
        assert!(schema.fields.iter().any(|f| f.name == "profile.firstName"));
        assert!(schema.fields.iter().any(|f| f.name == "profile.lastName"));
    }

    #[test]
    fn test_schema_inferrer_array_field() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "tags": ["rust", "mongodb", "database"]
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("articles".to_string());

        let tags_field = schema.fields.iter().find(|f| f.name == "tags");
        assert!(tags_field.is_some());
        assert!(matches!(
            &tags_field.as_ref().map(|f| &f.unified_type),
            Some(UnifiedDataType::Array { .. })
        ));
    }

    #[test]
    fn test_schema_inferrer_mixed_types() {
        let mut inferrer = SchemaInferrer::new();

        let doc1 = doc! {
            "_id": ObjectId::new(),
            "value": 42
        };

        let doc2 = doc! {
            "_id": ObjectId::new(),
            "value": "forty-two"
        };

        inferrer.analyze_document(&doc1);
        inferrer.analyze_document(&doc2);
        let schema = inferrer.finalize("mixed".to_string());

        let value_field = schema.fields.iter().find(|f| f.name == "value");
        assert!(value_field.is_some());
        // Should have multiple observed types
        assert!(
            value_field
                .as_ref()
                .map(|f| f.observed_types.len() > 1)
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_to_columns() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "name": "Test",
            "count": 42
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("test".to_string());
        let columns = schema.to_columns();

        assert_eq!(columns.len(), 3);

        // _id should be primary key
        let id_col = columns.iter().find(|c| c.name == "_id");
        assert!(id_col.is_some());
        assert!(id_col.as_ref().map(|c| c.is_primary_key).unwrap_or(false));
    }

    #[test]
    fn test_type_name_to_unified() {
        assert!(matches!(
            type_name_to_unified("string"),
            UnifiedDataType::String { max_length: None }
        ));
        assert!(matches!(
            type_name_to_unified("int32"),
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
        assert!(matches!(
            type_name_to_unified("bool"),
            UnifiedDataType::Boolean
        ));
        assert!(matches!(
            type_name_to_unified("objectId"),
            UnifiedDataType::String {
                max_length: Some(24)
            }
        ));
    }
}
