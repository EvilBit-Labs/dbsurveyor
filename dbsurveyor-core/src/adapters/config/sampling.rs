//! Data sampling configuration.
//!
//! This module provides configuration for data sampling operations
//! including sample size, throttling, and sensitive data detection.

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(Debug, Clone, Serialize)]
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
    pub(crate) compiled_patterns: Vec<(Regex, String)>,
}

impl<'de> Deserialize<'de> for SamplingConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Shadow struct for deriving `Deserialize` without recursion.
        #[derive(Deserialize)]
        struct Raw {
            sample_size: u32,
            throttle_ms: Option<u64>,
            query_timeout_secs: u64,
            warn_sensitive: bool,
            timestamp_columns: Vec<String>,
            sensitive_detection_patterns: Vec<SensitivePattern>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let compiled_patterns = compile_sensitive_patterns(&raw.sensitive_detection_patterns);
        Ok(Self {
            sample_size: raw.sample_size,
            throttle_ms: raw.throttle_ms,
            query_timeout_secs: raw.query_timeout_secs,
            warn_sensitive: raw.warn_sensitive,
            timestamp_columns: raw.timestamp_columns,
            sensitive_detection_patterns: raw.sensitive_detection_patterns,
            compiled_patterns,
        })
    }
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
                tracing::warn!("Skipping invalid sensitive pattern '{}': {e}", p.pattern);
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
        if self.sample_size == 0 || self.sample_size > MAX_SAMPLE_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "sample_size must be between 1 and {MAX_SAMPLE_SIZE}, got {}",
                self.sample_size
            )));
        }
        if self.query_timeout_secs == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "query_timeout_secs must be at least 1",
            ));
        }

        let bad_patterns: Vec<&str> = self
            .sensitive_detection_patterns
            .iter()
            .filter(|p| Regex::new(&p.pattern).is_err())
            .map(|p| p.pattern.as_str())
            .collect();
        if !bad_patterns.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "invalid sensitive detection regex pattern(s): {}",
                bad_patterns.join(", ")
            )));
        }

        Ok(())
    }

    /// Builder method to set sample size.
    ///
    /// Values below 1 are clamped to 1; values exceeding
    /// [`MAX_SAMPLE_SIZE`] are clamped to the maximum.
    #[must_use]
    pub fn with_sample_size(mut self, size: u32) -> Self {
        if size == 0 {
            tracing::warn!("Requested sample_size 0 is below minimum 1; clamped to 1");
        } else if size > MAX_SAMPLE_SIZE {
            tracing::warn!(
                "Requested sample_size {} exceeds maximum {}; clamped",
                size,
                MAX_SAMPLE_SIZE
            );
        }
        self.sample_size = size.clamp(1, MAX_SAMPLE_SIZE);
        self
    }

    /// Builder method to set throttle delay.
    #[must_use]
    pub fn with_throttle_ms(mut self, ms: u64) -> Self {
        self.throttle_ms = Some(ms);
        self
    }

    /// Builder method to set query timeout.
    #[must_use]
    pub fn with_query_timeout_secs(mut self, secs: u64) -> Self {
        self.query_timeout_secs = secs;
        self
    }

    /// Builder method to enable/disable sensitive data warnings.
    #[must_use]
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
    #[must_use]
    pub fn add_sensitive_pattern(mut self, pattern: SensitivePattern) -> Self {
        match Regex::new(&pattern.pattern) {
            Ok(regex) => {
                self.compiled_patterns
                    .push((regex, pattern.description.clone()));
                self.sensitive_detection_patterns.push(pattern);
            }
            Err(e) => {
                tracing::warn!(
                    "Skipping invalid sensitive pattern '{}': {e}",
                    pattern.pattern
                );
            }
        }
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
    fn test_invalid_pattern_skipped_entirely() {
        let initial_count = SamplingConfig::default().sensitive_detection_patterns.len();
        let config = SamplingConfig::new()
            .add_sensitive_pattern(SensitivePattern::new(r"[invalid", "Bad pattern"));

        // Invalid pattern is not stored in either list
        assert_eq!(config.sensitive_detection_patterns.len(), initial_count);
        assert_eq!(config.compiled_patterns.len(), initial_count);
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

    #[test]
    fn test_with_sample_size_clamps_zero_to_one() {
        let config = SamplingConfig::new().with_sample_size(0);
        assert_eq!(config.sample_size, 1);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_deserialization_recompiles_patterns() {
        let original = SamplingConfig::default();
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: SamplingConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            restored.compiled_patterns.len(),
            restored.sensitive_detection_patterns.len(),
        );
    }

    #[test]
    fn test_validate_rejects_invalid_regex_pattern() {
        let config = SamplingConfig {
            sensitive_detection_patterns: vec![
                SensitivePattern::new(r"(?i)password", "Valid pattern"),
                SensitivePattern::new(r"[invalid", "Bad regex"),
            ],
            ..SamplingConfig::default()
        };
        let err = config.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("[invalid"),
            "Error should mention the bad pattern, got: {msg}"
        );
    }
}
