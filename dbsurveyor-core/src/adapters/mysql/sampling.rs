//! MySQL data sampling utilities.
//!
//! This module provides intelligent ordering strategy detection and data sampling
//! functionality for MySQL tables.
//!
//! # Ordering Strategy Detection
//!
//! The detection priority is:
//! 1. Primary key columns (most reliable for consistent ordering)
//! 2. Timestamp columns (created_at, updated_at, etc.)
//! 3. Auto-increment columns
//! 4. Fallback to unordered (will use RAND() for sampling)

use crate::adapters::config::SamplingConfig;
use crate::error::DbSurveyorError;
use crate::models::{OrderingStrategy, SamplingStrategy, SortDirection, TableSample};
use serde_json::Value as JsonValue;
use sqlx::{MySqlPool, Row};
use std::time::Duration;

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

/// Detect the best ordering strategy for a MySQL table.
pub async fn detect_ordering_strategy(
    pool: &MySqlPool,
    db_name: &str,
    table: &str,
) -> Result<OrderingStrategy, DbSurveyorError> {
    // 1. Check for primary key
    if let Some(pk_strategy) = detect_primary_key(pool, db_name, table).await? {
        tracing::debug!(
            "Detected primary key ordering for {}.{}: {:?}",
            db_name,
            table,
            pk_strategy
        );
        return Ok(pk_strategy);
    }

    // 2. Check for timestamp columns
    if let Some(ts_strategy) = detect_timestamp_column(pool, db_name, table).await? {
        tracing::debug!(
            "Detected timestamp ordering for {}.{}: {:?}",
            db_name,
            table,
            ts_strategy
        );
        return Ok(ts_strategy);
    }

    // 3. Check for auto-increment columns
    if let Some(auto_strategy) = detect_auto_increment_column(pool, db_name, table).await? {
        tracing::debug!(
            "Detected auto-increment ordering for {}.{}: {:?}",
            db_name,
            table,
            auto_strategy
        );
        return Ok(auto_strategy);
    }

    // 4. Fallback to unordered
    tracing::debug!(
        "No reliable ordering found for {}.{}, using unordered fallback",
        db_name,
        table
    );
    Ok(OrderingStrategy::Unordered)
}

/// Detect primary key columns for ordering.
async fn detect_primary_key(
    pool: &MySqlPool,
    db_name: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let pk_query = r#"
        SELECT CAST(COLUMN_NAME AS CHAR) as COLUMN_NAME
        FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
        WHERE TABLE_SCHEMA = ?
        AND TABLE_NAME = ?
        AND CONSTRAINT_NAME = 'PRIMARY'
        ORDER BY ORDINAL_POSITION
    "#;

    let rows = sqlx::query(pk_query)
        .bind(db_name)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect primary key for table '{}.{}'",
                    db_name, table
                ),
                e,
            )
        })?;

    if rows.is_empty() {
        return Ok(None);
    }

    let columns: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("COLUMN_NAME"))
        .collect();

    Ok(Some(OrderingStrategy::PrimaryKey { columns }))
}

/// Detect timestamp columns suitable for ordering.
async fn detect_timestamp_column(
    pool: &MySqlPool,
    db_name: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let ts_query = r#"
        SELECT CAST(COLUMN_NAME AS CHAR) as COLUMN_NAME, CAST(DATA_TYPE AS CHAR) as DATA_TYPE
        FROM INFORMATION_SCHEMA.COLUMNS
        WHERE TABLE_SCHEMA = ?
        AND TABLE_NAME = ?
        AND DATA_TYPE IN ('timestamp', 'datetime', 'date')
        ORDER BY ORDINAL_POSITION
    "#;

    let rows = sqlx::query(ts_query)
        .bind(db_name)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect timestamp columns for table '{}.{}'",
                    db_name, table
                ),
                e,
            )
        })?;

    // Look for columns with common timestamp names
    for row in &rows {
        let column_name: String = row.get("COLUMN_NAME");
        let column_name_lower = column_name.to_lowercase();

        if TIMESTAMP_COLUMN_NAMES
            .iter()
            .any(|&name| column_name_lower == name)
        {
            return Ok(Some(OrderingStrategy::Timestamp {
                column: column_name,
                direction: SortDirection::Descending,
            }));
        }
    }

    // If no exact match, look for partial matches
    for row in &rows {
        let column_name: String = row.get("COLUMN_NAME");
        let column_name_lower = column_name.to_lowercase();

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

/// Detect auto-increment columns.
async fn detect_auto_increment_column(
    pool: &MySqlPool,
    db_name: &str,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let auto_query = r#"
        SELECT CAST(COLUMN_NAME AS CHAR) as COLUMN_NAME
        FROM INFORMATION_SCHEMA.COLUMNS
        WHERE TABLE_SCHEMA = ?
        AND TABLE_NAME = ?
        AND EXTRA LIKE '%auto_increment%'
        ORDER BY ORDINAL_POSITION
        LIMIT 1
    "#;

    let row = sqlx::query(auto_query)
        .bind(db_name)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect auto-increment columns for table '{}.{}'",
                    db_name, table
                ),
                e,
            )
        })?;

    if let Some(row) = row {
        let column_name: String = row.get("COLUMN_NAME");
        return Ok(Some(OrderingStrategy::AutoIncrement {
            column: column_name,
        }));
    }

    Ok(None)
}

/// Generate an ORDER BY clause for the given ordering strategy (MySQL syntax).
pub fn generate_order_by_clause(strategy: &OrderingStrategy, descending: bool) -> String {
    let direction = if descending { "DESC" } else { "ASC" };

    match strategy {
        OrderingStrategy::PrimaryKey { columns } => {
            let cols: Vec<String> = columns
                .iter()
                .map(|c| format!("`{}` {}", c, direction))
                .collect();
            format!("ORDER BY {}", cols.join(", "))
        }
        OrderingStrategy::Timestamp { column, .. } => {
            format!("ORDER BY `{}` {}", column, direction)
        }
        OrderingStrategy::AutoIncrement { column } => {
            format!("ORDER BY `{}` {}", column, direction)
        }
        OrderingStrategy::SystemRowId { column } => {
            format!("ORDER BY `{}` {}", column, direction)
        }
        OrderingStrategy::Unordered => {
            // For unordered tables, use RAND() for fair sampling
            "ORDER BY RAND()".to_string()
        }
    }
}

/// Sample data from a MySQL table with rate limiting and intelligent ordering.
pub async fn sample_table(
    pool: &MySqlPool,
    db_name: &str,
    table: &str,
    config: &SamplingConfig,
) -> Result<TableSample, DbSurveyorError> {
    let mut warnings = Vec::new();
    let _sample_start = std::time::Instant::now();

    // Apply throttling if configured
    if let Some(throttle_ms) = config.throttle_ms {
        tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
    }

    // Detect ordering strategy
    let strategy = detect_ordering_strategy(pool, db_name, table).await?;

    // Generate ORDER BY clause
    let order_by = generate_order_by_clause(&strategy, true);

    // Build and execute the sample query
    // Use backticks for MySQL identifier quoting
    let query = format!(
        "SELECT * FROM `{}`.`{}` {} LIMIT ?",
        db_name, table, order_by
    );

    let rows = sqlx::query(&query)
        .bind(config.sample_size as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!("Failed to sample data from table '{}.{}'", db_name, table),
                e,
            )
        })?;

    // Convert rows to JSON
    let mut json_rows = Vec::with_capacity(rows.len());
    for row in &rows {
        let json_row = row_to_json(row, config, &mut warnings)?;
        json_rows.push(json_row);
    }

    // Get total row count
    let count_query = format!("SELECT COUNT(*) FROM `{}`.`{}`", db_name, table);
    let total_rows: i64 = sqlx::query_scalar(&count_query)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Add warning if no reliable ordering found
    if matches!(strategy, OrderingStrategy::Unordered) {
        warnings.push(format!(
            "No reliable ordering found for table '{}', using random sampling",
            table
        ));
    }

    let sampling_strategy = SamplingStrategy::MostRecent {
        limit: config.sample_size,
    };

    Ok(TableSample {
        table_name: table.to_string(),
        schema_name: Some(db_name.to_string()),
        rows: json_rows,
        sample_size: rows.len() as u32,
        total_rows: Some(total_rows as u64),
        sampling_strategy,
        collected_at: chrono::Utc::now(),
        warnings,
    })
}

/// Convert a database row to JSON, checking for sensitive data patterns.
fn row_to_json(
    row: &sqlx::mysql::MySqlRow,
    config: &SamplingConfig,
    warnings: &mut Vec<String>,
) -> Result<JsonValue, DbSurveyorError> {
    use sqlx::Column;

    let mut map = serde_json::Map::new();

    for column in row.columns() {
        let column_name = column.name();

        // Check for sensitive column names if warnings are enabled
        if config.warn_sensitive {
            let name_lower = column_name.to_lowercase();
            for pattern in &config.sensitive_detection_patterns {
                if let Ok(regex) = regex::Regex::new(&pattern.pattern)
                    && regex.is_match(&name_lower)
                {
                    warnings.push(format!(
                        "Column '{}' may contain sensitive data ({})",
                        column_name, pattern.description
                    ));
                    break;
                }
            }
        }

        // Try to extract value as JSON-compatible type
        let value = extract_column_value(row, column_name);
        map.insert(column_name.to_string(), value);
    }

    Ok(JsonValue::Object(map))
}

/// Extract a column value as a JSON value.
fn extract_column_value(row: &sqlx::mysql::MySqlRow, column_name: &str) -> JsonValue {
    // Try different types in order of likelihood
    if let Ok(v) = row.try_get::<Option<String>, _>(column_name) {
        return v.map(JsonValue::String).unwrap_or(JsonValue::Null);
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(column_name) {
        return v
            .map(|n| JsonValue::Number(n.into()))
            .unwrap_or(JsonValue::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(column_name) {
        return v
            .and_then(serde_json::Number::from_f64)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(column_name) {
        return v.map(JsonValue::Bool).unwrap_or(JsonValue::Null);
    }

    // Default to null for unsupported types
    JsonValue::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_order_by_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["id".to_string()],
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY `id` DESC");
    }

    #[test]
    fn test_generate_order_by_composite_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["tenant_id".to_string(), "id".to_string()],
        };
        let clause = generate_order_by_clause(&strategy, false);
        assert_eq!(clause, "ORDER BY `tenant_id` ASC, `id` ASC");
    }

    #[test]
    fn test_generate_order_by_timestamp() {
        let strategy = OrderingStrategy::Timestamp {
            column: "created_at".to_string(),
            direction: SortDirection::Descending,
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY `created_at` DESC");
    }

    #[test]
    fn test_generate_order_by_auto_increment() {
        let strategy = OrderingStrategy::AutoIncrement {
            column: "id".to_string(),
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY `id` DESC");
    }

    #[test]
    fn test_generate_order_by_unordered() {
        let strategy = OrderingStrategy::Unordered;
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY RAND()");
    }
}
