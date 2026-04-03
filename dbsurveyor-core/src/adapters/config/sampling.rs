//! Data sampling configuration.
//!
//! This module provides configuration for data sampling operations
//! including sample size, throttling, and sensitive data detection.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Maximum allowed sample size to prevent OOM from unbounded LIMIT clauses.
pub const MAX_SAMPLE_SIZE: u32 = 10_000;

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
    /// Pre-compiled regex patterns paired with their description.
    ///
    /// Each entry is `(compiled_regex, description)`. Built from
    /// `sensitive_detection_patterns` to avoid recompiling on every row.
    #[serde(skip)]
    pub compiled_patterns: Vec<(Regex, String)>,
}

/// Compiles a list of [`SensitivePattern`]s into `(Regex, description)` pairs.
///
/// Invalid patterns are logged as warnings and skipped rather than
/// causing a hard failure, which also eliminates any ReDoS risk from
/// malformed user-supplied patterns.
fn compile_sensitive_patterns(patterns: &[SensitivePattern]) -> Vec<(Regex, String)> {
    patterns
        .iter()
        .filter_map(|p| match Regex::new(&p.pattern) {
            Ok(regex) => Some((regex, p.description.clone())),
            Err(e) => {
                eprintln!(
                    "WARNING: skipping invalid sensitive pattern '{}': {e}",
                    p.pattern
                );
                None
            }
        })
        .collect()
}

impl Default for SamplingConfig {
    fn default() -> Self {
        let sensitive_detection_patterns = vec![
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
        ];
        let compiled_patterns = compile_sensitive_patterns(&sensitive_detection_patterns);
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
            sensitive_detection_patterns,
            compiled_patterns,
        }
    }
}

impl SamplingConfig {
    /// Creates a new sampling config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates sampling configuration parameters.
    pub fn validate(&self) -> crate::Result<()> {
        if self.sample_size == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "sample_size must be at least 1",
            ));
        }
        if self.sample_size > MAX_SAMPLE_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "sample_size must not exceed {MAX_SAMPLE_SIZE}"
            )));
        }
        if self.query_timeout_secs == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "query_timeout_secs must be at least 1",
            ));
        }
        Ok(())
    }

    /// Builder method to set sample size.
    ///
    /// Values exceeding [`MAX_SAMPLE_SIZE`] are clamped to the maximum.
    pub fn with_sample_size(mut self, size: u32) -> Self {
        self.sample_size = size.min(MAX_SAMPLE_SIZE);
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
    ///
    /// The pattern is compiled immediately and added to `compiled_patterns`.
    /// If the regex is invalid, a warning is printed and the pattern is
    /// still stored in `sensitive_detection_patterns` but will not be
    /// included in `compiled_patterns`.
    pub fn add_sensitive_pattern(mut self, pattern: SensitivePattern) -> Self {
        match Regex::new(&pattern.pattern) {
            Ok(regex) => {
                self.compiled_patterns
                    .push((regex, pattern.description.clone()));
            }
            Err(e) => {
                eprintln!(
                    "WARNING: skipping invalid sensitive pattern '{}': {e}",
                    pattern.pattern
                );
            }
        }
        self.sensitive_detection_patterns.push(pattern);
        self
    }

    /// Recompiles all `compiled_patterns` from `sensitive_detection_patterns`.
    ///
    /// Call this after deserializing a `SamplingConfig` (since `compiled_patterns`
    /// is `#[serde(skip)]` and will be empty after deserialization).
    pub fn recompile_patterns(&mut self) {
        self.compiled_patterns = compile_sensitive_patterns(&self.sensitive_detection_patterns);
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
    fn test_validate_default_config() {
        let config = SamplingConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_sample_size() {
        // Construct directly to bypass the clamp in with_sample_size.
        let config = SamplingConfig {
            sample_size: 0,
            ..SamplingConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_exceeds_max_sample_size() {
        let config = SamplingConfig {
            sample_size: MAX_SAMPLE_SIZE + 1,
            ..SamplingConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_query_timeout() {
        let config = SamplingConfig {
            query_timeout_secs: 0,
            ..SamplingConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_with_sample_size_clamps_to_max() {
        let config = SamplingConfig::new().with_sample_size(u32::MAX);
        assert_eq!(config.sample_size, MAX_SAMPLE_SIZE);
    }

    #[test]
    fn test_validate_at_max_boundary() {
        let config = SamplingConfig::new().with_sample_size(MAX_SAMPLE_SIZE);
        assert_eq!(config.sample_size, MAX_SAMPLE_SIZE);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_add_sensitive_pattern() {
        let initial_count = SamplingConfig::default().sensitive_detection_patterns.len();

        let config = SamplingConfig::new()
            .add_sensitive_pattern(SensitivePattern::new(r"(?i)api_key", "API key detected"));

        assert_eq!(config.sensitive_detection_patterns.len(), initial_count + 1);
    }

    #[test]
    fn test_compiled_patterns_populated_on_default() {
        let config = SamplingConfig::default();
        assert_eq!(
            config.compiled_patterns.len(),
            config.sensitive_detection_patterns.len()
        );
    }

    #[test]
    fn test_compiled_patterns_updated_on_add() {
        let config = SamplingConfig::new()
            .add_sensitive_pattern(SensitivePattern::new(r"(?i)api_key", "API key detected"));

        assert_eq!(
            config.compiled_patterns.len(),
            config.sensitive_detection_patterns.len()
        );
    }

    #[test]
    fn test_invalid_pattern_skipped_in_compiled() {
        let config = SamplingConfig::new()
            .add_sensitive_pattern(SensitivePattern::new(r"[invalid", "Bad pattern"));

        // The raw pattern is still stored
        let last = config.sensitive_detection_patterns.last();
        assert!(last.is_some());
        // But compiled_patterns should not include the invalid one
        // (3 defaults + 0 for the invalid = 3)
        assert_eq!(config.compiled_patterns.len(), 3);
    }

    #[test]
    fn test_recompile_patterns() {
        let mut config = SamplingConfig::default();
        // Simulate deserialization clearing compiled_patterns
        config.compiled_patterns.clear();
        assert!(config.compiled_patterns.is_empty());

        config.recompile_patterns();
        assert_eq!(
            config.compiled_patterns.len(),
            config.sensitive_detection_patterns.len()
        );
    }
}
