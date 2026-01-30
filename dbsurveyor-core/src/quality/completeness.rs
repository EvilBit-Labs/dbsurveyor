//! Completeness analysis for data quality assessment.
//!
//! This module analyzes null and empty value distribution in sampled data
//! to calculate completeness metrics.

use crate::models::TableSample;

use super::models::{ColumnCompleteness, CompletenessMetrics};

/// Analyzes completeness of sampled data.
///
/// Completeness measures the presence of values - identifying null and empty
/// values that may indicate data quality issues.
///
/// # Note
/// Column names are derived from the first row only. Any columns that appear
/// exclusively in subsequent rows will not be analyzed. This is acceptable for
/// quality analysis on sampled data with consistent schemas.
pub fn analyze_completeness(sample: &TableSample) -> CompletenessMetrics {
    if sample.rows.is_empty() {
        return CompletenessMetrics::default();
    }

    let total_rows = sample.rows.len() as u64;
    let mut column_metrics: Vec<ColumnCompleteness> = Vec::new();
    let mut total_nulls: u64 = 0;
    let mut total_empty: u64 = 0;

    // Get column names from first row
    let column_names: Vec<String> = if let Some(first_row) = sample.rows.first() {
        if let Some(obj) = first_row.as_object() {
            obj.keys().cloned().collect()
        } else {
            return CompletenessMetrics::default();
        }
    } else {
        return CompletenessMetrics::default();
    };

    // Analyze each column
    for column_name in &column_names {
        let mut null_count: u64 = 0;
        let mut empty_count: u64 = 0;

        for row in &sample.rows {
            if let Some(obj) = row.as_object() {
                match obj.get(column_name) {
                    None | Some(serde_json::Value::Null) => {
                        null_count += 1;
                    }
                    Some(serde_json::Value::String(s)) if s.is_empty() => {
                        empty_count += 1;
                    }
                    _ => {}
                }
            }
        }

        total_nulls += null_count;
        total_empty += empty_count;

        column_metrics.push(ColumnCompleteness::new(
            column_name,
            null_count,
            empty_count,
            total_rows,
        ));
    }

    // Calculate overall score as average of column completeness
    let score = if column_metrics.is_empty() {
        1.0
    } else {
        column_metrics.iter().map(|c| c.completeness).sum::<f64>() / column_metrics.len() as f64
    };

    CompletenessMetrics {
        score,
        column_metrics,
        total_nulls,
        total_empty,
    }
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
    fn test_completeness_all_present() {
        let rows = vec![
            json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
            json!({"id": 2, "name": "Bob", "email": "bob@example.com"}),
            json!({"id": 3, "name": "Charlie", "email": "charlie@example.com"}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert!((metrics.score - 1.0).abs() < 0.001);
        assert_eq!(metrics.total_nulls, 0);
        assert_eq!(metrics.total_empty, 0);
        assert_eq!(metrics.column_metrics.len(), 3);
    }

    #[test]
    fn test_completeness_with_nulls() {
        let rows = vec![
            json!({"id": 1, "name": "Alice", "email": null}),
            json!({"id": 2, "name": null, "email": "bob@example.com"}),
            json!({"id": 3, "name": "Charlie", "email": "charlie@example.com"}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert!(metrics.score < 1.0);
        assert_eq!(metrics.total_nulls, 2);
        assert_eq!(metrics.total_empty, 0);

        // Find email column
        let email_col = metrics
            .column_metrics
            .iter()
            .find(|c| c.column_name == "email")
            .unwrap();
        assert_eq!(email_col.null_count, 1);
        assert!((email_col.completeness - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_completeness_with_empty_strings() {
        let rows = vec![
            json!({"id": 1, "name": "Alice", "email": ""}),
            json!({"id": 2, "name": "", "email": "bob@example.com"}),
            json!({"id": 3, "name": "Charlie", "email": ""}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert!(metrics.score < 1.0);
        assert_eq!(metrics.total_nulls, 0);
        assert_eq!(metrics.total_empty, 3);

        let email_col = metrics
            .column_metrics
            .iter()
            .find(|c| c.column_name == "email")
            .unwrap();
        assert_eq!(email_col.empty_count, 2);
    }

    #[test]
    fn test_completeness_mixed_nulls_and_empty() {
        let rows = vec![
            json!({"id": 1, "name": null, "email": ""}),
            json!({"id": 2, "name": "", "email": null}),
            json!({"id": 3, "name": "Charlie", "email": "charlie@example.com"}),
            json!({"id": 4, "name": "Dave", "email": "dave@example.com"}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert_eq!(metrics.total_nulls, 2);
        assert_eq!(metrics.total_empty, 2);
    }

    #[test]
    fn test_completeness_empty_sample() {
        let metrics = analyze_completeness(&create_sample(vec![]));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.column_metrics.is_empty());
        assert_eq!(metrics.total_nulls, 0);
        assert_eq!(metrics.total_empty, 0);
    }

    #[test]
    fn test_completeness_all_nulls() {
        let rows = vec![
            json!({"id": null, "name": null}),
            json!({"id": null, "name": null}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert_eq!(metrics.score, 0.0);
        assert_eq!(metrics.total_nulls, 4);
    }

    #[test]
    fn test_completeness_missing_keys() {
        // JSON object with missing key is treated as null
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2}), // missing "name" key
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        // The first row has both keys, we use first row's keys
        // Second row doesn't have "name", which counts as null
        let name_col = metrics
            .column_metrics
            .iter()
            .find(|c| c.column_name == "name")
            .unwrap();
        assert_eq!(name_col.null_count, 1);
    }

    #[test]
    fn test_completeness_non_object_row() {
        // First row is not an object - should return default metrics
        let rows = vec![json!([1, 2, 3]), json!([4, 5, 6])];

        let metrics = analyze_completeness(&create_sample(rows));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.column_metrics.is_empty());
    }

    #[test]
    fn test_completeness_single_column() {
        let rows = vec![
            json!({"value": 1}),
            json!({"value": 2}),
            json!({"value": 3}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        assert_eq!(metrics.column_metrics.len(), 1);
        assert_eq!(metrics.column_metrics[0].column_name, "value");
        assert!((metrics.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_completeness_whitespace_not_empty() {
        // Whitespace-only strings are NOT considered empty (only "" is)
        let rows = vec![
            json!({"name": "  "}),
            json!({"name": "\t"}),
            json!({"name": "valid"}),
        ];

        let metrics = analyze_completeness(&create_sample(rows));

        // Whitespace strings are considered present
        assert_eq!(metrics.total_empty, 0);
        assert!((metrics.score - 1.0).abs() < 0.001);
    }
}
