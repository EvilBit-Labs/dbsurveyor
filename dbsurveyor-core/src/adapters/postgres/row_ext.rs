//! Extension trait for extracting typed values from PostgreSQL rows
//! with consistent error handling.

use crate::{Result, error::DbSurveyorError};
use sqlx::{Row, postgres::PgRow};

/// Extension trait for extracting typed values from database rows
/// with consistent error handling.
///
/// # Example
/// ```rust,ignore
/// use dbsurveyor_core::adapters::postgres::RowExt;
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
