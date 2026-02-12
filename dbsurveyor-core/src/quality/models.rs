//! Data quality metrics models.
//!
//! This module defines the data structures for quality assessment results.
//! All metrics are designed to be safe for output - they contain only
//! counts and ratios, never actual data values.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Severity level for threshold violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViolationSeverity {
    /// Metric is below threshold but not critical
    Warning,
    /// Metric is significantly below threshold
    Critical,
}

/// A threshold violation detected during quality analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    /// Name of the metric that violated threshold
    pub metric: String,
    /// The configured threshold value
    pub threshold: f64,
    /// The actual measured value
    pub actual: f64,
    /// Severity of the violation
    pub severity: ViolationSeverity,
}

/// Values below this fraction of the threshold are classified as critical.
const CRITICAL_SEVERITY_RATIO: f64 = 0.8;

impl ThresholdViolation {
    /// Creates a new threshold violation.
    ///
    /// # Severity Classification
    /// - Critical: actual value is below 80% of threshold
    /// - Warning: actual value is between 80% and 100% of threshold
    pub fn new(metric: impl Into<String>, threshold: f64, actual: f64) -> Self {
        let severity = if actual < threshold * CRITICAL_SEVERITY_RATIO {
            ViolationSeverity::Critical
        } else {
            ViolationSeverity::Warning
        };

        Self {
            metric: metric.into(),
            threshold,
            actual,
            severity,
        }
    }
}

/// Completeness metrics for a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnCompleteness {
    /// Column name
    pub column_name: String,
    /// Count of null values
    pub null_count: u64,
    /// Count of empty string values
    pub empty_count: u64,
    /// Completeness ratio (0.0-1.0)
    pub completeness: f64,
}

impl ColumnCompleteness {
    /// Creates new column completeness metrics.
    pub fn new(
        column_name: impl Into<String>,
        null_count: u64,
        empty_count: u64,
        total: u64,
    ) -> Self {
        let column_name = column_name.into();

        // Diagnostic for anomalous input that could indicate upstream issues
        let combined = null_count.saturating_add(empty_count);
        if combined > total {
            tracing::warn!(
                "Quality metrics anomaly: null_count ({}) + empty_count ({}) exceeds total ({}) for column '{}'",
                null_count,
                empty_count,
                total,
                column_name
            );
        }

        let completeness = if total == 0 {
            1.0
        } else {
            (total.saturating_sub(combined)) as f64 / total as f64
        };

        Self {
            column_name,
            null_count,
            empty_count,
            completeness: completeness.clamp(0.0, 1.0),
        }
    }
}

/// Overall completeness metrics for a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletenessMetrics {
    /// Overall completeness score (0.0-1.0)
    pub score: f64,
    /// Per-column completeness metrics
    pub column_metrics: Vec<ColumnCompleteness>,
    /// Total null values across all columns
    pub total_nulls: u64,
    /// Total empty string values across all columns
    pub total_empty: u64,
}

impl Default for CompletenessMetrics {
    fn default() -> Self {
        Self {
            score: 1.0,
            column_metrics: Vec::new(),
            total_nulls: 0,
            total_empty: 0,
        }
    }
}

/// Type inconsistency detected in a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeInconsistency {
    /// Column name
    pub column_name: String,
    /// Expected/dominant type
    pub expected_type: String,
    /// Other types found
    pub found_types: Vec<String>,
    /// Count of inconsistent values
    pub inconsistent_count: u64,
}

/// Format violation detected in a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatViolation {
    /// Column name
    pub column_name: String,
    /// Expected format pattern name
    pub expected_format: String,
    /// Count of values not matching the format
    pub violation_count: u64,
}

/// Consistency metrics for a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyMetrics {
    /// Overall consistency score (0.0-1.0)
    pub score: f64,
    /// Columns with mixed type values
    pub type_inconsistencies: Vec<TypeInconsistency>,
    /// Format pattern violations
    pub format_violations: Vec<FormatViolation>,
}

impl Default for ConsistencyMetrics {
    fn default() -> Self {
        Self {
            score: 1.0,
            type_inconsistencies: Vec::new(),
            format_violations: Vec::new(),
        }
    }
}

/// Duplicate information for a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDuplicates {
    /// Column name
    pub column_name: String,
    /// Count of duplicate values (not unique)
    pub duplicate_count: u64,
    /// Total unique values
    pub unique_count: u64,
    /// Uniqueness ratio (0.0-1.0)
    pub uniqueness: f64,
}

impl ColumnDuplicates {
    /// Creates new column duplicates metrics.
    pub fn new(column_name: impl Into<String>, duplicate_count: u64, total: u64) -> Self {
        let column_name = column_name.into();

        if duplicate_count > total {
            tracing::warn!(
                "Quality metrics anomaly: duplicate_count ({}) exceeds total ({}) for column '{}'",
                duplicate_count,
                total,
                column_name
            );
        }

        let unique_count = total.saturating_sub(duplicate_count);
        let uniqueness = if total == 0 {
            1.0
        } else {
            unique_count as f64 / total as f64
        };

        Self {
            column_name,
            duplicate_count,
            unique_count,
            uniqueness: uniqueness.clamp(0.0, 1.0),
        }
    }
}

/// Uniqueness metrics for a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniquenessMetrics {
    /// Overall uniqueness score (0.0-1.0)
    pub score: f64,
    /// Columns with duplicate values
    pub duplicate_columns: Vec<ColumnDuplicates>,
    /// Total duplicate rows (exact row matches)
    pub duplicate_row_count: u64,
}

impl Default for UniquenessMetrics {
    fn default() -> Self {
        Self {
            score: 1.0,
            duplicate_columns: Vec::new(),
            duplicate_row_count: 0,
        }
    }
}

/// Anomaly detected in a column.
///
/// The `mean` and `std_dev` fields are statistical aggregates that do not
/// expose individual data values. However, for highly sensitive numeric
/// columns (e.g., salaries, transaction amounts), these aggregates may
/// reveal distribution characteristics. Operators working with sensitive
/// data should be aware of this trade-off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnAnomaly {
    /// Column name
    pub column_name: String,
    /// Number of outliers detected
    pub outlier_count: u64,
    /// Z-score threshold used for detection
    pub z_score_threshold: f64,
    /// Mean value (statistical aggregate, not actual data)
    pub mean: f64,
    /// Standard deviation (statistical aggregate, not actual data)
    pub std_dev: f64,
}

/// Anomaly detection metrics for a table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnomalyMetrics {
    /// Total outliers detected
    pub outlier_count: u64,
    /// Per-column anomaly details
    pub outliers: Vec<ColumnAnomaly>,
}

/// Complete quality metrics for a single table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableQualityMetrics {
    /// Table name
    pub table_name: String,
    /// Schema name (if applicable)
    pub schema_name: Option<String>,
    /// Number of rows analyzed
    pub analyzed_rows: u64,
    /// Completeness metrics
    pub completeness: CompletenessMetrics,
    /// Consistency metrics
    pub consistency: ConsistencyMetrics,
    /// Uniqueness metrics
    pub uniqueness: UniquenessMetrics,
    /// Anomaly metrics (if enabled)
    pub anomalies: Option<AnomalyMetrics>,
    /// Overall quality score (0.0-1.0)
    pub quality_score: f64,
    /// Threshold violations detected
    pub threshold_violations: Vec<ThresholdViolation>,
    /// Timestamp when analysis was performed
    pub analyzed_at: DateTime<Utc>,
}

impl TableQualityMetrics {
    /// Creates a new TableQualityMetrics with the given parameters.
    pub fn new(
        table_name: impl Into<String>,
        schema_name: Option<String>,
        analyzed_rows: u64,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            schema_name,
            analyzed_rows,
            completeness: CompletenessMetrics::default(),
            consistency: ConsistencyMetrics::default(),
            uniqueness: UniquenessMetrics::default(),
            anomalies: None,
            quality_score: 1.0,
            threshold_violations: Vec::new(),
            analyzed_at: Utc::now(),
        }
    }

    /// Sets the completeness metrics.
    pub fn with_completeness(mut self, metrics: CompletenessMetrics) -> Self {
        self.completeness = metrics;
        self
    }

    /// Sets the consistency metrics.
    pub fn with_consistency(mut self, metrics: ConsistencyMetrics) -> Self {
        self.consistency = metrics;
        self
    }

    /// Sets the uniqueness metrics.
    pub fn with_uniqueness(mut self, metrics: UniquenessMetrics) -> Self {
        self.uniqueness = metrics;
        self
    }

    /// Sets the anomaly metrics.
    pub fn with_anomalies(mut self, metrics: AnomalyMetrics) -> Self {
        self.anomalies = Some(metrics);
        self
    }

    /// Sets the overall quality score.
    pub fn with_quality_score(mut self, score: f64) -> Self {
        self.quality_score = score.clamp(0.0, 1.0);
        self
    }

    /// Adds a threshold violation.
    pub fn add_violation(mut self, violation: ThresholdViolation) -> Self {
        self.threshold_violations.push(violation);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_violation_warning_severity() {
        let violation = ThresholdViolation::new("completeness", 0.95, 0.90);
        assert_eq!(violation.severity, ViolationSeverity::Warning);
    }

    #[test]
    fn test_threshold_violation_critical_severity() {
        let violation = ThresholdViolation::new("completeness", 0.95, 0.70);
        assert_eq!(violation.severity, ViolationSeverity::Critical);
    }

    #[test]
    fn test_threshold_violation_boundary_severity() {
        // Exactly at 80% of threshold boundary - should be Warning, not Critical
        let violation = ThresholdViolation::new("completeness", 0.95, 0.76);
        assert_eq!(violation.severity, ViolationSeverity::Warning);

        // Just below 80% - should be Critical
        let violation = ThresholdViolation::new("completeness", 0.95, 0.759);
        assert_eq!(violation.severity, ViolationSeverity::Critical);
    }

    #[test]
    fn test_column_completeness_calculation() {
        let metrics = ColumnCompleteness::new("email", 5, 3, 100);
        assert_eq!(metrics.null_count, 5);
        assert_eq!(metrics.empty_count, 3);
        assert!((metrics.completeness - 0.92).abs() < 0.001);
    }

    #[test]
    fn test_column_completeness_empty_table() {
        let metrics = ColumnCompleteness::new("email", 0, 0, 0);
        assert_eq!(metrics.completeness, 1.0);
    }

    #[test]
    fn test_column_duplicates_calculation() {
        let metrics = ColumnDuplicates::new("status", 20, 100);
        assert_eq!(metrics.duplicate_count, 20);
        assert_eq!(metrics.unique_count, 80);
        assert!((metrics.uniqueness - 0.80).abs() < 0.001);
    }

    #[test]
    fn test_column_duplicates_empty_table() {
        let metrics = ColumnDuplicates::new("status", 0, 0);
        assert_eq!(metrics.uniqueness, 1.0);
    }

    #[test]
    fn test_table_quality_metrics_builder() {
        let metrics = TableQualityMetrics::new("users", Some("public".to_string()), 100)
            .with_quality_score(0.85)
            .add_violation(ThresholdViolation::new("completeness", 0.95, 0.90));

        assert_eq!(metrics.table_name, "users");
        assert_eq!(metrics.schema_name, Some("public".to_string()));
        assert_eq!(metrics.analyzed_rows, 100);
        assert!((metrics.quality_score - 0.85).abs() < 0.001);
        assert_eq!(metrics.threshold_violations.len(), 1);
    }

    #[test]
    fn test_table_quality_metrics_score_clamping() {
        let metrics = TableQualityMetrics::new("test", None, 10).with_quality_score(1.5);
        assert_eq!(metrics.quality_score, 1.0);

        let metrics = TableQualityMetrics::new("test", None, 10).with_quality_score(-0.5);
        assert_eq!(metrics.quality_score, 0.0);
    }

    #[test]
    fn test_completeness_metrics_default() {
        let metrics = CompletenessMetrics::default();
        assert_eq!(metrics.score, 1.0);
        assert!(metrics.column_metrics.is_empty());
        assert_eq!(metrics.total_nulls, 0);
        assert_eq!(metrics.total_empty, 0);
    }

    #[test]
    fn test_consistency_metrics_default() {
        let metrics = ConsistencyMetrics::default();
        assert_eq!(metrics.score, 1.0);
        assert!(metrics.type_inconsistencies.is_empty());
        assert!(metrics.format_violations.is_empty());
    }

    #[test]
    fn test_uniqueness_metrics_default() {
        let metrics = UniquenessMetrics::default();
        assert_eq!(metrics.score, 1.0);
        assert!(metrics.duplicate_columns.is_empty());
        assert_eq!(metrics.duplicate_row_count, 0);
    }

    #[test]
    fn test_anomaly_metrics_default() {
        let metrics = AnomalyMetrics::default();
        assert_eq!(metrics.outlier_count, 0);
        assert!(metrics.outliers.is_empty());
    }

    #[test]
    fn test_column_completeness_anomalous_input() {
        // When null_count + empty_count exceeds total, completeness should clamp to 0.0
        let metrics = ColumnCompleteness::new("problematic", 60, 50, 100);
        assert_eq!(metrics.completeness, 0.0);
    }

    #[test]
    fn test_column_duplicates_anomalous_input() {
        // When duplicate_count exceeds total, uniqueness should clamp to 0.0
        let metrics = ColumnDuplicates::new("problematic", 150, 100);
        assert_eq!(metrics.uniqueness, 0.0);
        assert_eq!(metrics.unique_count, 0);
    }

    #[test]
    fn test_table_quality_metrics_serde_roundtrip() {
        let metrics = TableQualityMetrics::new("orders", Some("sales".to_string()), 500)
            .with_quality_score(0.92)
            .with_completeness(CompletenessMetrics {
                score: 0.95,
                column_metrics: vec![ColumnCompleteness::new("email", 5, 0, 100)],
                total_nulls: 5,
                total_empty: 0,
            });

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: TableQualityMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(metrics.table_name, deserialized.table_name);
        assert_eq!(metrics.schema_name, deserialized.schema_name);
        assert!((metrics.quality_score - deserialized.quality_score).abs() < 0.001);
        assert!((metrics.completeness.score - deserialized.completeness.score).abs() < 0.001);
    }
}
