//! Uniqueness analysis for data quality assessment.
//!
//! This module analyzes duplicate values at both column and row level
//! to assess data uniqueness and integrity.

use std::collections::{BTreeMap, HashSet};

use crate::models::TableSample;

use super::models::{ColumnDuplicates, UniquenessMetrics};

/// Analyzes uniqueness of sampled data.
///
/// Uniqueness measures the presence of duplicate values, both at the
/// individual column level and for complete rows.
pub fn analyze_uniqueness(sample: &TableSample) -> UniquenessMetrics {
    if sample.rows.is_empty() {
        return UniquenessMetrics::default();
    }

    let total_rows = sample.rows.len() as u64;
    let mut duplicate_columns: Vec<ColumnDuplicates> = Vec::new();

    // Get column names from first row
    let column_names: Vec<String> = if let Some(first_row) = sample.rows.first() {
        if let Some(obj) = first_row.as_object() {
            obj.keys().cloned().collect()
        } else {
            return UniquenessMetrics::default();
        }
    } else {
        return UniquenessMetrics::default();
    };

    // Analyze column-level uniqueness
    for column_name in &column_names {
        let mut seen_values: HashSet<String> = HashSet::new();
        let mut duplicate_count: u64 = 0;

        for row in &sample.rows {
            if let Some(value) = row.as_object().and_then(|obj| obj.get(column_name)) {
                // Convert value to string for comparison
                // Nulls are treated as a single value
                let value_str = value_to_string(value);

                if seen_values.contains(&value_str) {
                    duplicate_count += 1;
                } else {
                    seen_values.insert(value_str);
                }
            }
        }

        // Only report columns with duplicates
        if duplicate_count > 0 {
            duplicate_columns.push(ColumnDuplicates::new(
                column_name,
                duplicate_count,
                total_rows,
            ));
        }
    }

    // Analyze row-level uniqueness (exact row matches)
    let duplicate_row_count = count_duplicate_rows(&sample.rows);

    // Calculate overall uniqueness score
    // Use the minimum column uniqueness as the overall score
    let column_uniqueness_scores: Vec<f64> = if duplicate_columns.is_empty() {
        vec![1.0]
    } else {
        duplicate_columns.iter().map(|c| c.uniqueness).collect()
    };

    let row_uniqueness = if total_rows == 0 {
        1.0
    } else {
        (total_rows - duplicate_row_count) as f64 / total_rows as f64
    };

    // Overall score is the minimum of row uniqueness and average column uniqueness
    let avg_column_uniqueness =
        column_uniqueness_scores.iter().sum::<f64>() / column_uniqueness_scores.len() as f64;
    let score = row_uniqueness.min(avg_column_uniqueness);

    UniquenessMetrics {
        score,
        duplicate_columns,
        duplicate_row_count,
    }
}

/// Converts a JSON value to a comparable string representation.
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "__NULL__".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => serde_json::to_string(a).unwrap_or_else(|e| {
            tracing::trace!("Failed to serialize array for uniqueness comparison: {}", e);
            "__SERIALIZE_ERROR__".to_string()
        }),
        serde_json::Value::Object(o) => serde_json::to_string(o).unwrap_or_else(|e| {
            tracing::trace!(
                "Failed to serialize object for uniqueness comparison: {}",
                e
            );
            "__SERIALIZE_ERROR__".to_string()
        }),
    }
}

/// Counts the number of duplicate rows in the sample.
///
/// Normalizes JSON object key ordering via `BTreeMap` so that rows with
/// identical key-value pairs but different insertion order are correctly
/// identified as duplicates.
fn count_duplicate_rows(rows: &[serde_json::Value]) -> u64 {
    let mut seen_rows: HashSet<String> = HashSet::new();
    let mut duplicate_count: u64 = 0;

    for row in rows {
        let row_str = if let Some(obj) = row.as_object() {
            let sorted: BTreeMap<_, _> = obj.iter().collect();
            serde_json::to_string(&sorted).unwrap_or_default()
        } else {
            serde_json::to_string(row).unwrap_or_default()
        };

        if seen_rows.contains(&row_str) {
            duplicate_count += 1;
        } else {
            seen_rows.insert(row_str);
        }
    }

    duplicate_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SamplingStrategy;
    use serde_json::json;

    fn create_sample(rows: Vec<serde_json::Value>) -> TableSample {
        TableSample {
            table_name: "test_table".to_string(),
            schema_name: Some("public".to_string()),
            rows,
            sample_size: 10,
            total_rows: Some(100),
            sampling_strategy: SamplingStrategy::MostRecent { limit: 10 },
            collected_at: chrono::Utc::now(),
            warnings: vec![],
        }
    }

    #[test]
    fn test_uniqueness_all_unique() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
            json!({"id": 3, "name": "Charlie"}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        assert!((metrics.score - 1.0).abs() < 0.001);
        assert!(metrics.duplicate_columns.is_empty());
        assert_eq!(metrics.duplicate_row_count, 0);
    }

    #[test]
    fn test_uniqueness_with_column_duplicates() {
        let rows = vec![
            json!({"id": 1, "status": "active"}),
            json!({"id": 2, "status": "active"}), // duplicate status
            json!({"id": 3, "status": "inactive"}),
            json!({"id": 4, "status": "active"}), // duplicate status
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        assert!(metrics.score < 1.0);
        assert_eq!(metrics.duplicate_columns.len(), 1);

        let status_col = &metrics.duplicate_columns[0];
        assert_eq!(status_col.column_name, "status");
        assert_eq!(status_col.duplicate_count, 2); // 2 duplicates of "active"
    }

    #[test]
    fn test_uniqueness_with_row_duplicates() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 1, "name": "Alice"}), // exact row duplicate
            json!({"id": 2, "name": "Bob"}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        assert!(metrics.score < 1.0);
        assert_eq!(metrics.duplicate_row_count, 1);
    }

    #[test]
    fn test_uniqueness_null_values() {
        // Multiple nulls should be counted as duplicates
        let rows = vec![
            json!({"id": 1, "email": null}),
            json!({"id": 2, "email": null}), // null duplicate
            json!({"id": 3, "email": "test@example.com"}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        let email_col = metrics
            .duplicate_columns
            .iter()
            .find(|c| c.column_name == "email");
        assert!(email_col.is_some());
        assert_eq!(email_col.unwrap().duplicate_count, 1);
    }

    #[test]
    fn test_uniqueness_empty_sample() {
        let metrics = analyze_uniqueness(&create_sample(vec![]));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.duplicate_columns.is_empty());
        assert_eq!(metrics.duplicate_row_count, 0);
    }

    #[test]
    fn test_uniqueness_all_duplicates() {
        let rows = vec![
            json!({"id": 1, "name": "Same"}),
            json!({"id": 1, "name": "Same"}),
            json!({"id": 1, "name": "Same"}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        // All rows are duplicates of the first
        assert_eq!(metrics.duplicate_row_count, 2);
        // Both columns have duplicates
        assert_eq!(metrics.duplicate_columns.len(), 2);
    }

    #[test]
    fn test_uniqueness_mixed_types() {
        // Different JSON types with different string representations are unique
        // Note: json!(1) and json!("1") both convert to "1", so would be duplicates
        // Here we use values that have distinct string representations
        let rows = vec![
            json!({"value": 42}),
            json!({"value": "hello"}),
            json!({"value": true}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        // All values are unique (42, "hello", and "true" are different strings)
        assert!(metrics.duplicate_columns.is_empty());
    }

    #[test]
    fn test_value_to_string() {
        assert_eq!(value_to_string(&json!(null)), "__NULL__");
        assert_eq!(value_to_string(&json!(true)), "true");
        assert_eq!(value_to_string(&json!(42)), "42");
        assert_eq!(value_to_string(&json!("hello")), "hello");
    }

    #[test]
    fn test_count_duplicate_rows() {
        let rows = vec![
            json!({"a": 1}),
            json!({"a": 2}),
            json!({"a": 1}), // duplicate
            json!({"a": 1}), // duplicate
        ];

        assert_eq!(count_duplicate_rows(&rows), 2);
    }

    #[test]
    fn test_uniqueness_non_object_row() {
        // First row is not an object - should return default metrics
        let rows = vec![json!([1, 2, 3]), json!([4, 5, 6])];

        let metrics = analyze_uniqueness(&create_sample(rows));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.duplicate_columns.is_empty());
    }

    #[test]
    fn test_uniqueness_array_values() {
        // Array values should be serialized for comparison
        let rows = vec![
            json!({"data": [1, 2, 3]}),
            json!({"data": [1, 2, 3]}), // duplicate array
            json!({"data": [4, 5, 6]}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        let data_col = metrics
            .duplicate_columns
            .iter()
            .find(|c| c.column_name == "data");
        assert!(data_col.is_some());
        assert_eq!(data_col.unwrap().duplicate_count, 1);
    }

    #[test]
    fn test_uniqueness_object_values() {
        // Object values should be serialized for comparison
        let rows = vec![
            json!({"meta": {"key": "value"}}),
            json!({"meta": {"key": "value"}}), // duplicate object
            json!({"meta": {"key": "other"}}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        let meta_col = metrics
            .duplicate_columns
            .iter()
            .find(|c| c.column_name == "meta");
        assert!(meta_col.is_some());
        assert_eq!(meta_col.unwrap().duplicate_count, 1);
    }

    #[test]
    fn test_uniqueness_boolean_values() {
        // Boolean values are common duplicates
        let rows = vec![
            json!({"active": true}),
            json!({"active": true}),
            json!({"active": false}),
            json!({"active": true}),
        ];

        let metrics = analyze_uniqueness(&create_sample(rows));

        let active_col = metrics
            .duplicate_columns
            .iter()
            .find(|c| c.column_name == "active");
        assert!(active_col.is_some());
        assert_eq!(active_col.unwrap().duplicate_count, 2); // 2 extra true values
    }

    #[test]
    fn test_uniqueness_single_value() {
        // Single row should have perfect uniqueness
        let rows = vec![json!({"id": 1, "name": "Alice"})];

        let metrics = analyze_uniqueness(&create_sample(rows));

        assert!((metrics.score - 1.0).abs() < 0.001);
        assert!(metrics.duplicate_columns.is_empty());
        assert_eq!(metrics.duplicate_row_count, 0);
    }
}
