//! Data sampling configuration.
//!
//! This module provides configuration for data sampling operations
//! including sample size, throttling, and sensitive data detection.

use serde::{Deserialize, Serialize};

/// Pattern for detecting sensitive data fields.
///
/// Used to identify columns that may contain sensitive information
/// such as passwords, emails, or social security numbers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePattern {
    /// Regex pattern to match column names
    pub pattern: String,
    /// Human-readable description of what was detected
    pub description: String,
}

impl SensitivePattern {
    /// Creates a new sensitive pattern.
    pub fn new(pattern: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            description: description.into(),
        }
    }
}

/// Configuration for data sampling.
///
/// Controls how data samples are collected from database tables,
/// including sample sizes, throttling, and sensitive data warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Number of rows to sample per table
    pub sample_size: u32,
    /// Optional throttle delay between queries (milliseconds)
    pub throttle_ms: Option<u64>,
    /// Query timeout in seconds
    pub query_timeout_secs: u64,
    /// Whether to warn about sensitive data detection
    pub warn_sensitive: bool,
    /// Column names to use for ordering samples (most recent first)
    pub timestamp_columns: Vec<String>,
    /// Patterns for detecting sensitive data fields
    pub sensitive_detection_patterns: Vec<SensitivePattern>,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            sample_size: 100,
            throttle_ms: None,
            query_timeout_secs: 30,
            warn_sensitive: true,
            timestamp_columns: vec![
                "created_at".to_string(),
                "updated_at".to_string(),
                "modified_at".to_string(),
                "timestamp".to_string(),
            ],
            sensitive_detection_patterns: vec![
                SensitivePattern {
                    pattern: r"(?i)(password|passwd|pwd)".to_string(),
                    description: "Password field detected".to_string(),
                },
                SensitivePattern {
                    pattern: r"(?i)(email|mail)".to_string(),
                    description: "Email field detected".to_string(),
                },
                SensitivePattern {
                    pattern: r"(?i)(ssn|social_security)".to_string(),
                    description: "Social Security Number field detected".to_string(),
                },
            ],
        }
    }
}

impl SamplingConfig {
    /// Creates a new sampling config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set sample size.
    pub fn with_sample_size(mut self, size: u32) -> Self {
        self.sample_size = size;
        self
    }

    /// Builder method to set throttle delay.
    pub fn with_throttle_ms(mut self, ms: u64) -> Self {
        self.throttle_ms = Some(ms);
        self
    }

    /// Builder method to set query timeout.
    pub fn with_query_timeout_secs(mut self, secs: u64) -> Self {
        self.query_timeout_secs = secs;
        self
    }

    /// Builder method to enable/disable sensitive data warnings.
    pub fn with_sensitive_warnings(mut self, enabled: bool) -> Self {
        self.warn_sensitive = enabled;
        self
    }

    /// Adds a custom sensitive pattern.
    pub fn add_sensitive_pattern(mut self, pattern: SensitivePattern) -> Self {
        self.sensitive_detection_patterns.push(pattern);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_config_default() {
        let config = SamplingConfig::default();
        assert_eq!(config.sample_size, 100);
        assert_eq!(config.throttle_ms, None);
        assert_eq!(config.query_timeout_secs, 30);
        assert!(config.warn_sensitive);
        assert!(!config.timestamp_columns.is_empty());
        assert!(!config.sensitive_detection_patterns.is_empty());
    }

    #[test]
    fn test_sampling_config_builder() {
        let config = SamplingConfig::new()
            .with_sample_size(50)
            .with_throttle_ms(100)
            .with_query_timeout_secs(60)
            .with_sensitive_warnings(false);

        assert_eq!(config.sample_size, 50);
        assert_eq!(config.throttle_ms, Some(100));
        assert_eq!(config.query_timeout_secs, 60);
        assert!(!config.warn_sensitive);
    }

    #[test]
    fn test_sensitive_pattern_new() {
        let pattern = SensitivePattern::new(r"(?i)api_key", "API key detected");
        assert_eq!(pattern.pattern, r"(?i)api_key");
        assert_eq!(pattern.description, "API key detected");
    }

    #[test]
    fn test_add_sensitive_pattern() {
        let initial_count = SamplingConfig::default().sensitive_detection_patterns.len();

        let config = SamplingConfig::new()
            .add_sensitive_pattern(SensitivePattern::new(r"(?i)api_key", "API key detected"));

        assert_eq!(config.sensitive_detection_patterns.len(), initial_count + 1);
    }
}
