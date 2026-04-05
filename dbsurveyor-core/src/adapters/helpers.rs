//! Helper utilities for database adapter implementations.
//!
//! Provides common functionality shared across different database adapters
//! to reduce code duplication and ensure consistent error handling.

use std::sync::OnceLock;

/// Pre-compiled regex patterns for validation to avoid repeated compilation.
///
/// Uses `OnceLock` for thread-safe lazy initialization.
pub struct ValidationPatterns {
    /// Patterns for detecting connection strings in input
    pub connection_patterns: Vec<regex::Regex>,
    /// Patterns for detecting sensitive data fields
    pub sensitive_field_patterns: Vec<regex::Regex>,
}

impl ValidationPatterns {
    /// Gets the singleton instance of pre-compiled validation patterns.
    pub fn instance() -> &'static Self {
        static PATTERNS: OnceLock<ValidationPatterns> = OnceLock::new();
        PATTERNS.get_or_init(Self::compile)
    }

    /// Compiles all validation patterns.
    ///
    /// This is called once during initialization.
    fn compile() -> Self {
        // Connection string patterns for credential detection
        let connection_patterns = vec![
            regex::Regex::new(r"postgres://.*:.*@").expect("Invalid postgres pattern"),
            regex::Regex::new(r"postgresql://.*:.*@").expect("Invalid postgresql pattern"),
            regex::Regex::new(r"mysql://.*:.*@").expect("Invalid mysql pattern"),
            regex::Regex::new(r"mongodb://.*:.*@").expect("Invalid mongodb pattern"),
            regex::Regex::new(r"mongodb\+srv://.*:.*@").expect("Invalid mongodb+srv pattern"),
            regex::Regex::new(r"mssql://.*:.*@").expect("Invalid mssql pattern"),
        ];

        // Sensitive field patterns for data classification
        let sensitive_field_patterns = vec![
            regex::Regex::new(r"(?i)(password|passwd|pwd)").expect("Invalid password pattern"),
            regex::Regex::new(r"(?i)(email|mail)").expect("Invalid email pattern"),
            regex::Regex::new(r"(?i)(ssn|social_security)").expect("Invalid SSN pattern"),
            regex::Regex::new(r"(?i)(credit_card|card_number|cvv)").expect("Invalid CC pattern"),
            regex::Regex::new(r"(?i)(api_key|apikey|secret_key)").expect("Invalid API key pattern"),
            regex::Regex::new(r"(?i)(token|auth_token|bearer)").expect("Invalid token pattern"),
        ];

        Self {
            connection_patterns,
            sensitive_field_patterns,
        }
    }

    /// Checks if a string contains connection credentials.
    pub fn contains_credentials(&self, s: &str) -> bool {
        let lower = s.to_lowercase();
        self.connection_patterns
            .iter()
            .any(|pattern| pattern.is_match(&lower))
    }

    /// Checks if a field name appears sensitive.
    pub fn is_sensitive_field(&self, field_name: &str) -> bool {
        let lower = field_name.to_lowercase();
        self.sensitive_field_patterns
            .iter()
            .any(|pattern| pattern.is_match(&lower))
    }
}

/// Common timestamp column names used for ordering by "most recent" rows.
///
/// Shared across all database adapters to detect timestamp-like columns
/// for intelligent sampling order.
pub(crate) const TIMESTAMP_COLUMN_NAMES: &[&str] = &[
    "created_at",
    "updated_at",
    "modified_at",
    "inserted_at",
    "timestamp",
    "created",
    "updated",
    "modified",
    "date_created",
    "date_updated",
    "date_modified",
    "createdat",
    "updatedat",
    "modifiedat",
    "creation_time",
    "modification_time",
    "update_time",
    "create_time",
];

/// Collects an optional schema object (views, triggers, functions, etc.).
///
/// On success, logs the count and returns the collected items.
/// On failure, pushes a warning message and returns an empty `Vec`.
///
/// This eliminates the repeated match-Ok-log / Err-warn-push pattern
/// found in every adapter's `collect_schema` function.
pub(crate) fn resolve_optional_collection<T>(
    description: &str,
    result: crate::Result<Vec<T>>,
    warnings: &mut Vec<String>,
) -> Vec<T> {
    match result {
        Ok(items) => {
            tracing::info!("Successfully collected {} {}", items.len(), description);
            items
        }
        Err(e) => {
            let warning = format!("Failed to collect {}: {}", description, e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    }
}

/// Macro for reducing boilerplate error handling when querying database metadata.
///
/// # Example
/// ```rust,ignore
/// let rows = query_with_privilege_check!(
///     self.pool,
///     "SELECT * FROM information_schema.tables",
///     "information_schema.tables"
/// )?;
/// ```
#[macro_export]
macro_rules! query_with_privilege_check {
    ($pool:expr, $query:expr, $resource:expr) => {{
        sqlx::query($query)
            .fetch_all($pool)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                    $crate::error::DbSurveyorError::insufficient_privileges(concat!(
                        "Cannot access ",
                        $resource,
                        " - insufficient privileges"
                    ))
                }
                _ => $crate::error::DbSurveyorError::collection_failed(
                    concat!("Failed to query ", $resource),
                    e,
                ),
            })
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_patterns_singleton() {
        // Should return the same instance
        let p1 = ValidationPatterns::instance();
        let p2 = ValidationPatterns::instance();
        assert!(std::ptr::eq(p1, p2));
    }

    #[test]
    fn test_contains_credentials() {
        let patterns = ValidationPatterns::instance();

        assert!(patterns.contains_credentials("postgres://user:pass@localhost/db"));
        assert!(patterns.contains_credentials("POSTGRES://USER:PASS@LOCALHOST/DB"));
        assert!(!patterns.contains_credentials("postgres://localhost/db"));
        assert!(!patterns.contains_credentials("just a normal string"));
    }

    #[test]
    fn test_resolve_optional_collection_ok() {
        let mut warnings = Vec::new();
        let result: crate::Result<Vec<String>> = Ok(vec!["a".to_string(), "b".to_string()]);
        let items = resolve_optional_collection("widgets", result, &mut warnings);
        assert_eq!(items.len(), 2);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_resolve_optional_collection_err() {
        let mut warnings = Vec::new();
        let result: crate::Result<Vec<String>> =
            Err(crate::error::DbSurveyorError::collection_failed(
                "test error",
                std::io::Error::other("boom"),
            ));
        let items = resolve_optional_collection("widgets", result, &mut warnings);
        assert!(items.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Failed to collect widgets"));
    }

    #[test]
    fn test_resolve_optional_collection_empty_ok() {
        let mut warnings = Vec::new();
        let result: crate::Result<Vec<i32>> = Ok(Vec::new());
        let items = resolve_optional_collection("things", result, &mut warnings);
        assert!(items.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_is_sensitive_field() {
        let patterns = ValidationPatterns::instance();

        assert!(patterns.is_sensitive_field("password"));
        assert!(patterns.is_sensitive_field("PASSWORD"));
        assert!(patterns.is_sensitive_field("user_email"));
        assert!(patterns.is_sensitive_field("api_key"));
        assert!(!patterns.is_sensitive_field("username"));
        assert!(!patterns.is_sensitive_field("created_at"));
    }
}
