//! Anomaly detection for data quality assessment.
//!
//! This module provides statistical outlier detection using z-score
//! analysis on numeric columns.

use crate::models::TableSample;

use super::config::AnomalySensitivity;
use super::models::{AnomalyMetrics, ColumnAnomaly};

/// Analyzes anomalies in sampled data using statistical methods.
///
/// Anomaly detection identifies statistical outliers in numeric columns
/// using z-score analysis with configurable sensitivity thresholds.
///
/// # Arguments
/// * `sample` - The table sample to analyze
/// * `sensitivity` - The sensitivity level for outlier detection
///
/// # Returns
/// Anomaly metrics containing detected outliers per column.
pub fn analyze_anomalies(sample: &TableSample, sensitivity: AnomalySensitivity) -> AnomalyMetrics {
    let column_names = match sample.column_names() {
        Some(names) => names,
        None => return AnomalyMetrics::default(),
    };

    let z_threshold = sensitivity.z_score_threshold();
    let mut outliers: Vec<ColumnAnomaly> = Vec::new();
    let mut total_outlier_count: u64 = 0;

    // Analyze each column for numeric outliers
    for column_name in &column_names {
        // Extract numeric values from this column
        let numeric_values: Vec<f64> = sample
            .rows
            .iter()
            .filter_map(|row| {
                row.as_object()
                    .and_then(|obj| obj.get(column_name))
                    .and_then(extract_numeric)
            })
            .collect();

        // Need at least 3 values for meaningful statistics
        if numeric_values.len() < 3 {
            continue;
        }

        // Calculate mean and standard deviation
        let (mean, std_dev) = calculate_statistics(&numeric_values);

        // Skip if std_dev is too small (all values are nearly identical)
        if std_dev < 1e-10 {
            continue;
        }

        // Count outliers using z-score
        let outlier_count = numeric_values
            .iter()
            .filter(|&&value| {
                let z_score = (value - mean).abs() / std_dev;
                z_score > z_threshold
            })
            .count() as u64;

        if outlier_count > 0 {
            total_outlier_count += outlier_count;
            outliers.push(ColumnAnomaly {
                column_name: column_name.clone(),
                outlier_count,
                z_score_threshold: z_threshold,
                mean,
                std_dev,
            });
        }
    }

    AnomalyMetrics {
        outlier_count: total_outlier_count,
        outliers,
    }
}

/// Extracts a finite numeric value from a JSON value.
///
/// Only finite values are accepted. String representations of non-finite
/// numbers such as "NaN" or "inf" are rejected to avoid poisoning
/// statistical calculations with non-finite values.
fn extract_numeric(value: &serde_json::Value) -> Option<f64> {
    let numeric = match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    match numeric {
        Some(v) if v.is_finite() => Some(v),
        _ => None,
    }
}

/// Calculates mean and population standard deviation for a set of values.
///
/// Uses population standard deviation (divides by n, not n-1). For quality
/// analysis on sampled data this is acceptable -- the bias is minimal and
/// tends to be slightly conservative in outlier detection.
fn calculate_statistics(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let n = values.len() as f64;

    // Calculate mean
    let mean = values.iter().sum::<f64>() / n;

    // Calculate standard deviation
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    (mean, std_dev)
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
    fn test_anomaly_no_outliers() {
        // Normal distribution-like data with no outliers
        let rows = vec![
            json!({"id": 1, "value": 50}),
            json!({"id": 2, "value": 52}),
            json!({"id": 3, "value": 48}),
            json!({"id": 4, "value": 51}),
            json!({"id": 5, "value": 49}),
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
        assert!(metrics.outliers.is_empty());
    }

    #[test]
    fn test_anomaly_with_outliers() {
        // Data with a clear outlier - need more data points and extreme outlier
        // to exceed z-score threshold of 2.5 (medium sensitivity)
        let rows = vec![
            json!({"id": 1, "value": 10}),
            json!({"id": 2, "value": 10}),
            json!({"id": 3, "value": 10}),
            json!({"id": 4, "value": 10}),
            json!({"id": 5, "value": 10}),
            json!({"id": 6, "value": 10}),
            json!({"id": 7, "value": 10}),
            json!({"id": 8, "value": 10}),
            json!({"id": 9, "value": 10}),
            json!({"id": 10, "value": 1000}), // extreme outlier
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert!(metrics.outlier_count > 0);
        assert!(!metrics.outliers.is_empty());

        let value_anomaly = metrics
            .outliers
            .iter()
            .find(|a| a.column_name == "value")
            .unwrap();
        assert!(value_anomaly.outlier_count >= 1);
    }

    #[test]
    fn test_anomaly_sensitivity_levels() {
        // Data where sensitivity level matters
        let rows = vec![
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 25}), // Moderate outlier
        ];

        // High sensitivity should detect more
        let high_metrics =
            analyze_anomalies(&create_sample(rows.clone()), AnomalySensitivity::High);

        // Low sensitivity should detect fewer
        let low_metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Low);

        // High sensitivity uses z=2.0, low uses z=3.0
        // The outlier at 25 may be detected at high but not low
        assert!(high_metrics.outlier_count >= low_metrics.outlier_count);
    }

    #[test]
    fn test_anomaly_empty_sample() {
        let metrics = analyze_anomalies(&create_sample(vec![]), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
        assert!(metrics.outliers.is_empty());
    }

    #[test]
    fn test_anomaly_non_numeric_column() {
        // Non-numeric columns should be skipped
        let rows = vec![
            json!({"name": "Alice"}),
            json!({"name": "Bob"}),
            json!({"name": "Charlie"}),
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
        assert!(metrics.outliers.is_empty());
    }

    #[test]
    fn test_anomaly_string_numbers() {
        // Numbers stored as strings should still be analyzed
        // Need more data points and extreme outlier for z-score detection
        let rows = vec![
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "10"}),
            json!({"value": "1000"}), // extreme outlier
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        // Should detect the outlier in string numbers
        assert!(metrics.outlier_count > 0);
    }

    #[test]
    fn test_anomaly_insufficient_data() {
        // Less than 3 values - should skip analysis
        let rows = vec![json!({"value": 10}), json!({"value": 100})];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
    }

    #[test]
    fn test_anomaly_identical_values() {
        // All identical values - std_dev = 0, should skip
        let rows = vec![
            json!({"value": 42}),
            json!({"value": 42}),
            json!({"value": 42}),
            json!({"value": 42}),
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
    }

    #[test]
    fn test_anomaly_non_object_row() {
        // First row is not an object - should return default metrics
        let rows = vec![json!([1, 2, 3]), json!([4, 5, 6])];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert_eq!(metrics.outlier_count, 0);
        assert!(metrics.outliers.is_empty());
    }

    #[test]
    fn test_anomaly_multiple_numeric_columns() {
        // Multiple columns with outliers
        let rows = vec![
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 10, "b": 100}),
            json!({"a": 1000, "b": 10000}), // outliers in both columns
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        // Should detect outliers in both columns
        assert!(metrics.outlier_count >= 2);
        assert!(metrics.outliers.len() >= 2);
    }

    #[test]
    fn test_anomaly_negative_values() {
        // Test with negative values and negative outlier
        let rows = vec![
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -10}),
            json!({"value": -1000}), // negative outlier
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        assert!(metrics.outlier_count > 0);
    }

    #[test]
    fn test_anomaly_non_finite_values_rejected() {
        // NaN and Infinity strings should be rejected, not poison statistics
        let rows = vec![
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": "NaN"}),
            json!({"value": "inf"}),
            json!({"value": "-inf"}),
            json!({"value": "Infinity"}),
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        // Should not detect outliers -- the three finite values are identical
        assert_eq!(metrics.outlier_count, 0);
    }

    #[test]
    fn test_anomaly_mixed_numeric_and_non_numeric() {
        // Some values are numeric, some are not
        let rows = vec![
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": "not a number"}), // skipped
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": null}), // skipped
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 1000}), // outlier
        ];

        let metrics = analyze_anomalies(&create_sample(rows), AnomalySensitivity::Medium);

        // Should still detect outlier among numeric values
        assert!(metrics.outlier_count > 0);
    }
}
