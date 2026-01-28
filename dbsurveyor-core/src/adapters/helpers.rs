//! Helper utilities for database adapter implementations.
//!
//! Provides common functionality shared across different database adapters
//! to reduce code duplication and ensure consistent error handling.

use crate::{Result, error::DbSurveyorError};
use sqlx::{Row, postgres::PgRow};
use std::sync::OnceLock;

/// Extension trait for extracting typed values from database rows
/// with consistent error handling.
///
/// # Example
/// ```rust,ignore
/// use dbsurveyor_core::adapters::helpers::RowExt;
///
/// let name: String = row.get_field("column_name", Some("my_table"))?;
/// let size: Option<i64> = row.get_field("size_bytes", None)?;
/// ```
pub trait RowExt {
    /// Extracts a typed field from the row with proper error context.
    ///
    /// # Arguments
    /// * `field_name` - Name of the column to extract
    /// * `table_context` - Optional table name for error messages
    ///
    /// # Returns
    /// The extracted value or an error with context
    fn get_field<'r, T>(&'r self, field_name: &str, table_context: Option<&str>) -> Result<T>
    where
        T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>;
}

impl RowExt for PgRow {
    fn get_field<'r, T>(&'r self, field_name: &str, table_context: Option<&str>) -> Result<T>
    where
        T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
    {
        self.try_get(field_name)
            .map_err(|e| DbSurveyorError::parse_field(field_name, table_context, e))
    }
}

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
