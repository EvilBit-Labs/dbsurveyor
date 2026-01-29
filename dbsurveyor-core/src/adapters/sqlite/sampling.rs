//! SQLite data sampling utilities.
//!
//! This module provides intelligent ordering strategy detection and data sampling
//! functionality for SQLite tables.
//!
//! # Ordering Strategy Detection
//!
//! The detection priority is:
//! 1. Primary key columns (most reliable for consistent ordering)
//! 2. Timestamp columns (created_at, updated_at, etc.)
//! 3. Auto-increment columns (INTEGER PRIMARY KEY)
//! 4. ROWID (SQLite's built-in row identifier)
//! 5. Fallback to unordered (uses RANDOM() for sampling)

use crate::adapters::config::SamplingConfig;
use crate::error::DbSurveyorError;
use crate::models::{OrderingStrategy, SamplingStrategy, SortDirection, TableSample};
use serde_json::Value as JsonValue;
use sqlx::{Row, SqlitePool};
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

/// Detect the best ordering strategy for a SQLite table.
pub async fn detect_ordering_strategy(
    pool: &SqlitePool,
    table: &str,
) -> Result<OrderingStrategy, DbSurveyorError> {
    // 1. Check for primary key
    if let Some(pk_strategy) = detect_primary_key(pool, table).await? {
        tracing::debug!(
            "Detected primary key ordering for {}: {:?}",
            table,
            pk_strategy
        );
        return Ok(pk_strategy);
    }

    // 2. Check for timestamp columns
    if let Some(ts_strategy) = detect_timestamp_column(pool, table).await? {
        tracing::debug!(
            "Detected timestamp ordering for {}: {:?}",
            table,
            ts_strategy
        );
        return Ok(ts_strategy);
    }

    // 3. Check for auto-increment columns
    if let Some(auto_strategy) = detect_auto_increment_column(pool, table).await? {
        tracing::debug!(
            "Detected auto-increment ordering for {}: {:?}",
            table,
            auto_strategy
        );
        return Ok(auto_strategy);
    }

    // 4. Check for ROWID (almost all SQLite tables have this)
    if let Some(rowid_strategy) = detect_rowid(pool, table).await? {
        tracing::debug!(
            "Detected ROWID ordering for {}: {:?}",
            table,
            rowid_strategy
        );
        return Ok(rowid_strategy);
    }

    // 5. Fallback to unordered
    tracing::debug!(
        "No reliable ordering found for {}, using unordered fallback",
        table
    );
    Ok(OrderingStrategy::Unordered)
}

/// Detect primary key columns for ordering.
async fn detect_primary_key(
    pool: &SqlitePool,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    let pk_query = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));

    let rows = sqlx::query(&pk_query).fetch_all(pool).await.map_err(|e| {
        DbSurveyorError::collection_failed(
            format!("Failed to detect primary key for table '{}'", table),
            e,
        )
    })?;

    // Collect columns with pk > 0
    let mut pk_columns: Vec<(i32, String)> = rows
        .iter()
        .filter_map(|row| {
            let pk: i32 = row.try_get("pk").unwrap_or(0);
            if pk > 0 {
                let name: String = row.try_get("name").unwrap_or_default();
                Some((pk, name))
            } else {
                None
            }
        })
        .collect();

    if pk_columns.is_empty() {
        return Ok(None);
    }

    // Sort by pk order for composite keys
    pk_columns.sort_by_key(|(pk, _)| *pk);
    let columns: Vec<String> = pk_columns.into_iter().map(|(_, name)| name).collect();

    Ok(Some(OrderingStrategy::PrimaryKey { columns }))
}

/// Detect timestamp columns suitable for ordering.
async fn detect_timestamp_column(
    pool: &SqlitePool,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    let columns_query = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));

    let rows = sqlx::query(&columns_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!("Failed to detect timestamp columns for table '{}'", table),
                e,
            )
        })?;

    // Collect columns with timestamp-like types or names
    let timestamp_columns: Vec<(String, String)> = rows
        .iter()
        .filter_map(|row| {
            let name: String = row.try_get("name").unwrap_or_default();
            let data_type: String = row.try_get("type").unwrap_or_default();
            let type_upper = data_type.to_uppercase();

            // Check if type suggests timestamp
            if type_upper.contains("DATE")
                || type_upper.contains("TIME")
                || type_upper.contains("TIMESTAMP")
            {
                Some((name, data_type))
            } else {
                None
            }
        })
        .collect();

    // Look for columns with common timestamp names
    for (column_name, _) in &timestamp_columns {
        let column_name_lower = column_name.to_lowercase();

        if TIMESTAMP_COLUMN_NAMES
            .iter()
            .any(|&name| column_name_lower == name)
        {
            return Ok(Some(OrderingStrategy::Timestamp {
                column: column_name.clone(),
                direction: SortDirection::Descending,
            }));
        }
    }

    // If no exact match, look for partial matches
    for (column_name, _) in &timestamp_columns {
        let column_name_lower = column_name.to_lowercase();

        if column_name_lower.contains("created")
            || column_name_lower.contains("inserted")
            || column_name_lower.contains("timestamp")
        {
            return Ok(Some(OrderingStrategy::Timestamp {
                column: column_name.clone(),
                direction: SortDirection::Descending,
            }));
        }
    }

    Ok(None)
}

/// Detect auto-increment columns (INTEGER PRIMARY KEY in SQLite).
async fn detect_auto_increment_column(
    pool: &SqlitePool,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    let columns_query = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));

    let rows = sqlx::query(&columns_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!(
                    "Failed to detect auto-increment columns for table '{}'",
                    table
                ),
                e,
            )
        })?;

    // In SQLite, INTEGER PRIMARY KEY is auto-increment
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        let data_type: String = row.try_get("type").unwrap_or_default();
        let pk: i32 = row.try_get("pk").unwrap_or(0);

        if pk > 0 && data_type.to_uppercase() == "INTEGER" {
            return Ok(Some(OrderingStrategy::AutoIncrement { column: name }));
        }
    }

    Ok(None)
}

/// Detect if table has accessible ROWID.
async fn detect_rowid(
    pool: &SqlitePool,
    table: &str,
) -> Result<Option<OrderingStrategy>, DbSurveyorError> {
    // Try to query rowid to check if it exists
    let test_query = format!(
        "SELECT rowid FROM \"{}\" LIMIT 1",
        table.replace('"', "\"\"")
    );

    match sqlx::query(&test_query).fetch_optional(pool).await {
        Ok(_) => Ok(Some(OrderingStrategy::SystemRowId {
            column: "rowid".to_string(),
        })),
        Err(_) => Ok(None), // Table might be WITHOUT ROWID
    }
}

/// Generate an ORDER BY clause for the given ordering strategy (SQLite syntax).
pub fn generate_order_by_clause(strategy: &OrderingStrategy, descending: bool) -> String {
    let direction = if descending { "DESC" } else { "ASC" };

    match strategy {
        OrderingStrategy::PrimaryKey { columns } => {
            let cols: Vec<String> = columns
                .iter()
                .map(|c| format!("\"{}\" {}", c.replace('"', "\"\""), direction))
                .collect();
            format!("ORDER BY {}", cols.join(", "))
        }
        OrderingStrategy::Timestamp { column, .. } => {
            format!("ORDER BY \"{}\" {}", column.replace('"', "\"\""), direction)
        }
        OrderingStrategy::AutoIncrement { column } => {
            format!("ORDER BY \"{}\" {}", column.replace('"', "\"\""), direction)
        }
        OrderingStrategy::SystemRowId { column } => {
            format!("ORDER BY {} {}", column, direction)
        }
        OrderingStrategy::Unordered => {
            // For unordered tables, use RANDOM() for fair sampling
            "ORDER BY RANDOM()".to_string()
        }
    }
}

/// Sample data from a SQLite table with rate limiting and intelligent ordering.
pub async fn sample_table(
    pool: &SqlitePool,
    table: &str,
    config: &SamplingConfig,
) -> Result<TableSample, DbSurveyorError> {
    let mut warnings = Vec::new();

    // Apply throttling if configured
    if let Some(throttle_ms) = config.throttle_ms {
        tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
    }

    // Detect ordering strategy
    let strategy = detect_ordering_strategy(pool, table).await?;

    // Generate ORDER BY clause
    let order_by = generate_order_by_clause(&strategy, true);

    // Build and execute the sample query
    // Use double quotes for SQLite identifier quoting
    let query = format!(
        "SELECT * FROM \"{}\" {} LIMIT ?",
        table.replace('"', "\"\""),
        order_by
    );

    let rows = sqlx::query(&query)
        .bind(config.sample_size as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!("Failed to sample data from table '{}'", table),
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
    let count_query = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
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
        schema_name: None,
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
    row: &sqlx::sqlite::SqliteRow,
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
fn extract_column_value(row: &sqlx::sqlite::SqliteRow, column_name: &str) -> JsonValue {
    // Try different types in order of likelihood
    // SQLite is dynamically typed, so we need to try multiple types
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
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(column_name) {
        // For BLOB data, convert to base64 string
        return v
            .map(|bytes| {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                JsonValue::String(format!("base64:{}", encoded))
            })
            .unwrap_or(JsonValue::Null);
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
        assert_eq!(clause, "ORDER BY \"id\" DESC");
    }

    #[test]
    fn test_generate_order_by_composite_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["tenant_id".to_string(), "id".to_string()],
        };
        let clause = generate_order_by_clause(&strategy, false);
        assert_eq!(clause, "ORDER BY \"tenant_id\" ASC, \"id\" ASC");
    }

    #[test]
    fn test_generate_order_by_timestamp() {
        let strategy = OrderingStrategy::Timestamp {
            column: "created_at".to_string(),
            direction: SortDirection::Descending,
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"created_at\" DESC");
    }

    #[test]
    fn test_generate_order_by_auto_increment() {
        let strategy = OrderingStrategy::AutoIncrement {
            column: "id".to_string(),
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"id\" DESC");
    }

    #[test]
    fn test_generate_order_by_rowid() {
        let strategy = OrderingStrategy::SystemRowId {
            column: "rowid".to_string(),
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY rowid DESC");
    }

    #[test]
    fn test_generate_order_by_unordered() {
        let strategy = OrderingStrategy::Unordered;
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY RANDOM()");
    }

    #[test]
    fn test_identifier_escaping() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["weird\"column".to_string()],
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, "ORDER BY \"weird\"\"column\" DESC");
    }
}
