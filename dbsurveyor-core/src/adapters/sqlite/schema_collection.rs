//! SQLite schema collection implementation.
//!
//! This module contains all the database introspection logic for collecting
//! tables, columns, constraints, indexes, views, and triggers from SQLite.
//!
//! # SQLite System Tables
//! - `sqlite_master`: Contains schema definitions for all database objects
//! - `PRAGMA table_info()`: Returns column information for a table
//! - `PRAGMA foreign_key_list()`: Returns foreign key information
//! - `PRAGMA index_list()`: Returns index information
//! - `PRAGMA index_info()`: Returns columns in an index

use super::SqliteAdapter;
use super::type_mapping::map_sqlite_type;
use crate::Result;
use crate::models::*;
use sqlx::Row;
use std::collections::HashMap;

/// Main entry point for schema collection.
pub(crate) async fn collect_schema(adapter: &SqliteAdapter) -> Result<DatabaseSchema> {
    let start_time = std::time::Instant::now();
    let mut warnings = Vec::new();

    let db_name = adapter
        .config
        .database
        .as_deref()
        .unwrap_or("main")
        .to_string();

    tracing::info!(
        "Starting SQLite schema collection for database: {}",
        db_name
    );

    // Collect database information
    tracing::debug!("Collecting database information");
    let database_info = collect_database_info(adapter, &db_name).await?;

    // Collect tables with comprehensive metadata
    tracing::debug!("Enumerating database tables");
    let table_collection_start = std::time::Instant::now();
    let tables = match collect_tables(adapter).await {
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
    let views = match collect_views(adapter).await {
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

    // Collect triggers
    let triggers = match collect_triggers(adapter).await {
        Ok(triggers) => {
            tracing::info!("Successfully collected {} triggers", triggers.len());
            triggers
        }
        Err(e) => {
            let warning = format!("Failed to collect triggers: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    let collection_duration = start_time.elapsed();

    tracing::info!(
        "SQLite schema collection completed in {:.2}s - found {} tables, {} views, {} triggers",
        collection_duration.as_secs_f64(),
        tables.len(),
        views.len(),
        triggers.len()
    );

    // Aggregate all indexes and constraints from tables
    let mut all_indexes = Vec::new();
    let mut all_constraints = Vec::new();

    for table in &tables {
        all_indexes.extend(table.indexes.clone());
        all_constraints.extend(table.constraints.clone());
    }

    Ok(DatabaseSchema {
        format_version: "1.0".to_string(),
        database_info,
        tables,
        views,
        indexes: all_indexes,
        constraints: all_constraints,
        procedures: Vec::new(), // SQLite doesn't have stored procedures
        functions: Vec::new(),  // SQLite doesn't have user-defined functions in schema
        triggers,
        custom_types: Vec::new(), // SQLite doesn't have custom types
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

/// Collects database information from SQLite.
async fn collect_database_info(adapter: &SqliteAdapter, db_name: &str) -> Result<DatabaseInfo> {
    // Get SQLite version
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get SQLite version", e)
        })?;

    // Get database size if file-based
    let size_bytes = if !adapter.is_in_memory() {
        // Query page_count and page_size to calculate size
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(&adapter.pool)
            .await
            .unwrap_or(0);
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&adapter.pool)
            .await
            .unwrap_or(4096);
        Some((page_count * page_size) as u64)
    } else {
        None
    };

    // Get encoding
    let encoding: String = sqlx::query_scalar("PRAGMA encoding")
        .fetch_one(&adapter.pool)
        .await
        .unwrap_or_else(|_| "UTF-8".to_string());

    Ok(DatabaseInfo {
        name: db_name.to_string(),
        version: Some(format!("SQLite {}", version)),
        size_bytes,
        encoding: Some(encoding),
        collation: None, // SQLite uses per-column collation
        owner: None,     // SQLite doesn't have database owners
        is_system_database: false,
        access_level: AccessLevel::Full,
        collection_status: CollectionStatus::Success,
    })
}

/// Collects all tables from the SQLite database.
async fn collect_tables(adapter: &SqliteAdapter) -> Result<Vec<Table>> {
    // Query sqlite_master for table definitions
    let tables_query = r#"
        SELECT name, sql
        FROM sqlite_master
        WHERE type = 'table'
        AND name NOT LIKE 'sqlite_%'
        ORDER BY name
    "#;

    let table_rows = sqlx::query(tables_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to enumerate tables", e)
        })?;

    let mut tables = Vec::new();

    for row in &table_rows {
        let table_name: String = row.try_get("name").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to parse table name", e)
        })?;

        // Collect columns for this table
        let columns = collect_table_columns(adapter, &table_name).await?;

        // Collect primary key
        let primary_key = detect_primary_key(&columns);

        // Collect foreign keys
        let foreign_keys = collect_table_foreign_keys(adapter, &table_name).await?;

        // Collect indexes
        let indexes = collect_table_indexes(adapter, &table_name).await?;

        // Collect constraints
        let constraints = collect_table_constraints(&columns, &table_name);

        // Get row count estimate
        let row_count = get_table_row_count(adapter, &table_name).await.ok();

        let table = Table {
            name: table_name.clone(),
            schema: None, // SQLite doesn't have schemas in the PostgreSQL sense
            columns,
            primary_key,
            foreign_keys,
            indexes,
            constraints,
            comment: None, // SQLite doesn't support table comments
            row_count,
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

/// Collects column metadata for a specific table.
async fn collect_table_columns(adapter: &SqliteAdapter, table_name: &str) -> Result<Vec<Column>> {
    // Use PRAGMA table_info to get column details
    let columns_query = format!("PRAGMA table_info('{}')", table_name.replace('\'', "''"));

    let column_rows = sqlx::query(&columns_query)
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
        let cid: i32 = row.try_get("cid").unwrap_or(0);
        let name: String = row.try_get("name").unwrap_or_default();
        let data_type: String = row.try_get("type").unwrap_or_default();
        let notnull: i32 = row.try_get("notnull").unwrap_or(0);
        let default_value: Option<String> = row.try_get("dflt_value").ok();
        let pk: i32 = row.try_get("pk").unwrap_or(0);

        // Map SQLite data type to unified data type
        let unified_data_type = map_sqlite_type(&data_type);

        // Check if column is auto-increment (INTEGER PRIMARY KEY is auto-increment in SQLite)
        let is_auto_increment = pk > 0
            && data_type.to_uppercase().contains("INTEGER")
            && name.to_uppercase() != "ROWID";

        // In SQLite, PRIMARY KEY columns are implicitly NOT NULL (even though PRAGMA may report otherwise)
        // except for INTEGER PRIMARY KEY which technically can be NULL but in practice aliases to ROWID
        let is_nullable = notnull == 0 && pk == 0;

        let column = Column {
            name,
            data_type: unified_data_type,
            is_nullable,
            is_primary_key: pk > 0,
            is_auto_increment,
            default_value,
            comment: None, // SQLite doesn't support column comments
            ordinal_position: cid as u32,
        };

        columns.push(column);
    }

    Ok(columns)
}

/// Detects primary key from column information.
fn detect_primary_key(columns: &[Column]) -> Option<PrimaryKey> {
    let pk_columns: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    if pk_columns.is_empty() {
        None
    } else {
        Some(PrimaryKey {
            name: None, // SQLite doesn't name primary key constraints
            columns: pk_columns,
        })
    }
}

/// Collects foreign keys for a table.
async fn collect_table_foreign_keys(
    adapter: &SqliteAdapter,
    table_name: &str,
) -> Result<Vec<ForeignKey>> {
    let fk_query = format!(
        "PRAGMA foreign_key_list('{}')",
        table_name.replace('\'', "''")
    );

    let fk_rows = sqlx::query(&fk_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect foreign keys for table '{}'", table_name),
                e,
            )
        })?;

    // Group by FK id (composite foreign keys have the same id)
    let mut fk_map: HashMap<i32, ForeignKey> = HashMap::new();

    for row in fk_rows {
        let id: i32 = row.try_get("id").unwrap_or(0);
        let seq: i32 = row.try_get("seq").unwrap_or(0);
        let ref_table: String = row.try_get("table").unwrap_or_default();
        let from_col: String = row.try_get("from").unwrap_or_default();
        let to_col: String = row.try_get("to").unwrap_or_default();
        let on_update: String = row.try_get("on_update").unwrap_or_default();
        let on_delete: String = row.try_get("on_delete").unwrap_or_default();

        let fk = fk_map.entry(id).or_insert(ForeignKey {
            name: None, // SQLite doesn't name FK constraints
            columns: Vec::new(),
            referenced_table: ref_table,
            referenced_schema: None,
            referenced_columns: Vec::new(),
            on_delete: parse_referential_action(&on_delete),
            on_update: parse_referential_action(&on_update),
        });

        // Insert at correct position for composite keys
        if seq as usize >= fk.columns.len() {
            fk.columns.push(from_col);
            fk.referenced_columns.push(to_col);
        } else {
            fk.columns.insert(seq as usize, from_col);
            fk.referenced_columns.insert(seq as usize, to_col);
        }
    }

    Ok(fk_map.into_values().collect())
}

/// Parses SQLite referential action string.
fn parse_referential_action(action: &str) -> Option<ReferentialAction> {
    match action.to_uppercase().as_str() {
        "CASCADE" => Some(ReferentialAction::Cascade),
        "SET NULL" => Some(ReferentialAction::SetNull),
        "SET DEFAULT" => Some(ReferentialAction::SetDefault),
        "RESTRICT" => Some(ReferentialAction::Restrict),
        "NO ACTION" | "" => Some(ReferentialAction::NoAction),
        _ => None,
    }
}

/// Collects indexes for a table.
async fn collect_table_indexes(adapter: &SqliteAdapter, table_name: &str) -> Result<Vec<Index>> {
    let index_list_query = format!("PRAGMA index_list('{}')", table_name.replace('\'', "''"));

    let index_rows = sqlx::query(&index_list_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect indexes for table '{}'", table_name),
                e,
            )
        })?;

    let mut indexes = Vec::new();

    for row in index_rows {
        let index_name: String = row.try_get("name").unwrap_or_default();
        let is_unique: i32 = row.try_get("unique").unwrap_or(0);
        let origin: String = row.try_get("origin").unwrap_or_default();

        // Skip auto-created indexes (pk = primary key, u = unique constraint)
        let is_primary = origin == "pk";

        // Get index columns
        let columns = collect_index_columns(adapter, &index_name).await?;

        let index = Index {
            name: index_name,
            table_name: table_name.to_string(),
            schema: None,
            columns,
            is_unique: is_unique != 0,
            is_primary,
            index_type: Some("btree".to_string()), // SQLite uses B-tree indexes
        };

        indexes.push(index);
    }

    Ok(indexes)
}

/// Collects columns for a specific index.
async fn collect_index_columns(
    adapter: &SqliteAdapter,
    index_name: &str,
) -> Result<Vec<IndexColumn>> {
    let index_info_query = format!("PRAGMA index_info('{}')", index_name.replace('\'', "''"));

    let column_rows = sqlx::query(&index_info_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to collect index columns for '{}'", index_name),
                e,
            )
        })?;

    let mut columns = Vec::new();

    for row in column_rows {
        let name: String = row.try_get("name").unwrap_or_default();

        // SQLite doesn't store sort order in index_info, default to ascending
        columns.push(IndexColumn {
            name,
            sort_order: Some(SortOrder::Ascending),
        });
    }

    Ok(columns)
}

/// Collects constraints from column information.
fn collect_table_constraints(columns: &[Column], table_name: &str) -> Vec<Constraint> {
    let mut constraints = Vec::new();

    // Add NOT NULL constraints
    for column in columns {
        if !column.is_nullable && !column.is_primary_key {
            constraints.push(Constraint {
                name: format!("{}_{}_{}", table_name, column.name, "notnull"),
                table_name: table_name.to_string(),
                schema: None,
                constraint_type: ConstraintType::NotNull,
                columns: vec![column.name.clone()],
                check_clause: None,
            });
        }
    }

    constraints
}

/// Gets estimated row count for a table.
async fn get_table_row_count(adapter: &SqliteAdapter, table_name: &str) -> Result<u64> {
    // Use a simple COUNT(*) for small tables
    // For larger tables, this could be slow, but SQLite doesn't have table statistics
    let query = format!(
        "SELECT COUNT(*) FROM \"{}\"",
        table_name.replace('"', "\"\"")
    );

    let count: i64 = sqlx::query_scalar(&query)
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!("Failed to get row count for table '{}'", table_name),
                e,
            )
        })?;

    Ok(count as u64)
}

/// Collects views from the SQLite database.
async fn collect_views(adapter: &SqliteAdapter) -> Result<Vec<View>> {
    let views_query = r#"
        SELECT name, sql
        FROM sqlite_master
        WHERE type = 'view'
        AND name NOT LIKE 'sqlite_%'
        ORDER BY name
    "#;

    let view_rows = sqlx::query(views_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to collect views", e)
        })?;

    let mut views = Vec::new();

    for row in view_rows {
        let view_name: String = row.try_get("name").unwrap_or_default();
        let definition: Option<String> = row.try_get("sql").ok();

        // Collect view columns (same method as table columns)
        let columns = collect_table_columns(adapter, &view_name)
            .await
            .unwrap_or_default();

        views.push(View {
            name: view_name,
            schema: None,
            definition,
            columns,
            comment: None,
        });
    }

    Ok(views)
}

/// Collects triggers from the SQLite database.
async fn collect_triggers(adapter: &SqliteAdapter) -> Result<Vec<Trigger>> {
    let triggers_query = r#"
        SELECT name, tbl_name, sql
        FROM sqlite_master
        WHERE type = 'trigger'
        AND name NOT LIKE 'sqlite_%'
        ORDER BY name
    "#;

    let trigger_rows = sqlx::query(triggers_query)
        .fetch_all(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to collect triggers", e)
        })?;

    let mut triggers = Vec::new();

    for row in trigger_rows {
        let trigger_name: String = row.try_get("name").unwrap_or_default();
        let table_name: String = row.try_get("tbl_name").unwrap_or_default();
        let definition: Option<String> = row.try_get("sql").ok();

        // Parse trigger timing and event from SQL definition
        let (timing, event) = parse_trigger_definition(definition.as_deref());

        triggers.push(Trigger {
            name: trigger_name,
            table_name,
            schema: None,
            event,
            timing,
            definition,
        });
    }

    Ok(triggers)
}

/// Parses trigger definition to extract timing and event.
fn parse_trigger_definition(sql: Option<&str>) -> (TriggerTiming, TriggerEvent) {
    let default_timing = TriggerTiming::After;
    let default_event = TriggerEvent::Insert;

    let sql = match sql {
        Some(s) => s.to_uppercase(),
        None => return (default_timing, default_event),
    };

    // Determine timing
    let timing = if sql.contains("BEFORE") {
        TriggerTiming::Before
    } else if sql.contains("INSTEAD OF") {
        TriggerTiming::InsteadOf
    } else {
        TriggerTiming::After
    };

    // Determine event
    let event = if sql.contains("INSERT") {
        TriggerEvent::Insert
    } else if sql.contains("UPDATE") {
        TriggerEvent::Update
    } else if sql.contains("DELETE") {
        TriggerEvent::Delete
    } else {
        default_event
    };

    (timing, event)
}
