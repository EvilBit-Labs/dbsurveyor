//! Quality assessment configuration.
//!
//! This module provides configuration for data quality analysis including
//! threshold settings and anomaly detection sensitivity.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Anomaly detection sensitivity level.
///
/// Controls how many standard deviations from the mean a value
/// must be to be considered an outlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AnomalySensitivity {
    /// 3.0 standard deviations - fewer false positives
    Low,
    /// 2.5 standard deviations - balanced detection
    #[default]
    Medium,
    /// 2.0 standard deviations - more aggressive detection
    High,
}

impl AnomalySensitivity {
    /// Returns the z-score threshold for this sensitivity level.
    pub fn z_score_threshold(&self) -> f64 {
        match self {
            AnomalySensitivity::Low => 3.0,
            AnomalySensitivity::Medium => 2.5,
            AnomalySensitivity::High => 2.0,
        }
    }
}

/// Anomaly detection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Enable anomaly detection
    pub enabled: bool,
    /// Detection sensitivity level
    pub sensitivity: AnomalySensitivity,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: AnomalySensitivity::Medium,
        }
    }
}

impl AnomalyConfig {
    /// Creates a new anomaly config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to enable/disable anomaly detection.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder method to set sensitivity level.
    pub fn with_sensitivity(mut self, sensitivity: AnomalySensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }
}

/// Quality assessment configuration.
///
/// Controls data quality analysis thresholds and anomaly detection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    /// Enable quality analysis
    pub enabled: bool,
    /// Minimum completeness threshold (0.0-1.0)
    pub completeness_min: f64,
    /// Minimum uniqueness threshold (0.0-1.0)
    pub uniqueness_min: f64,
    /// Minimum consistency threshold (0.0-1.0)
    pub consistency_min: f64,
    /// Anomaly detection settings
    pub anomaly_detection: AnomalyConfig,
}

/// Validation errors for quality configuration.
#[derive(Debug, Error)]
pub enum ConfigValidationError {
    #[error("completeness_min must be between 0.0 and 1.0, got {0}")]
    InvalidCompleteness(f64),
    #[error("uniqueness_min must be between 0.0 and 1.0, got {0}")]
    InvalidUniqueness(f64),
    #[error("consistency_min must be between 0.0 and 1.0, got {0}")]
    InvalidConsistency(f64),
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            completeness_min: 0.95,
            // Note: 0.98 is strict. Low-cardinality columns (e.g., status,
            // category) will naturally trigger violations and may need
            // per-table threshold adjustments via CLI overrides.
            uniqueness_min: 0.98,
            consistency_min: 0.90,
            anomaly_detection: AnomalyConfig::default(),
        }
    }
}

impl QualityConfig {
    /// Creates a new quality config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to enable/disable quality analysis.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder method to set completeness threshold.
    pub fn with_completeness_min(mut self, threshold: f64) -> Self {
        if !(0.0..=1.0).contains(&threshold) {
            tracing::warn!(
                "completeness_min {} clamped to valid range [0.0, 1.0]",
                threshold
            );
        }
        self.completeness_min = threshold.clamp(0.0, 1.0);
        self
    }

    /// Builder method to set uniqueness threshold.
    pub fn with_uniqueness_min(mut self, threshold: f64) -> Self {
        if !(0.0..=1.0).contains(&threshold) {
            tracing::warn!(
                "uniqueness_min {} clamped to valid range [0.0, 1.0]",
                threshold
            );
        }
        self.uniqueness_min = threshold.clamp(0.0, 1.0);
        self
    }

    /// Builder method to set consistency threshold.
    pub fn with_consistency_min(mut self, threshold: f64) -> Self {
        if !(0.0..=1.0).contains(&threshold) {
            tracing::warn!(
                "consistency_min {} clamped to valid range [0.0, 1.0]",
                threshold
            );
        }
        self.consistency_min = threshold.clamp(0.0, 1.0);
        self
    }

    /// Builder method to set anomaly detection config.
    pub fn with_anomaly_detection(mut self, config: AnomalyConfig) -> Self {
        self.anomaly_detection = config;
        self
    }

    /// Validates the configuration.
    ///
    /// Returns an error if any threshold is outside valid range.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if !(0.0..=1.0).contains(&self.completeness_min) {
            return Err(ConfigValidationError::InvalidCompleteness(
                self.completeness_min,
            ));
        }
        if !(0.0..=1.0).contains(&self.uniqueness_min) {
            return Err(ConfigValidationError::InvalidUniqueness(
                self.uniqueness_min,
            ));
        }
        if !(0.0..=1.0).contains(&self.consistency_min) {
            return Err(ConfigValidationError::InvalidConsistency(
                self.consistency_min,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_sensitivity_z_scores() {
        assert_eq!(AnomalySensitivity::Low.z_score_threshold(), 3.0);
        assert_eq!(AnomalySensitivity::Medium.z_score_threshold(), 2.5);
        assert_eq!(AnomalySensitivity::High.z_score_threshold(), 2.0);
    }

    #[test]
    fn test_anomaly_sensitivity_default() {
        let sensitivity = AnomalySensitivity::default();
        assert_eq!(sensitivity, AnomalySensitivity::Medium);
    }

    #[test]
    fn test_anomaly_config_default() {
        let config = AnomalyConfig::default();
        assert!(config.enabled);
        assert_eq!(config.sensitivity, AnomalySensitivity::Medium);
    }

    #[test]
    fn test_anomaly_config_builder() {
        let config = AnomalyConfig::new()
            .with_enabled(false)
            .with_sensitivity(AnomalySensitivity::High);

        assert!(!config.enabled);
        assert_eq!(config.sensitivity, AnomalySensitivity::High);
    }

    #[test]
    fn test_quality_config_default() {
        let config = QualityConfig::default();
        assert!(config.enabled);
        assert_eq!(config.completeness_min, 0.95);
        assert_eq!(config.uniqueness_min, 0.98);
        assert_eq!(config.consistency_min, 0.90);
        assert!(config.anomaly_detection.enabled);
    }

    #[test]
    fn test_quality_config_builder() {
        let config = QualityConfig::new()
            .with_enabled(true)
            .with_completeness_min(0.8)
            .with_uniqueness_min(0.9)
            .with_consistency_min(0.85)
            .with_anomaly_detection(
                AnomalyConfig::new()
                    .with_enabled(false)
                    .with_sensitivity(AnomalySensitivity::Low),
            );

        assert!(config.enabled);
        assert_eq!(config.completeness_min, 0.8);
        assert_eq!(config.uniqueness_min, 0.9);
        assert_eq!(config.consistency_min, 0.85);
        assert!(!config.anomaly_detection.enabled);
        assert_eq!(
            config.anomaly_detection.sensitivity,
            AnomalySensitivity::Low
        );
    }

    #[test]
    fn test_quality_config_threshold_clamping() {
        let config = QualityConfig::new()
            .with_completeness_min(1.5) // Over 1.0
            .with_uniqueness_min(-0.5); // Below 0.0

        assert_eq!(config.completeness_min, 1.0);
        assert_eq!(config.uniqueness_min, 0.0);
    }

    #[test]
    fn test_quality_config_validate_success() {
        let config = QualityConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_quality_config_validate_invalid_completeness() {
        // Note: Due to clamping, we need to set the field directly to test validation
        let config = QualityConfig {
            completeness_min: 1.5, // Bypass clamping
            ..QualityConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidCompleteness(_))
        ));
    }

    #[test]
    fn test_quality_config_validate_invalid_uniqueness() {
        let config = QualityConfig {
            uniqueness_min: -0.1,
            ..QualityConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidUniqueness(_))
        ));
    }

    #[test]
    fn test_quality_config_validate_invalid_consistency() {
        let config = QualityConfig {
            consistency_min: 2.0,
            ..QualityConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidConsistency(_))
        ));
    }

    #[test]
    fn test_quality_config_serde_roundtrip() {
        let config = QualityConfig::new()
            .with_completeness_min(0.75)
            .with_anomaly_detection(
                AnomalyConfig::new().with_sensitivity(AnomalySensitivity::High),
            );

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: QualityConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.completeness_min, deserialized.completeness_min);
        assert_eq!(
            config.anomaly_detection.sensitivity,
            deserialized.anomaly_detection.sensitivity
        );
    }
}
