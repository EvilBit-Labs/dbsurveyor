//! PostgreSQL data sampling utilities.
//!
//! This module provides intelligent ordering strategy detection and data sampling
//! functionality for PostgreSQL tables. It analyzes table structure to determine
//! the best way to order rows when sampling data.
//!
//! # Ordering Strategy Detection
//!
//! The detection priority is:
//! 1. Primary key columns (most reliable for consistent ordering)
//! 2. Timestamp columns (created_at, updated_at, etc.)
//! 3. Auto-increment/serial columns
//! 4. Fallback to unordered (will use RANDOM() for sampling)
//!
//! # Security
//! - All queries are read-only
//! - Uses parameterized queries to prevent SQL injection
//! - Respects connection pool timeout settings

use crate::error::DbSurveyorError;
use crate::models::{OrderingStrategy, SortDirection};
use sqlx::{PgPool, Row};

/// Common timestamp column names used for ordering by "most recent"
const TIMESTAMP_COLUMN_NAMES: &[&str] = &[
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

/// Detect the best ordering strategy for a table.
///
/// This function analyzes the table structure to determine the most reliable
/// way to order rows for sampling. The detection priority is:
///
/// 1. **Primary key** - Most reliable for consistent ordering
/// 2. **Timestamp columns** - Good for "most recent" semantics
/// 3. **Auto-increment columns** - Reliable insertion order
/// 4. **Unordered fallback** - When no reliable ordering exists
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `schema` - Schema name (e.g., "public")
/// * `table` - Table name
///
/// # Returns
///
/// Returns the detected `OrderingStrategy` or an error if the detection fails.
///
/// # Example
///
/// ```rust,ignore
/// let strategy = detect_ordering_strategy(&pool, "public", "users").await?;
/// match strategy {
///     OrderingStrategy::PrimaryKey { columns } => {
///         println!("Order by primary key: {:?}", columns);
///     }
///     OrderingStrategy::Timestamp { column, direction } => {
///         println!("Order by timestamp: {}", column);
///     }
///     _ => {}
/// }
/// ```
pub async fn detect_ordering_strategy(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<OrderingStrategy, DbSurveyorError> {
    // 1. Check for primary key
    if let Some(pk_strategy) = detect_primary_key(pool, schema, table).await? {
        tracing::debug!(
            "Detected primary key ordering for {}.{}: {:?}",
            schema,
            table,
            pk_strategy
        );
        return Ok(pk_strategy);
    }

    // 2. Check for timestamp columns
    if let Some(ts_strategy) = detect_timestamp_column(pool, schema, table).await? {
        tracing::debug!(
            "Detected timestamp ordering for {}.{}: {:?}",
            schema,
            table,
            ts_strategy
        );
        return Ok(ts_strategy);
    }

    // 3. Check for auto-increment/serial columns
    if let Some(auto_strategy) = detect_auto_increment_column(pool, schema, table).await? {
        tracing::debug!(
            "Detected auto-increment ordering for {}.{}: {:?}",
            schema,
            table,
            auto_strategy
        );
        return Ok(auto_strategy);
    }

    // 4. Fallback to unordered
    tracing::debug!(
        "No reliable ordering found for {}.{}, using unordered fallback",
        schema,
        table
    );
    Ok(OrderingStrategy::Unordered)
}

/// Detect primary key columns for ordering.
///
/// Queries the PostgreSQL system catalogs to find primary key columns.
async fn detect_primary_key(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Query to get primary key columns in order
    let pk_query = r#"
        SELECT a.attname AS column_name
        FROM pg_index i
        JOIN pg_class c ON c.oid = i.indrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
        WHERE i.indisprimary = true
        AND c.relname = $1
        AND n.nspname = $2
        ORDER BY array_position(i.indkey, a.attnum)
    "#;

    let rows = sqlx::query(pk_query)
        .bind(table)
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect primary key for table '{}.{}'",
                    schema, table
                ),
                e,
            )
        })?;

    if rows.is_empty() {
        return Ok(None);
    }

    let columns: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect();

    Ok(Some(OrderingStrategy::PrimaryKey { columns }))
}

/// Detect timestamp columns suitable for ordering.
///
/// Looks for common timestamp column names and validates they are
/// timestamp or timestamptz types.
async fn detect_timestamp_column(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Query to find timestamp columns with common names
    // We look for specific column names that typically represent record creation/modification time
    let ts_query = r#"
        SELECT column_name, data_type
        FROM information_schema.columns
        WHERE table_schema = $1
        AND table_name = $2
        AND data_type IN ('timestamp without time zone', 'timestamp with time zone', 'date')
        ORDER BY ordinal_position
    "#;

    let rows = sqlx::query(ts_query)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect timestamp columns for table '{}.{}'",
                    schema, table
                ),
                e,
            )
        })?;

    // Look for columns with common timestamp names
    for row in &rows {
        let column_name: String = row.get("column_name");
        let column_name_lower = column_name.to_lowercase();

        // Check if this column has a common timestamp name
        if TIMESTAMP_COLUMN_NAMES
            .iter()
            .any(|&name| column_name_lower == name)
        {
            return Ok(Some(OrderingStrategy::Timestamp {
                column: column_name,
                direction: SortDirection::Descending, // Most recent first
            }));
        }
    }

    // If no exact match, look for columns containing common patterns
    for row in &rows {
        let column_name: String = row.get("column_name");
        let column_name_lower = column_name.to_lowercase();

        // Check for partial matches
        if column_name_lower.contains("created")
            || column_name_lower.contains("inserted")
            || column_name_lower.contains("timestamp")
        {
            return Ok(Some(OrderingStrategy::Timestamp {
                column: column_name,
                direction: SortDirection::Descending,
            }));
        }
    }

    Ok(None)
}

/// Detect auto-increment/serial columns.
///
/// Looks for columns with nextval() defaults, indicating serial/identity columns.
async fn detect_auto_increment_column(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Query to find auto-increment columns (serial types use nextval())
    // Also check for IDENTITY columns (PostgreSQL 10+)
    let auto_query = r#"
        SELECT column_name, column_default, is_identity
        FROM information_schema.columns
        WHERE table_schema = $1
        AND table_name = $2
        AND (
            (column_default IS NOT NULL AND column_default LIKE 'nextval%')
            OR is_identity = 'YES'
        )
        ORDER BY ordinal_position
        LIMIT 1
    "#;

    let row = sqlx::query(auto_query)
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect auto-increment columns for table '{}.{}'",
                    schema, table
                ),
                e,
            )
        })?;

    if let Some(row) = row {
        let column_name: String = row.get("column_name");
        return Ok(Some(OrderingStrategy::AutoIncrement {
            column: column_name,
        }));
    }

    Ok(None)
}

/// Generate an ORDER BY clause for the given ordering strategy.
///
/// # Arguments
///
/// * `strategy` - The ordering strategy to generate SQL for
/// * `descending` - If true, order descending (most recent first)
///
/// # Returns
///
/// Returns a SQL ORDER BY clause string.
///
/// # Example
///
/// ```rust,ignore
/// let strategy = OrderingStrategy::PrimaryKey { columns: vec!["id".to_string()] };
/// let clause = generate_order_by_clause(&strategy, true);
/// assert_eq!(clause, "ORDER BY \"id\" DESC");
/// ```
pub fn generate_order_by_clause(strategy: &OrderingStrategy, descending: bool) -> String {
    let direction = if descending { "DESC" } else { "ASC" };

    match strategy {
        OrderingStrategy::PrimaryKey { columns } => {
            let cols: Vec<String> = columns
                .iter()
                .map(|c| format!("\"{}\" {}", c, direction))
                .collect();
            format!("ORDER BY {}", cols.join(", "))
        }
        OrderingStrategy::Timestamp { column, .. } => {
            format!("ORDER BY \"{}\" {}", column, direction)
        }
        OrderingStrategy::AutoIncrement { column } => {
            format!("ORDER BY \"{}\" {}", column, direction)
        }
        OrderingStrategy::SystemRowId { column } => {
            format!("ORDER BY \"{}\" {}", column, direction)
        }
        OrderingStrategy::Unordered => {
            // For unordered tables, use RANDOM() for fair sampling
            "ORDER BY RANDOM()".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_order_by_clause_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["id".to_string()],
        };

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"id\" DESC");

        let clause = generate_order_by_clause(&strategy, false);
        assert_eq!(clause, "ORDER BY \"id\" ASC");
    }

    #[test]
    fn test_generate_order_by_clause_composite_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["tenant_id".to_string(), "id".to_string()],
        };

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"tenant_id\" DESC, \"id\" DESC");
    }

    #[test]
    fn test_generate_order_by_clause_timestamp() {
        let strategy = OrderingStrategy::Timestamp {
            column: "created_at".to_string(),
            direction: SortDirection::Descending,
        };

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"created_at\" DESC");
    }

    #[test]
    fn test_generate_order_by_clause_auto_increment() {
        let strategy = OrderingStrategy::AutoIncrement {
            column: "row_id".to_string(),
        };

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"row_id\" DESC");
    }

    #[test]
    fn test_generate_order_by_clause_unordered() {
        let strategy = OrderingStrategy::Unordered;

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY RANDOM()");

        // Direction doesn't matter for random
        let clause = generate_order_by_clause(&strategy, false);
        assert_eq!(clause, "ORDER BY RANDOM()");
    }

    #[test]
    fn test_generate_order_by_clause_special_characters() {
        // Test with column names that need quoting
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["user-id".to_string(), "table".to_string()],
        };

        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"user-id\" DESC, \"table\" DESC");
    }
}
