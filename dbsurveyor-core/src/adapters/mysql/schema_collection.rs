//! MySQL schema collection implementation.
//!
//! This module contains all the database introspection logic for collecting
//! tables, columns, constraints, indexes, and foreign keys from MySQL.

use super::MySqlAdapter;
use super::type_mapping::map_mysql_type;
use crate::Result;
use crate::models::*;
use sqlx::Row;
use std::collections::HashMap;

/// Maps MySQL referential action string to ReferentialAction enum
fn parse_referential_action(action: Option<String>) -> Option<ReferentialAction> {
    action.and_then(|a| match a.to_uppercase().as_str() {
        "CASCADE" => Some(ReferentialAction::Cascade),
        "SET NULL" => Some(ReferentialAction::SetNull),
        "SET DEFAULT" => Some(ReferentialAction::SetDefault),
        "RESTRICT" => Some(ReferentialAction::Restrict),
        "NO ACTION" => Some(ReferentialAction::NoAction),
        _ => None,
    })
}

/// Main entry point for schema collection
pub(crate) async fn collect_schema(adapter: &MySqlAdapter) -> Result<DatabaseSchema> {
    let start_time = std::time::Instant::now();
    let mut warnings = Vec::new();

    let db_name = adapter
        .config
        .database
        .as_deref()
        .unwrap_or("unknown")
        .to_string();

    tracing::info!(
        "Starting MySQL schema collection for database: {}:{}",
        adapter.config.host,
        adapter.config.port.unwrap_or(3306)
    );

    // Collect database information
    tracing::debug!("Collecting database information");
    let database_info = collect_database_info(adapter, &db_name).await?;

    // Collect tables with comprehensive metadata
    tracing::debug!("Enumerating database tables");
    let table_collection_start = std::time::Instant::now();
    let tables = match collect_tables(adapter, &db_name).await {
        Ok(tables) => {
            let table_collection_duration = table_collection_start.elapsed();
            tracing::info!(
                "Successfully collected {} tables in {:.2}s",
                tables.len(),
                table_collection_duration.as_secs_f64()
            );
            tables
        }
        Err(e) => {
            tracing::error!("Failed to collect tables: {}", e);
            return Err(e);
        }
    };

    // Collect views
    let views = match collect_views(adapter, &db_name).await {
        Ok(views) => {
            tracing::info!("Successfully collected {} views", views.len());
            views
        }
        Err(e) => {
            let warning = format!("Failed to collect views: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    let collection_duration = start_time.elapsed();

    tracing::info!(
        "MySQL schema collection completed in {:.2}s - found {} tables, {} views",
        collection_duration.as_secs_f64(),
        tables.len(),
        views.len()
    );

    // Aggregate all indexes and constraints from tables
    let mut all_indexes = Vec::new();
    let mut all_constraints = Vec::new();

    for table in &tables {
        all_indexes.extend(table.indexes.clone());
        all_constraints.extend(table.constraints.clone());
    }

    Ok(DatabaseSchema {
        format_version: FORMAT_VERSION.to_string(),
        database_info,
        tables,
        views,
        indexes: all_indexes,
        constraints: all_constraints,
        procedures: Vec::new(),
        functions: Vec::new(),
        triggers: Vec::new(),
        custom_types: Vec::new(),
        samples: None,
        quality_metrics: None,
        collection_metadata: CollectionMetadata {
            collected_at: chrono::Utc::now(),
            collection_duration_ms: collection_duration.as_millis() as u64,
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            warnings,
        },
    })
}

/// Collects database information from MySQL
async fn collect_database_info(adapter: &MySqlAdapter, db_name: &str) -> Result<DatabaseInfo> {
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get database version", e)
        })?;

    // Get database size and character set
    let db_info_query = r#"
        SELECT
            DEFAULT_CHARACTER_SET_NAME as charset,
            DEFAULT_COLLATION_NAME as collation
        FROM INFORMATION_SCHEMA.SCHEMATA
        WHERE SCHEMA_NAME = ?
    "#;

    let row = sqlx::query(db_info_query)
        .bind(db_name)
        .fetch_optional(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to query database metadata", e)
        })?;

    let (encoding, collation) = if let Some(row) = row {
        let charset: Option<String> = row.try_get("charset").ok();
        let collation: Option<String> = row.try_get("collation").ok();
        (charset, collation)
    } else {
        (None, None)
    };

    // Get database size (cast to SIGNED to avoid DECIMAL type issues)
    let size_query = r#"
        SELECT CAST(SUM(data_length + index_length) AS SIGNED) as size_bytes
        FROM INFORMATION_SCHEMA.TABLES
        WHERE TABLE_SCHEMA = ?
    "#;

    let size_bytes: Option<i64> = sqlx::query_scalar(size_query)
        .bind(db_name)
        .fetch_optional(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to query database size", e)
        })?
        .flatten();

    // Check if this is a system database
    let is_system_database = matches!(
        db_name.to_lowercase().as_str(),
        "mysql" | "information_schema" | "performance_schema" | "sys"
    );

    Ok(DatabaseInfo {
        name: db_name.to_string(),
        version: Some(version),
        size_bytes: size_bytes.map(|s| s as u64),
        encoding,
        collation,
        owner: None, // MySQL doesn't have per-database owners like PostgreSQL
        is_system_database,
        access_level: AccessLevel::Full,
        collection_status: CollectionStatus::Success,
    })
}

/// Collects all tables from the MySQL database
async fn collect_tables(adapter: &MySqlAdapter, db_name: &str) -> Result<Vec<Table>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let tables_query = r#"
        SELECT
            CAST(TABLE_NAME AS CHAR) as TABLE_NAME,
            CAST(TABLE_COMMENT AS CHAR) as TABLE_COMMENT,
            TABLE_ROWS,
            DATA_LENGTH,
            INDEX_LENGTH
        FROM INFORMATION_SCHEMA.TABLES
        WHERE TABLE_SCHEMA = ?
        AND TABLE_TYPE = 'BASE TABLE'
        ORDER BY TABLE_NAME
    "#;

    let table_rows = sqlx::query(tables_query)
        .bind(db_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to enumerate tables", e)
        })?;

    let mut tables = Vec::new();

    for row in &table_rows {
        let table_name: String = row.try_get("TABLE_NAME").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to parse table name", e)
        })?;
        let table_comment: Option<String> = row.try_get("TABLE_COMMENT").ok();
        let estimated_rows: Option<i64> = row.try_get("TABLE_ROWS").ok();

        // Collect columns for this table
        let columns = collect_table_columns(adapter, db_name, &table_name).await?;

        // Collect primary key
        let primary_key = collect_table_primary_key(adapter, db_name, &table_name).await?;

        // Collect foreign keys
        let foreign_keys = collect_table_foreign_keys(adapter, db_name, &table_name).await?;

        // Collect indexes
        let indexes = collect_table_indexes(adapter, db_name, &table_name).await?;

        // Collect constraints
        let constraints = collect_table_constraints(adapter, db_name, &table_name).await?;

        // Filter out empty comments (MySQL returns empty string for no comment)
        let comment = table_comment.filter(|c| !c.is_empty());

        let table = Table {
            name: table_name.clone(),
            schema: Some(db_name.to_string()),
            columns,
            primary_key,
            foreign_keys,
            indexes,
            constraints,
            comment,
            row_count: estimated_rows.map(|r| r as u64),
        };

        tracing::debug!(
            "Collected table '{}' with {} columns, {} foreign keys, {} indexes",
            table.name,
            table.columns.len(),
            table.foreign_keys.len(),
            table.indexes.len()
        );

        tables.push(table);
    }

    Ok(tables)
}

/// Collects column metadata for a specific table
async fn collect_table_columns(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
) -> Result<Vec<Column>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let columns_query = r#"
        SELECT
            CAST(c.COLUMN_NAME AS CHAR) as COLUMN_NAME,
            CAST(c.DATA_TYPE AS CHAR) as DATA_TYPE,
            CAST(c.COLUMN_TYPE AS CHAR) as COLUMN_TYPE,
            c.CHARACTER_MAXIMUM_LENGTH,
            c.NUMERIC_PRECISION,
            c.NUMERIC_SCALE,
            CAST(c.IS_NULLABLE AS CHAR) as IS_NULLABLE,
            CAST(c.COLUMN_DEFAULT AS CHAR) as COLUMN_DEFAULT,
            c.ORDINAL_POSITION,
            CAST(c.COLUMN_COMMENT AS CHAR) as COLUMN_COMMENT,
            CAST(c.EXTRA AS CHAR) as EXTRA,
            CAST(c.COLUMN_KEY AS CHAR) as COLUMN_KEY
        FROM INFORMATION_SCHEMA.COLUMNS c
        WHERE c.TABLE_SCHEMA = ?
        AND c.TABLE_NAME = ?
        ORDER BY c.ORDINAL_POSITION
    "#;

    let column_rows = sqlx::query(columns_query)
        .bind(db_name)
        .bind(table_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect columns for table '{}'", table_name),
                e,
            )
        })?;

    let mut columns = Vec::new();

    for row in column_rows.iter() {
        let column_name: String = row.try_get("COLUMN_NAME").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to parse column name", e)
        })?;
        let data_type: String = row.try_get("DATA_TYPE").unwrap_or_default();
        let column_type: String = row.try_get("COLUMN_TYPE").unwrap_or_default();
        let char_max_length: Option<i64> = row.try_get("CHARACTER_MAXIMUM_LENGTH").ok();
        let numeric_precision: Option<i64> = row.try_get("NUMERIC_PRECISION").ok();
        let numeric_scale: Option<i64> = row.try_get("NUMERIC_SCALE").ok();
        let is_nullable: String = row.try_get("IS_NULLABLE").unwrap_or_default();
        let column_default: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
        let ordinal_position: i32 = row.try_get("ORDINAL_POSITION").unwrap_or(0);
        let column_comment: Option<String> = row.try_get("COLUMN_COMMENT").ok();
        let extra: String = row.try_get("EXTRA").unwrap_or_default();
        let column_key: String = row.try_get("COLUMN_KEY").unwrap_or_default();

        // Check for unsigned in COLUMN_TYPE
        let is_unsigned = column_type.to_lowercase().contains("unsigned");
        let type_for_mapping = if is_unsigned {
            format!("{} unsigned", data_type)
        } else {
            data_type.clone()
        };

        // Map MySQL data type to unified data type
        let unified_data_type = map_mysql_type(
            &type_for_mapping,
            char_max_length.map(|l| l as u32),
            numeric_precision.map(|p| p as u8),
            numeric_scale.map(|s| s as u8),
        );

        // Filter out empty comments
        let comment = column_comment.filter(|c| !c.is_empty());

        let column = Column {
            name: column_name,
            data_type: unified_data_type,
            is_nullable: is_nullable.to_uppercase() == "YES",
            is_primary_key: column_key == "PRI",
            is_auto_increment: extra.to_lowercase().contains("auto_increment"),
            default_value: column_default,
            comment,
            ordinal_position: ordinal_position as u32,
        };

        columns.push(column);
    }

    Ok(columns)
}

/// Collects primary key for a table
async fn collect_table_primary_key(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
) -> Result<Option<PrimaryKey>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let pk_query = r#"
        SELECT
            CAST(tc.CONSTRAINT_NAME AS CHAR) as CONSTRAINT_NAME,
            CAST(kcu.COLUMN_NAME AS CHAR) as COLUMN_NAME
        FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc
        JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
            ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME
            AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA
            AND tc.TABLE_NAME = kcu.TABLE_NAME
        WHERE tc.TABLE_SCHEMA = ?
        AND tc.TABLE_NAME = ?
        AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY'
        ORDER BY kcu.ORDINAL_POSITION
    "#;

    let pk_rows = sqlx::query(pk_query)
        .bind(db_name)
        .bind(table_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect primary key for table '{}'", table_name),
                e,
            )
        })?;

    if pk_rows.is_empty() {
        return Ok(None);
    }

    let constraint_name: Option<String> = pk_rows[0].try_get("CONSTRAINT_NAME").ok();
    let mut columns = Vec::new();

    for row in pk_rows {
        let column_name: String = row.try_get("COLUMN_NAME").unwrap_or_default();
        if !column_name.is_empty() {
            columns.push(column_name);
        }
    }

    Ok(Some(PrimaryKey {
        name: constraint_name,
        columns,
    }))
}

/// Collects foreign keys for a table
async fn collect_table_foreign_keys(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
) -> Result<Vec<ForeignKey>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let fk_query = r#"
        SELECT
            CAST(kcu.CONSTRAINT_NAME AS CHAR) as CONSTRAINT_NAME,
            CAST(kcu.COLUMN_NAME AS CHAR) as COLUMN_NAME,
            CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) as REFERENCED_TABLE_SCHEMA,
            CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) as REFERENCED_TABLE_NAME,
            CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) as REFERENCED_COLUMN_NAME,
            CAST(rc.UPDATE_RULE AS CHAR) as UPDATE_RULE,
            CAST(rc.DELETE_RULE AS CHAR) as DELETE_RULE
        FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
        JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS rc
            ON kcu.CONSTRAINT_NAME = rc.CONSTRAINT_NAME
            AND kcu.TABLE_SCHEMA = rc.CONSTRAINT_SCHEMA
        WHERE kcu.TABLE_SCHEMA = ?
        AND kcu.TABLE_NAME = ?
        AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
        ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION
    "#;

    let fk_rows = sqlx::query(fk_query)
        .bind(db_name)
        .bind(table_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect foreign keys for table '{}'", table_name),
                e,
            )
        })?;

    // Group by constraint name
    let mut fk_map: HashMap<String, ForeignKey> = HashMap::new();

    for row in fk_rows {
        let constraint_name: String = row.try_get("CONSTRAINT_NAME").unwrap_or_default();
        let column_name: String = row.try_get("COLUMN_NAME").unwrap_or_default();
        let referenced_schema: Option<String> = row.try_get("REFERENCED_TABLE_SCHEMA").ok();
        let referenced_table: String = row.try_get("REFERENCED_TABLE_NAME").unwrap_or_default();
        let referenced_column: String = row.try_get("REFERENCED_COLUMN_NAME").unwrap_or_default();
        let update_rule: Option<String> = row.try_get("UPDATE_RULE").ok();
        let delete_rule: Option<String> = row.try_get("DELETE_RULE").ok();

        let fk = fk_map.entry(constraint_name.clone()).or_insert(ForeignKey {
            name: Some(constraint_name),
            columns: Vec::new(),
            referenced_table: referenced_table.clone(),
            referenced_schema,
            referenced_columns: Vec::new(),
            on_delete: parse_referential_action(delete_rule),
            on_update: parse_referential_action(update_rule),
        });

        fk.columns.push(column_name);
        fk.referenced_columns.push(referenced_column);
    }

    Ok(fk_map.into_values().collect())
}

/// Collects indexes for a table
async fn collect_table_indexes(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
) -> Result<Vec<Index>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let index_query = r#"
        SELECT
            CAST(INDEX_NAME AS CHAR) as INDEX_NAME,
            CAST(COLUMN_NAME AS CHAR) as COLUMN_NAME,
            NON_UNIQUE,
            SEQ_IN_INDEX,
            CAST(INDEX_TYPE AS CHAR) as INDEX_TYPE,
            CAST(COLLATION AS CHAR) as COLLATION
        FROM INFORMATION_SCHEMA.STATISTICS
        WHERE TABLE_SCHEMA = ?
        AND TABLE_NAME = ?
        ORDER BY INDEX_NAME, SEQ_IN_INDEX
    "#;

    let index_rows = sqlx::query(index_query)
        .bind(db_name)
        .bind(table_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect indexes for table '{}'", table_name),
                e,
            )
        })?;

    // Group by index name
    let mut index_map: HashMap<String, Index> = HashMap::new();

    for row in index_rows {
        let index_name: String = row.try_get("INDEX_NAME").unwrap_or_default();
        let column_name: String = row.try_get("COLUMN_NAME").unwrap_or_default();
        let non_unique: i32 = row.try_get("NON_UNIQUE").unwrap_or(1);
        let index_type: Option<String> = row.try_get("INDEX_TYPE").ok();
        let collation: Option<String> = row.try_get("COLLATION").ok();

        let is_primary = index_name == "PRIMARY";
        let is_unique = non_unique == 0;

        // Determine sort order from collation (A = ascending, D = descending)
        let sort_order = match collation.as_deref() {
            Some("A") => Some(SortDirection::Ascending),
            Some("D") => Some(SortDirection::Descending),
            _ => None,
        };

        let index = index_map.entry(index_name.clone()).or_insert(Index {
            name: index_name,
            table_name: table_name.to_string(),
            schema: Some(db_name.to_string()),
            columns: Vec::new(),
            is_unique,
            is_primary,
            index_type,
        });

        index.columns.push(IndexColumn {
            name: column_name,
            sort_order,
        });
    }

    Ok(index_map.into_values().collect())
}

/// Collects constraints for a table (unique, check)
async fn collect_table_constraints(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
) -> Result<Vec<Constraint>> {
    // MySQL 8.0+ supports CHECK constraints
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let constraint_query = r#"
        SELECT
            CAST(tc.CONSTRAINT_NAME AS CHAR) as CONSTRAINT_NAME,
            CAST(tc.CONSTRAINT_TYPE AS CHAR) as CONSTRAINT_TYPE,
            CAST(cc.CHECK_CLAUSE AS CHAR) as CHECK_CLAUSE
        FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc
        LEFT JOIN INFORMATION_SCHEMA.CHECK_CONSTRAINTS cc
            ON tc.CONSTRAINT_NAME = cc.CONSTRAINT_NAME
            AND tc.CONSTRAINT_SCHEMA = cc.CONSTRAINT_SCHEMA
        WHERE tc.TABLE_SCHEMA = ?
        AND tc.TABLE_NAME = ?
        AND tc.CONSTRAINT_TYPE IN ('UNIQUE', 'CHECK')
        ORDER BY tc.CONSTRAINT_NAME
    "#;

    let constraint_rows = sqlx::query(constraint_query)
        .bind(db_name)
        .bind(table_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect constraints for table '{}'", table_name),
                e,
            )
        })?;

    let mut constraints = Vec::new();

    for row in constraint_rows {
        let constraint_name: String = row.try_get("CONSTRAINT_NAME").unwrap_or_default();
        let constraint_type_str: String = row.try_get("CONSTRAINT_TYPE").unwrap_or_default();
        let check_clause: Option<String> = row.try_get("CHECK_CLAUSE").ok();

        let (constraint_type, is_unique) = match constraint_type_str.as_str() {
            "UNIQUE" => (ConstraintType::Unique, true),
            "CHECK" => (ConstraintType::Check, false),
            _ => continue, // Skip unknown constraint types
        };

        // Get columns for UNIQUE constraints
        let columns = if is_unique {
            get_constraint_columns(adapter, db_name, table_name, &constraint_name).await?
        } else {
            Vec::new()
        };

        constraints.push(Constraint {
            name: constraint_name,
            table_name: table_name.to_string(),
            schema: Some(db_name.to_string()),
            constraint_type,
            columns,
            check_clause,
        });
    }

    Ok(constraints)
}

/// Gets columns for a specific constraint
async fn get_constraint_columns(
    adapter: &MySqlAdapter,
    db_name: &str,
    table_name: &str,
    constraint_name: &str,
) -> Result<Vec<String>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let query = r#"
        SELECT CAST(COLUMN_NAME AS CHAR) as COLUMN_NAME
        FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
        WHERE TABLE_SCHEMA = ?
        AND TABLE_NAME = ?
        AND CONSTRAINT_NAME = ?
        ORDER BY ORDINAL_POSITION
    "#;

    let rows = sqlx::query(query)
        .bind(db_name)
        .bind(table_name)
        .bind(constraint_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to get constraint columns for '{}'", constraint_name),
                e,
            )
        })?;

    let mut columns = Vec::new();
    for row in rows {
        let column_name: String = row.try_get("COLUMN_NAME").unwrap_or_default();
        if !column_name.is_empty() {
            columns.push(column_name);
        }
    }

    Ok(columns)
}

/// Collects views from the MySQL database
async fn collect_views(adapter: &MySqlAdapter, db_name: &str) -> Result<Vec<View>> {
    // Cast to CHAR to avoid VARBINARY type issues in MySQL 8.0+
    let views_query = r#"
        SELECT
            CAST(TABLE_NAME AS CHAR) as TABLE_NAME,
            CAST(VIEW_DEFINITION AS CHAR) as VIEW_DEFINITION
        FROM INFORMATION_SCHEMA.VIEWS
        WHERE TABLE_SCHEMA = ?
        ORDER BY TABLE_NAME
    "#;

    let view_rows = sqlx::query(views_query)
        .bind(db_name)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to collect views", e)
        })?;

    let mut views = Vec::new();

    for row in view_rows {
        let view_name: String = row.try_get("TABLE_NAME").unwrap_or_default();
        let definition: Option<String> = row.try_get("VIEW_DEFINITION").ok();

        // Collect view columns (same as table columns)
        let columns = collect_table_columns(adapter, db_name, &view_name).await?;

        views.push(View {
            name: view_name,
            schema: Some(db_name.to_string()),
            definition,
            columns,
            comment: None,
        });
    }

    Ok(views)
}
