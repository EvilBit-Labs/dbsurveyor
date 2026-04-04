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

use crate::adapters::config::SamplingConfig;
use crate::adapters::helpers::TIMESTAMP_COLUMN_NAMES;
use crate::error::DbSurveyorError;
use crate::models::{
    Column, OrderingStrategy, SampleStatus, SamplingStrategy, SortDirection, TableSample,
    UnifiedDataType,
};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use std::time::Duration;

/// Minimum estimated row count to use `TABLESAMPLE BERNOULLI` instead of
/// `ORDER BY RANDOM()`. For very small tables the overhead of the tablesample
/// operator is not worthwhile and the percentage calculation becomes unreliable.
const TABLESAMPLE_MIN_ROWS: u64 = 1000;

/// Oversampling multiplier applied to the Bernoulli percentage so that the
/// returned set is very likely to contain at least `sample_size` rows.
/// The LIMIT clause trims the excess.
const TABLESAMPLE_OVERSAMPLING_FACTOR: f64 = 2.0;

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
    detect_ordering_strategy_with_columns(pool, schema, table, None).await
}

/// Detect the best ordering strategy, optionally using pre-collected column metadata.
///
/// When `columns` is `Some`, the ordering strategy is derived entirely from the
/// provided metadata, avoiding redundant database queries for PK, timestamp, and
/// auto-increment detection. When `None`, falls back to the original query-based
/// detection.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool (unused when `columns` is `Some`)
/// * `schema` - Schema name (e.g., "public")
/// * `table` - Table name
/// * `columns` - Optional pre-collected column metadata from schema collection
pub async fn detect_ordering_strategy_with_columns(
    pool: &PgPool,
    schema: &str,
    table: &str,
    columns: Option<&[Column]>,
) -> Result<OrderingStrategy, DbSurveyorError> {
    // Fast path: derive strategy from pre-collected metadata
    if let Some(cols) = columns {
        let strategy = detect_ordering_from_columns(cols);
        tracing::debug!(
            "Derived ordering strategy for {}.{} from metadata: {:?}",
            schema,
            table,
            strategy
        );
        return Ok(strategy);
    }

    // Slow path: query the database for ordering info
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

    let Some(row) = row else {
        return Ok(None);
    };

    let column_name: String = row.get("column_name");
    Ok(Some(OrderingStrategy::AutoIncrement {
        column: column_name,
    }))
}

/// Derive an ordering strategy from pre-collected column metadata.
///
/// This avoids redundant database queries when schema collection has already
/// gathered column information (primary keys, data types, auto-increment flags).
///
/// The detection priority mirrors [`detect_ordering_strategy`]:
/// 1. Primary key columns (ordered by `ordinal_position`)
/// 2. Timestamp columns matching common naming conventions
/// 3. Auto-increment/serial columns
/// 4. Unordered fallback
fn detect_ordering_from_columns(columns: &[Column]) -> OrderingStrategy {
    // 1. Check for primary key columns
    let mut pk_columns: Vec<&Column> = columns.iter().filter(|c| c.is_primary_key).collect();
    if !pk_columns.is_empty() {
        pk_columns.sort_by_key(|c| c.ordinal_position);
        let names = pk_columns.iter().map(|c| c.name.clone()).collect();
        return OrderingStrategy::PrimaryKey { columns: names };
    }

    // 2. Check for timestamp columns with well-known names
    let timestamp_columns: Vec<&Column> = columns
        .iter()
        .filter(|c| is_timestamp_type(&c.data_type))
        .collect();

    // Exact match on common timestamp column names (highest priority)
    for col in &timestamp_columns {
        let lower = col.name.to_lowercase();
        if TIMESTAMP_COLUMN_NAMES.iter().any(|&name| lower == name) {
            return OrderingStrategy::Timestamp {
                column: col.name.clone(),
                direction: SortDirection::Descending,
            };
        }
    }

    // Partial match on common patterns
    for col in &timestamp_columns {
        let lower = col.name.to_lowercase();
        if lower.contains("created") || lower.contains("inserted") || lower.contains("timestamp") {
            return OrderingStrategy::Timestamp {
                column: col.name.clone(),
                direction: SortDirection::Descending,
            };
        }
    }

    // 3. Check for auto-increment columns
    let mut auto_columns: Vec<&Column> = columns.iter().filter(|c| c.is_auto_increment).collect();
    if !auto_columns.is_empty() {
        auto_columns.sort_by_key(|c| c.ordinal_position);
        return OrderingStrategy::AutoIncrement {
            column: auto_columns[0].name.clone(),
        };
    }

    // 4. Fallback
    OrderingStrategy::Unordered
}

/// Returns `true` if the unified data type represents a timestamp or date type
/// suitable for ordering.
fn is_timestamp_type(data_type: &UnifiedDataType) -> bool {
    matches!(
        data_type,
        UnifiedDataType::DateTime { .. } | UnifiedDataType::Date
    )
}

/// Escapes a SQL identifier for use in double-quoted PostgreSQL identifiers.
///
/// PostgreSQL escapes embedded double quotes by doubling them.
/// E.g., `my"table` becomes `"my""table"`.
fn escape_identifier(ident: &str) -> String {
    ident.replace('"', "\"\"")
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
                .map(|c| format!("\"{}\" {}", escape_identifier(c), direction))
                .collect();
            format!("ORDER BY {}", cols.join(", "))
        }
        OrderingStrategy::Timestamp { column, .. } => {
            format!("ORDER BY \"{}\" {}", escape_identifier(column), direction)
        }
        OrderingStrategy::AutoIncrement { column } => {
            format!("ORDER BY \"{}\" {}", escape_identifier(column), direction)
        }
        OrderingStrategy::SystemRowId { column } => {
            format!("ORDER BY \"{}\" {}", escape_identifier(column), direction)
        }
        OrderingStrategy::Unordered => {
            // For unordered tables, use RANDOM() for fair sampling
            "ORDER BY RANDOM()".to_string()
        }
    }
}

/// Sample data from a table with rate limiting and intelligent ordering.
///
/// This function samples rows from a table using the detected ordering strategy
/// to provide meaningful samples (e.g., most recent records). Rate limiting is
/// applied to prevent overwhelming the database with sampling queries.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `schema` - Optional schema name (e.g., `Some("public")`). When `None`, catalog
///   lookups default to `"public"` and the FROM clause uses an unqualified table name.
/// * `table` - Table name
/// * `config` - Sampling configuration including sample size and throttle settings
///
/// # Returns
///
/// Returns a `TableSample` containing the sampled rows as JSON, metadata about
/// the sampling operation, and any warnings encountered.
///
/// # Ordering Strategy
///
/// The function automatically detects the best ordering strategy:
/// 1. Primary key - Most reliable for consistent ordering (descending = most recent)
/// 2. Timestamp columns - Good for "most recent" semantics
/// 3. Auto-increment columns - Reliable insertion order
/// 4. Random sampling - Fallback when no reliable ordering exists
///
/// # Example
///
/// ```rust,ignore
/// let config = SamplingConfig::new()
///     .with_sample_size(10)
///     .with_throttle_ms(100);
///
/// let sample = sample_table(&pool, Some("public"), "users", &config).await?;
/// println!("Sampled {} rows out of ~{}", sample.rows.len(), sample.total_rows.unwrap_or(0));
/// ```
pub async fn sample_table(
    pool: &PgPool,
    schema: Option<&str>,
    table: &str,
    config: &SamplingConfig,
) -> Result<TableSample, DbSurveyorError> {
    sample_table_with_columns(pool, schema, table, config, None).await
}

/// Sample data from a table, optionally using pre-collected column metadata.
///
/// When `columns` is `Some`, ordering strategy detection uses the provided
/// metadata instead of querying the database, avoiding redundant PK/timestamp/
/// auto-increment lookups that schema collection already performed.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `schema` - Optional schema name (e.g., `Some("public")`)
/// * `table` - Table name
/// * `config` - Sampling configuration including sample size and throttle settings
/// * `columns` - Optional pre-collected column metadata from schema collection
pub async fn sample_table_with_columns(
    pool: &PgPool,
    schema: Option<&str>,
    table: &str,
    config: &SamplingConfig,
    columns: Option<&[Column]>,
) -> Result<TableSample, DbSurveyorError> {
    config.validate()?;
    let mut warnings = Vec::new();
    let display_name = match schema {
        Some(s) => format!("{}.{}", s, table),
        None => table.to_string(),
    };

    // Apply rate limiting delay if configured
    if let Some(throttle_ms) = config.throttle_ms {
        let delay = Duration::from_millis(throttle_ms);
        tokio::time::sleep(delay).await;
    }

    // Ordering detection queries filter by schema name in pg_namespace,
    // so default to "public" when no schema is specified.
    let detection_schema = match schema {
        Some(s) => s,
        None => {
            tracing::warn!(
                "No schema specified for table '{}'; defaulting to 'public' for ordering detection",
                table
            );
            warnings.push(format!(
                "Schema not specified for '{}'; defaulted to 'public' for ordering detection",
                table
            ));
            "public"
        }
    };
    let strategy =
        detect_ordering_strategy_with_columns(pool, detection_schema, table, columns).await?;

    // Determine sampling strategy and add warnings for unordered tables
    let (sampling_strategy, is_random) = match &strategy {
        OrderingStrategy::Unordered => {
            warnings.push(
                "No reliable ordering found - using random sampling which may not be reproducible"
                    .to_string(),
            );
            (
                SamplingStrategy::Random {
                    limit: config.sample_size,
                },
                true,
            )
        }
        _ => (
            SamplingStrategy::MostRecent {
                limit: config.sample_size,
            },
            false,
        ),
    };

    // Get total row count estimate from pg_class (fast approximate count)
    let count_query = r#"
        SELECT reltuples::bigint AS estimated_count
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
    "#;

    let total_rows: Option<i64> = sqlx::query_scalar(count_query)
        .bind(detection_schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!("Failed to get row count for table '{}'", display_name),
                e,
            )
        })?;

    // Build FROM clause: schema-qualified when schema is present, table-only otherwise.
    // Identifiers are escaped to prevent SQL injection from embedded quotes.
    let base_table = match schema {
        Some(s) => format!(
            "\"{}\".\"{}\"",
            escape_identifier(s),
            escape_identifier(table)
        ),
        None => format!("\"{}\"", escape_identifier(table)),
    };

    // For unordered tables with a sufficiently large row-count estimate, use
    // TABLESAMPLE BERNOULLI to avoid the expensive full-table ORDER BY RANDOM().
    let use_tablesample = is_random
        && total_rows
            .and_then(|r| u64::try_from(r.max(0)).ok())
            .is_some_and(|r| r >= TABLESAMPLE_MIN_ROWS);

    let sample_query = if use_tablesample {
        // Safety: we checked total_rows is Some and >= TABLESAMPLE_MIN_ROWS above
        #[allow(clippy::cast_precision_loss)]
        let estimated = total_rows.unwrap_or(0).max(1) as f64;
        #[allow(clippy::cast_precision_loss)]
        let desired = config.sample_size as f64;
        // Oversample so LIMIT almost always has enough rows
        let pct =
            ((desired * TABLESAMPLE_OVERSAMPLING_FACTOR) / estimated * 100.0).clamp(0.01, 100.0);
        format!(
            "SELECT row_to_json(t.*) AS row_data FROM {} TABLESAMPLE BERNOULLI({:.4}) AS t LIMIT $1",
            base_table, pct
        )
    } else {
        let order_clause = generate_order_by_clause(&strategy, true); // DESC for most recent
        format!(
            "SELECT row_to_json(t.*) AS row_data FROM {} t {} LIMIT $1",
            base_table, order_clause
        )
    };

    tracing::debug!(
        "Sampling {} with query: {} (limit: {})",
        display_name,
        sample_query,
        config.sample_size
    );

    // Execute sample query
    let rows: Vec<JsonValue> = sqlx::query_scalar(&sample_query)
        .bind(config.sample_size as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            DbSurveyorError::collection_failed(
                format!("Failed to sample data from table '{}'", display_name),
                e,
            )
        })?;

    let actual_sample_size = u32::try_from(rows.len()).unwrap_or(u32::MAX);

    // Add warning if we got fewer rows than requested (table has fewer rows)
    if actual_sample_size < config.sample_size && !is_random {
        tracing::debug!(
            "Table {} has only {} rows, less than requested sample size of {}",
            display_name,
            actual_sample_size,
            config.sample_size
        );
    }

    Ok(TableSample {
        table_name: table.to_string(),
        schema_name: schema.map(str::to_string),
        rows,
        sample_size: actual_sample_size,
        total_rows: total_rows.map(|t| t.max(0) as u64),
        sampling_strategy,
        collected_at: chrono::Utc::now(),
        warnings,
        sample_status: Some(SampleStatus::Complete),
    })
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

    #[test]
    fn test_escape_identifier() {
        assert_eq!(escape_identifier("normal"), "normal");
        assert_eq!(escape_identifier("has space"), "has space");
        assert_eq!(escape_identifier(r#"has"quote"#), r#"has""quote"#);
        assert_eq!(escape_identifier(r#""""#), r#""""""#);
    }

    #[test]
    fn test_generate_order_by_clause_embedded_quotes() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec![r#"my"col"#.to_string()],
        };
        let clause = generate_order_by_clause(&strategy, true);
        assert_eq!(clause, r#"ORDER BY "my""col" DESC"#);
    }

    /// Helper to build a `Column` with sensible defaults for testing.
    fn make_column(
        name: &str,
        ordinal: u32,
        data_type: UnifiedDataType,
        is_pk: bool,
        is_auto: bool,
    ) -> Column {
        Column {
            name: name.to_string(),
            data_type,
            is_nullable: false,
            is_primary_key: is_pk,
            is_auto_increment: is_auto,
            default_value: None,
            comment: None,
            ordinal_position: ordinal,
        }
    }

    #[test]
    fn test_detect_ordering_from_columns_primary_key() {
        let columns = vec![
            make_column(
                "id",
                1,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                true,
                false,
            ),
            make_column(
                "name",
                2,
                UnifiedDataType::String { max_length: None },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::PrimaryKey {
                columns: vec!["id".to_string()]
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_composite_pk() {
        let columns = vec![
            make_column(
                "tenant_id",
                1,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                true,
                false,
            ),
            make_column(
                "name",
                2,
                UnifiedDataType::String { max_length: None },
                false,
                false,
            ),
            make_column(
                "id",
                3,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                true,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::PrimaryKey {
                columns: vec!["tenant_id".to_string(), "id".to_string()]
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_timestamp() {
        let columns = vec![
            make_column(
                "name",
                1,
                UnifiedDataType::String { max_length: None },
                false,
                false,
            ),
            make_column(
                "created_at",
                2,
                UnifiedDataType::DateTime {
                    with_timezone: true,
                },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::Timestamp {
                column: "created_at".to_string(),
                direction: SortDirection::Descending,
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_timestamp_partial_match() {
        let columns = vec![make_column(
            "record_created_date",
            1,
            UnifiedDataType::DateTime {
                with_timezone: false,
            },
            false,
            false,
        )];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::Timestamp {
                column: "record_created_date".to_string(),
                direction: SortDirection::Descending,
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_auto_increment() {
        let columns = vec![
            make_column(
                "row_id",
                1,
                UnifiedDataType::Integer {
                    bits: 64,
                    signed: true,
                },
                false,
                true,
            ),
            make_column(
                "data",
                2,
                UnifiedDataType::String { max_length: None },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::AutoIncrement {
                column: "row_id".to_string()
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_unordered() {
        let columns = vec![
            make_column(
                "data",
                1,
                UnifiedDataType::String { max_length: None },
                false,
                false,
            ),
            make_column(
                "value",
                2,
                UnifiedDataType::Float { precision: None },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(strategy, OrderingStrategy::Unordered);
    }

    #[test]
    fn test_detect_ordering_from_columns_pk_takes_priority() {
        // PK should win even when timestamp and auto-increment are present
        let columns = vec![
            make_column(
                "id",
                1,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                true,
                true,
            ),
            make_column(
                "created_at",
                2,
                UnifiedDataType::DateTime {
                    with_timezone: true,
                },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::PrimaryKey {
                columns: vec!["id".to_string()]
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_timestamp_over_auto_increment() {
        // Timestamp should win over auto-increment when no PK
        let columns = vec![
            make_column(
                "seq",
                1,
                UnifiedDataType::Integer {
                    bits: 64,
                    signed: true,
                },
                false,
                true,
            ),
            make_column(
                "created_at",
                2,
                UnifiedDataType::DateTime {
                    with_timezone: true,
                },
                false,
                false,
            ),
        ];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(
            strategy,
            OrderingStrategy::Timestamp {
                column: "created_at".to_string(),
                direction: SortDirection::Descending,
            }
        );
    }

    #[test]
    fn test_detect_ordering_from_columns_empty() {
        let strategy = detect_ordering_from_columns(&[]);
        assert_eq!(strategy, OrderingStrategy::Unordered);
    }

    #[test]
    fn test_is_timestamp_type() {
        assert!(is_timestamp_type(&UnifiedDataType::DateTime {
            with_timezone: true
        }));
        assert!(is_timestamp_type(&UnifiedDataType::DateTime {
            with_timezone: false
        }));
        assert!(is_timestamp_type(&UnifiedDataType::Date));
        assert!(!is_timestamp_type(&UnifiedDataType::String {
            max_length: None
        }));
        assert!(!is_timestamp_type(&UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }));
        assert!(!is_timestamp_type(&UnifiedDataType::Time {
            with_timezone: false
        }));
    }

    #[test]
    fn test_detect_ordering_from_columns_date_type_not_timestamp_name() {
        // A Date column named "birthday" should NOT match timestamp heuristics
        let columns = vec![make_column(
            "birthday",
            1,
            UnifiedDataType::Date,
            false,
            false,
        )];

        let strategy = detect_ordering_from_columns(&columns);
        assert_eq!(strategy, OrderingStrategy::Unordered);
    }

    #[test]
    fn test_tablesample_constants_are_sane() {
        const { assert!(TABLESAMPLE_MIN_ROWS > 0) };
        const { assert!(TABLESAMPLE_OVERSAMPLING_FACTOR >= 1.0) };
    }

    /// Verify that `TABLESAMPLE BERNOULLI` percentage calculation stays within bounds.
    #[test]
    fn test_tablesample_percentage_clamped() {
        let sample_size: f64 = 10.0;
        let estimated: f64 = 1_000_000.0;
        let pct =
            (sample_size * TABLESAMPLE_OVERSAMPLING_FACTOR / estimated * 100.0).clamp(0.01, 100.0);
        // 10 * 2 / 1_000_000 * 100 = 0.002 -> clamped to 0.01
        assert!((pct - 0.01).abs() < f64::EPSILON);

        // Very small table just above threshold
        let estimated_small: f64 = 1000.0;
        let pct_small = (sample_size * TABLESAMPLE_OVERSAMPLING_FACTOR / estimated_small * 100.0)
            .clamp(0.01, 100.0);
        // 10 * 2 / 1000 * 100 = 2.0
        assert!((pct_small - 2.0).abs() < f64::EPSILON);

        // When sample_size > total, pct should clamp to 100
        let estimated_tiny: f64 = 1.0;
        let pct_over =
            (10000.0 * TABLESAMPLE_OVERSAMPLING_FACTOR / estimated_tiny * 100.0).clamp(0.01, 100.0);
        assert!((pct_over - 100.0).abs() < f64::EPSILON);
    }
}
