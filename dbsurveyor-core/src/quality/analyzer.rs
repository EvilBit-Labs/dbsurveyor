//! Quality analyzer facade.
//!
//! This module provides the main `QualityAnalyzer` that orchestrates
//! all quality analysis functions and produces comprehensive metrics.

use crate::Result;
use crate::models::TableSample;

use super::anomaly::analyze_anomalies;
use super::completeness::analyze_completeness;
use super::config::QualityConfig;
use super::consistency::analyze_consistency;
use super::models::{TableQualityMetrics, ThresholdViolation};
use super::uniqueness::analyze_uniqueness;

/// Quality analyzer for assessing data quality metrics.
///
/// The analyzer processes `TableSample` objects and produces comprehensive
/// quality metrics including completeness, consistency, uniqueness, and
/// anomaly detection.
///
/// # Example
///
/// ```rust,ignore
/// use dbsurveyor_core::quality::{QualityAnalyzer, QualityConfig};
///
/// let config = QualityConfig::default();
/// let analyzer = QualityAnalyzer::new(config);
///
/// let metrics = analyzer.analyze(&table_sample)?;
/// println!("Quality score: {:.2}%", metrics.quality_score * 100.0);
/// ```
#[derive(Debug, Clone)]
pub struct QualityAnalyzer {
    config: QualityConfig,
}

impl QualityAnalyzer {
    /// Creates a new quality analyzer with the given configuration.
    pub fn new(config: QualityConfig) -> Self {
        Self { config }
    }

    /// Creates a new quality analyzer with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(QualityConfig::default())
    }

    /// Returns a reference to the analyzer configuration.
    pub fn config(&self) -> &QualityConfig {
        &self.config
    }

    /// Analyzes a table sample and returns quality metrics.
    ///
    /// This method runs all enabled quality analyses:
    /// - Completeness (null/empty value detection)
    /// - Consistency (type and format validation)
    /// - Uniqueness (duplicate detection)
    /// - Anomaly detection (statistical outliers, if enabled)
    ///
    /// # Arguments
    /// * `sample` - The table sample to analyze
    ///
    /// # Returns
    /// Comprehensive quality metrics for the table.
    pub fn analyze(&self, sample: &TableSample) -> Result<TableQualityMetrics> {
        if !self.config.enabled {
            // Return minimal metrics if analysis is disabled
            return Ok(TableQualityMetrics::new(
                &sample.table_name,
                sample.schema_name.clone(),
                sample.rows.len() as u64,
            ));
        }

        // Run all analyses
        let completeness = analyze_completeness(sample);
        let consistency = analyze_consistency(sample);
        let uniqueness = analyze_uniqueness(sample);

        // Run anomaly detection if enabled
        let anomalies = if self.config.anomaly_detection.enabled {
            Some(analyze_anomalies(
                sample,
                self.config.anomaly_detection.sensitivity,
            ))
        } else {
            None
        };

        // Calculate overall quality score
        let quality_score =
            self.calculate_quality_score(completeness.score, consistency.score, uniqueness.score);

        // Check for threshold violations
        let mut threshold_violations = Vec::new();

        if completeness.score < self.config.completeness_min {
            threshold_violations.push(ThresholdViolation::new(
                "completeness",
                self.config.completeness_min,
                completeness.score,
            ));
        }

        if consistency.score < self.config.consistency_min {
            threshold_violations.push(ThresholdViolation::new(
                "consistency",
                self.config.consistency_min,
                consistency.score,
            ));
        }

        if uniqueness.score < self.config.uniqueness_min {
            threshold_violations.push(ThresholdViolation::new(
                "uniqueness",
                self.config.uniqueness_min,
                uniqueness.score,
            ));
        }

        Ok(TableQualityMetrics::new(
            &sample.table_name,
            sample.schema_name.clone(),
            sample.rows.len() as u64,
        )
        .with_completeness(completeness)
        .with_consistency(consistency)
        .with_uniqueness(uniqueness)
        .with_quality_score(quality_score)
        .with_threshold_violations(threshold_violations)
        .with_optional_anomalies(anomalies))
    }

    /// Analyzes multiple table samples and returns metrics for each.
    ///
    /// Samples that fail analysis are logged and skipped rather than
    /// aborting the entire batch. This ensures partial results are
    /// still available when individual tables encounter issues.
    ///
    /// # Arguments
    /// * `samples` - The table samples to analyze
    ///
    /// # Returns
    /// A vector of quality metrics for successfully analyzed samples.
    pub fn analyze_all(&self, samples: &[TableSample]) -> Result<Vec<TableQualityMetrics>> {
        let mut results = Vec::with_capacity(samples.len());
        for sample in samples {
            match self.analyze(sample) {
                Ok(metrics) => results.push(metrics),
                Err(e) => {
                    tracing::warn!(
                        "Quality analysis failed for table '{}': {}",
                        sample.table_name,
                        e
                    );
                }
            }
        }
        Ok(results)
    }

    /// Calculates the overall quality score from individual metrics.
    ///
    /// The score is a weighted average of completeness, consistency,
    /// and uniqueness scores.
    fn calculate_quality_score(&self, completeness: f64, consistency: f64, uniqueness: f64) -> f64 {
        // Equal weighting for all three metrics
        (completeness + consistency + uniqueness) / 3.0
    }
}

// Extension methods for TableQualityMetrics builder
impl TableQualityMetrics {
    /// Sets the threshold violations.
    pub fn with_threshold_violations(mut self, violations: Vec<ThresholdViolation>) -> Self {
        self.threshold_violations = violations;
        self
    }

    /// Sets the anomalies if present.
    pub fn with_optional_anomalies(
        mut self,
        anomalies: Option<super::models::AnomalyMetrics>,
    ) -> Self {
        self.anomalies = anomalies;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SamplingStrategy;
    use crate::quality::config::{AnomalyConfig, AnomalySensitivity};
    use serde_json::json;

    fn create_sample(table_name: &str, rows: Vec<serde_json::Value>) -> TableSample {
        TableSample {
            table_name: table_name.to_string(),
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
    fn test_analyzer_creation() {
        let config = QualityConfig::default();
        let analyzer = QualityAnalyzer::new(config.clone());

        assert!(analyzer.config().enabled);
        assert_eq!(analyzer.config().completeness_min, config.completeness_min);
    }

    #[test]
    fn test_analyzer_with_defaults() {
        let analyzer = QualityAnalyzer::with_defaults();
        assert!(analyzer.config().enabled);
    }

    #[test]
    fn test_analyzer_disabled() {
        let config = QualityConfig::new().with_enabled(false);
        let analyzer = QualityAnalyzer::new(config);

        let sample = create_sample("test", vec![json!({"id": 1, "name": null})]);
        let metrics = analyzer.analyze(&sample).unwrap();

        // Should return minimal metrics with default scores
        assert_eq!(metrics.table_name, "test");
        assert_eq!(metrics.quality_score, 1.0); // Default score
    }

    #[test]
    fn test_analyzer_full_analysis() {
        let config = QualityConfig::default();
        let analyzer = QualityAnalyzer::new(config);

        let rows = vec![
            json!({"id": 1, "name": "Alice", "value": 100}),
            json!({"id": 2, "name": "Bob", "value": 200}),
            json!({"id": 3, "name": null, "value": 150}),
        ];

        let metrics = analyzer.analyze(&create_sample("users", rows)).unwrap();

        assert_eq!(metrics.table_name, "users");
        assert_eq!(metrics.schema_name, Some("public".to_string()));
        assert_eq!(metrics.analyzed_rows, 3);
        assert!(metrics.quality_score > 0.0);
        assert!(metrics.quality_score <= 1.0);
    }

    #[test]
    fn test_analyzer_threshold_violations() {
        let config = QualityConfig::new()
            .with_completeness_min(0.99) // Very high threshold
            .with_consistency_min(0.99)
            .with_uniqueness_min(0.99);

        let analyzer = QualityAnalyzer::new(config);

        let rows = vec![
            json!({"id": 1, "status": "active"}),
            json!({"id": 2, "status": null}), // Affects completeness
            json!({"id": 3, "status": "active"}), // Duplicate status
        ];

        let metrics = analyzer.analyze(&create_sample("test", rows)).unwrap();

        // Should have threshold violations
        assert!(!metrics.threshold_violations.is_empty());
    }

    #[test]
    fn test_analyzer_no_anomaly_detection() {
        let config =
            QualityConfig::new().with_anomaly_detection(AnomalyConfig::new().with_enabled(false));

        let analyzer = QualityAnalyzer::new(config);

        let rows = vec![
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 1000}), // Would be an outlier
        ];

        let metrics = analyzer.analyze(&create_sample("test", rows)).unwrap();

        // Anomalies should be None when disabled
        assert!(metrics.anomalies.is_none());
    }

    #[test]
    fn test_analyzer_with_anomaly_detection() {
        let config = QualityConfig::new().with_anomaly_detection(
            AnomalyConfig::new()
                .with_enabled(true)
                .with_sensitivity(AnomalySensitivity::High),
        );

        let analyzer = QualityAnalyzer::new(config);

        // Need more data points for z-score detection to work
        let rows = vec![
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 10}),
            json!({"value": 1000}), // Extreme outlier
        ];

        let metrics = analyzer.analyze(&create_sample("test", rows)).unwrap();

        // Anomalies should be present
        assert!(metrics.anomalies.is_some());
        let anomalies = metrics.anomalies.unwrap();
        assert!(anomalies.outlier_count > 0);
    }

    #[test]
    fn test_analyzer_analyze_all() {
        let analyzer = QualityAnalyzer::with_defaults();

        let samples = vec![
            create_sample("table1", vec![json!({"id": 1})]),
            create_sample("table2", vec![json!({"id": 2})]),
            create_sample("table3", vec![json!({"id": 3})]),
        ];

        let results = analyzer.analyze_all(&samples).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].table_name, "table1");
        assert_eq!(results[1].table_name, "table2");
        assert_eq!(results[2].table_name, "table3");
    }

    #[test]
    fn test_quality_score_calculation() {
        let analyzer = QualityAnalyzer::with_defaults();

        // Perfect scores
        let score = analyzer.calculate_quality_score(1.0, 1.0, 1.0);
        assert!((score - 1.0).abs() < 0.001);

        // Mixed scores
        let score = analyzer.calculate_quality_score(0.9, 0.8, 0.7);
        assert!((score - 0.8).abs() < 0.001);

        // Zero scores
        let score = analyzer.calculate_quality_score(0.0, 0.0, 0.0);
        assert!((score - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_analyzer_empty_sample() {
        let analyzer = QualityAnalyzer::with_defaults();
        let metrics = analyzer.analyze(&create_sample("empty", vec![])).unwrap();

        assert_eq!(metrics.table_name, "empty");
        assert_eq!(metrics.analyzed_rows, 0);
        assert_eq!(metrics.quality_score, 1.0); // Default perfect score for empty
    }

    #[test]
    fn test_violation_severity_assignment() {
        let config = QualityConfig::new().with_completeness_min(0.95);
        let analyzer = QualityAnalyzer::new(config);

        // Create sample with enough nulls to trigger violation
        let rows = vec![
            json!({"id": null}),
            json!({"id": null}),
            json!({"id": null}),
            json!({"id": 4}),
            json!({"id": 5}),
        ];

        let metrics = analyzer.analyze(&create_sample("test", rows)).unwrap();

        // Should have completeness violation
        let completeness_violation = metrics
            .threshold_violations
            .iter()
            .find(|v| v.metric == "completeness");

        assert!(completeness_violation.is_some());
    }
}
