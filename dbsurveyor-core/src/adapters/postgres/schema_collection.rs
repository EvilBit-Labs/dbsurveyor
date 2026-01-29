//! PostgreSQL schema collection implementation.
//!
//! This module contains all the database introspection logic for collecting
//! tables, columns, constraints, indexes, views, routines, triggers, and
//! foreign keys from PostgreSQL.

use super::PostgresAdapter;
use super::{routines, triggers, views};
use crate::Result;
use crate::adapters::helpers::RowExt;
use crate::models::*;
use sqlx::Row;
use std::collections::HashMap;

/// Main entry point for schema collection
pub(crate) async fn collect_schema(adapter: &PostgresAdapter) -> Result<DatabaseSchema> {
    let start_time = std::time::Instant::now();
    let mut warnings = Vec::new();

    tracing::info!(
        "Starting PostgreSQL schema collection for database: {}:{}",
        adapter.config.host,
        adapter.config.port.unwrap_or(5432)
    );

    // Set up session security settings
    adapter.setup_session().await?;

    // Validate that user has sufficient privileges for schema collection
    if let Err(e) = adapter.validate_schema_privileges().await {
        tracing::error!("Schema collection privilege validation failed: {}", e);
        return Err(e);
    }

    // Collect database information
    tracing::debug!("Collecting database information");
    let database_info = adapter.collect_database_info().await?;

    // Collect schemas first to understand database structure
    tracing::debug!("Enumerating database schemas");
    let schemas = match adapter.collect_schemas().await {
        Ok(schemas) => {
            tracing::info!("Found {} accessible schemas", schemas.len());
            schemas
        }
        Err(e) => {
            let warning = format!("Failed to enumerate schemas: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    // Collect tables with comprehensive metadata
    tracing::debug!("Enumerating database tables");
    let table_collection_start = std::time::Instant::now();
    let tables = match adapter.collect_tables().await {
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
    tracing::debug!("Enumerating database views");
    let collected_views = match views::collect_views(&adapter.pool).await {
        Ok(v) => {
            tracing::info!("Successfully collected {} views", v.len());
            v
        }
        Err(e) => {
            let warning = format!("Failed to collect views: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    // Collect functions
    tracing::debug!("Enumerating database functions");
    let functions = match routines::collect_functions(&adapter.pool).await {
        Ok(f) => {
            tracing::info!("Successfully collected {} functions", f.len());
            f
        }
        Err(e) => {
            let warning = format!("Failed to collect functions: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    // Collect procedures
    tracing::debug!("Enumerating database procedures");
    let procedures = match routines::collect_procedures(&adapter.pool).await {
        Ok(p) => {
            tracing::info!("Successfully collected {} procedures", p.len());
            p
        }
        Err(e) => {
            let warning = format!("Failed to collect procedures: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    // Collect triggers
    tracing::debug!("Enumerating database triggers");
    let collected_triggers = match triggers::collect_triggers(&adapter.pool).await {
        Ok(t) => {
            tracing::info!("Successfully collected {} triggers", t.len());
            t
        }
        Err(e) => {
            let warning = format!("Failed to collect triggers: {}", e);
            tracing::warn!("{}", warning);
            warnings.push(warning);
            Vec::new()
        }
    };

    // Log schema distribution for debugging
    if !schemas.is_empty() && !tables.is_empty() {
        let mut schema_table_counts = HashMap::with_capacity(schemas.len());
        for table in &tables {
            let schema_name = table.schema.as_deref().unwrap_or("public");
            *schema_table_counts.entry(schema_name).or_insert(0) += 1;
        }

        for (schema, count) in &schema_table_counts {
            tracing::debug!("Schema '{}': {} tables", schema, count);
        }
    }

    let collection_duration = start_time.elapsed();

    tracing::info!(
        "PostgreSQL schema collection completed in {:.2}s - found {} tables, {} views, {} functions, {} procedures, {} triggers across {} schemas",
        collection_duration.as_secs_f64(),
        tables.len(),
        collected_views.len(),
        functions.len(),
        procedures.len(),
        collected_triggers.len(),
        schemas.len()
    );

    // Aggregate all indexes and constraints from tables for schema-level view
    let mut all_indexes = Vec::new();
    let mut all_constraints = Vec::new();

    for table in &tables {
        all_indexes.extend(table.indexes.clone());
        all_constraints.extend(table.constraints.clone());
    }

    tracing::info!(
        "Collected {} total indexes and {} total constraints across all tables",
        all_indexes.len(),
        all_constraints.len()
    );

    Ok(DatabaseSchema {
        format_version: "1.0".to_string(),
        database_info,
        tables,
        views: collected_views,
        indexes: all_indexes,
        constraints: all_constraints,
        procedures,
        functions,
        triggers: collected_triggers,
        custom_types: Vec::new(),
        samples: None,
        collection_metadata: CollectionMetadata {
            collected_at: chrono::Utc::now(),
            collection_duration_ms: collection_duration.as_millis() as u64,
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            warnings,
        },
    })
}

impl PostgresAdapter {
    /// Collects comprehensive database information
    pub(crate) async fn collect_database_info(&self) -> Result<DatabaseInfo> {
        let version_query = "SELECT version()";
        let version: String = sqlx::query_scalar(version_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to get database version",
                    e,
                )
            })?;

        let db_info_query = r#"
            SELECT
                current_database() as name,
                COALESCE(pg_database_size(current_database()), 0) as size_bytes,
                COALESCE(pg_encoding_to_char(encoding), 'UTF8') as encoding,
                COALESCE(datcollate, 'C') as collation,
                COALESCE(r.rolname, 'unknown') as owner
            FROM pg_database d
            LEFT JOIN pg_roles r ON d.datdba = r.oid
            WHERE d.datname = current_database()
        "#;

        let row = sqlx::query(db_info_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database information query failed: {}", e);
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to query database metadata from pg_database",
                    e,
                )
            })?;

        // Extract database metadata using RowExt for consistent error handling
        let name: String = row.get_field("name", Some("pg_database"))?;
        let size_bytes: Option<i64> = row.get_field("size_bytes", Some("pg_database"))?;
        let encoding: Option<String> = row.get_field("encoding", Some("pg_database"))?;
        let collation: Option<String> = row.get_field("collation", Some("pg_database"))?;
        let owner: Option<String> = row.get_field("owner", Some("pg_database"))?;

        // Check if this is a system database
        let is_system_database = matches!(name.as_str(), "template0" | "template1" | "postgres");

        Ok(DatabaseInfo {
            name,
            version: Some(version),
            size_bytes: size_bytes.map(|s| s as u64),
            encoding,
            collation,
            owner,
            is_system_database,
            access_level: AccessLevel::Full,
            collection_status: CollectionStatus::Success,
        })
    }

    /// Collects all schemas from the database
    pub(crate) async fn collect_schemas(&self) -> Result<Vec<String>> {
        tracing::debug!("Starting schema enumeration for PostgreSQL database");

        let schema_query = r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
            AND has_schema_privilege(schema_name, 'USAGE')
            ORDER BY schema_name
        "#;

        let schema_rows = sqlx::query(schema_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to enumerate schemas: {}", e);
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.schemata - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        "Failed to enumerate database schemas",
                        e,
                    ),
                }
            })?;

        let mut schemas = Vec::new();
        for row in schema_rows {
            let schema_name: String = row.try_get("schema_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse schema name from database result",
                    e,
                )
            })?;
            schemas.push(schema_name);
        }

        tracing::info!("Successfully enumerated {} schemas", schemas.len());
        Ok(schemas)
    }

    /// Collects all tables from the database with comprehensive metadata
    pub(crate) async fn collect_tables(&self) -> Result<Vec<Table>> {
        tracing::debug!("Starting table enumeration for PostgreSQL database");

        let tables_query = r#"
            SELECT
                t.table_name,
                t.table_schema,
                t.table_type,
                obj_description(c.oid) as table_comment,
                c.reltuples::bigint as estimated_rows,
                pg_size_pretty(pg_total_relation_size(c.oid)) as table_size,
                pg_total_relation_size(c.oid) as table_size_bytes
            FROM information_schema.tables t
            LEFT JOIN pg_class c ON c.relname = t.table_name
            LEFT JOIN pg_namespace n ON n.nspname = t.table_schema AND c.relnamespace = n.oid
            WHERE t.table_type IN ('BASE TABLE', 'VIEW')
            AND t.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
            AND has_table_privilege(t.table_schema || '.' || t.table_name, 'SELECT')
            ORDER BY t.table_schema, t.table_name
        "#;

        let table_rows = sqlx::query(tables_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to enumerate tables: {}", e);
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.tables - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        "Failed to enumerate database tables",
                        e,
                    ),
                }
            })?;

        let mut tables = Vec::new();

        for row in &table_rows {
            let table_name: String = row.try_get("table_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table name from database result",
                    e,
                )
            })?;
            let schema_name: Option<String> = row.try_get("table_schema").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse schema name from database result",
                    e,
                )
            })?;
            let table_comment: Option<String> = row.try_get("table_comment").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table comment from database result",
                    e,
                )
            })?;
            let estimated_rows: Option<i64> = row.try_get("estimated_rows").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse estimated rows from database result",
                    e,
                )
            })?;

            // Collect columns for this table
            let columns = self
                .collect_table_columns(&table_name, &schema_name)
                .await?;

            // Collect primary key
            let primary_key = self
                .collect_table_primary_key(&table_name, &schema_name)
                .await?;

            // Collect foreign keys
            let foreign_keys = self
                .collect_table_foreign_keys(&table_name, &schema_name)
                .await?;

            // Collect indexes
            let indexes = self
                .collect_table_indexes(&table_name, &schema_name)
                .await?;

            // Collect constraints
            let constraints = self
                .collect_table_constraints(&table_name, &schema_name)
                .await?;

            let table = Table {
                name: table_name.clone(),
                schema: schema_name,
                columns,
                primary_key,
                foreign_keys,
                indexes,
                constraints,
                comment: table_comment,
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
    pub(crate) async fn collect_table_columns(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Column>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        let columns_query = r#"
            SELECT
                c.column_name,
                c.data_type,
                c.udt_name,
                c.character_maximum_length,
                c.numeric_precision,
                c.numeric_scale,
                c.datetime_precision,
                c.is_nullable,
                c.column_default,
                c.ordinal_position,
                col_description(pgc.oid, c.ordinal_position) as column_comment,
                c.is_identity,
                c.identity_generation,
                CASE
                    WHEN c.data_type = 'ARRAY' THEN
                        CASE
                            WHEN c.udt_name LIKE '_%' THEN substring(c.udt_name from 2)
                            ELSE c.udt_name
                        END
                    ELSE NULL
                END as array_element_type,
                CASE
                    WHEN pk.column_name IS NOT NULL THEN true
                    ELSE false
                END as is_primary_key
            FROM information_schema.columns c
            LEFT JOIN pg_class pgc ON pgc.relname = c.table_name
            LEFT JOIN pg_namespace pgn ON pgn.nspname = c.table_schema AND pgc.relnamespace = pgn.oid
            LEFT JOIN (
                SELECT
                    kcu.column_name,
                    kcu.table_name,
                    kcu.table_schema
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
            ) pk ON pk.column_name = c.column_name
                AND pk.table_name = c.table_name
                AND pk.table_schema = c.table_schema
            WHERE c.table_name = $1
            AND c.table_schema = $2
            ORDER BY c.ordinal_position
        "#;

        let column_rows = sqlx::query(columns_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to collect columns for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

        let mut columns = Vec::new();

        for row in column_rows.iter() {
            let column_name: String = row.get_field("column_name", Some(table_name))?;
            let data_type: String = row.get_field("data_type", Some(table_name))?;
            let udt_name: String = row.get_field("udt_name", Some(table_name))?;
            let character_maximum_length: Option<i32> =
                row.get_field("character_maximum_length", Some(table_name))?;
            let numeric_precision: Option<i32> =
                row.get_field("numeric_precision", Some(table_name))?;
            let numeric_scale: Option<i32> = row.get_field("numeric_scale", Some(table_name))?;
            let is_nullable: String = row.get_field("is_nullable", Some(table_name))?;
            let column_default: Option<String> =
                row.get_field("column_default", Some(table_name))?;
            let ordinal_position: i32 = row.get_field("ordinal_position", Some(table_name))?;
            let column_comment: Option<String> =
                row.get_field("column_comment", Some(table_name))?;
            let is_identity: String = row.get_field("is_identity", Some(table_name))?;
            let array_element_type: Option<String> =
                row.get_field("array_element_type", Some(table_name))?;
            let is_primary_key: bool = row.get_field("is_primary_key", Some(table_name))?;

            // Map PostgreSQL data type to unified data type
            let unified_data_type = Self::map_postgres_type_to_unified(
                &data_type,
                &udt_name,
                character_maximum_length,
                numeric_precision,
                numeric_scale,
                array_element_type.as_deref(),
            )?;

            // Determine if column is auto-increment
            let is_auto_increment = is_identity == "YES"
                || column_default.as_ref().is_some_and(|default| {
                    default.starts_with("nextval(")
                        || default.contains("_seq'::regclass)")
                        || default.contains("::regclass")
                });

            columns.push(Column {
                name: column_name,
                data_type: unified_data_type,
                is_nullable: is_nullable == "YES",
                is_primary_key,
                is_auto_increment,
                default_value: column_default,
                comment: column_comment,
                ordinal_position: ordinal_position as u32,
            });
        }

        Ok(columns)
    }

    /// Collects primary key for a specific table
    pub(crate) async fn collect_table_primary_key(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Option<PrimaryKey>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        let pk_query = r#"
            SELECT
                tc.constraint_name,
                string_agg(kcu.column_name, ',' ORDER BY kcu.ordinal_position) as columns
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'PRIMARY KEY'
            AND tc.table_name = $1
            AND tc.table_schema = $2
            GROUP BY tc.constraint_name
        "#;

        let pk_row = sqlx::query(pk_query)
            .bind(table_name)
            .bind(schema)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to collect primary key for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

        if let Some(row) = pk_row {
            let name: Option<String> = row.get_field("constraint_name", Some(table_name))?;
            let columns_str: String = row.get_field("columns", Some(table_name))?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

            Ok(Some(PrimaryKey { name, columns }))
        } else {
            Ok(None)
        }
    }

    /// Collects foreign keys for a specific table
    pub(crate) async fn collect_table_foreign_keys(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<ForeignKey>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        let fk_query = r#"
            SELECT
                con.conname::text as constraint_name,
                rc.update_rule::text,
                rc.delete_rule::text,
                a.attname::text as column_name,
                fns.nspname::text as referenced_table_schema,
                fcl.relname::text as referenced_table_name,
                fa.attname::text as referenced_column_name,
                a.attnum::integer as ordinal_position
            FROM pg_constraint con
            JOIN pg_class cl ON con.conrelid = cl.oid
            JOIN pg_namespace ns ON cl.relnamespace = ns.oid
            JOIN information_schema.referential_constraints rc
                ON con.conname = rc.constraint_name
                AND ns.nspname = rc.constraint_schema
            JOIN pg_class fcl ON con.confrelid = fcl.oid
            JOIN pg_namespace fns ON fcl.relnamespace = fns.oid
            JOIN pg_attribute a ON a.attrelid = con.conrelid
            JOIN pg_attribute fa ON fa.attrelid = con.confrelid
            WHERE con.contype = 'f'
            AND cl.relname = $1
            AND ns.nspname = $2
            AND a.attnum = ANY(con.conkey)
            AND fa.attnum = ANY(con.confkey)
            AND array_position(con.conkey, a.attnum) = array_position(con.confkey, fa.attnum)
            ORDER BY con.conname, array_position(con.conkey, a.attnum)
        "#;

        let fk_rows = sqlx::query(fk_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to collect foreign keys for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

        // Group by constraint name
        let mut fk_groups: HashMap<String, Vec<sqlx::postgres::PgRow>> = HashMap::new();
        for row in fk_rows {
            let constraint_name: String = row.get_field("constraint_name", Some(table_name))?;
            fk_groups.entry(constraint_name).or_default().push(row);
        }

        let mut foreign_keys = Vec::new();

        for (constraint_name, rows) in fk_groups {
            if rows.is_empty() {
                continue;
            }

            let first_row = &rows[0];
            let update_rule: String = first_row.get_field("update_rule", Some(table_name))?;
            let delete_rule: String = first_row.get_field("delete_rule", Some(table_name))?;
            let referenced_schema_raw: Option<String> =
                first_row.get_field("referenced_table_schema", Some(table_name))?;
            // Normalize "public" schema to None (PostgreSQL default schema convention)
            let referenced_schema = referenced_schema_raw.filter(|s| s != "public");
            let referenced_table: String =
                first_row.get_field("referenced_table_name", Some(table_name))?;

            let mut columns = Vec::new();
            let mut referenced_columns = Vec::new();

            for row in &rows {
                let column: String = row.get_field("column_name", Some(table_name))?;
                let ref_column: String =
                    row.get_field("referenced_column_name", Some(table_name))?;
                columns.push(column);
                referenced_columns.push(ref_column);
            }

            foreign_keys.push(ForeignKey {
                name: Some(constraint_name),
                columns,
                referenced_table,
                referenced_schema,
                referenced_columns,
                on_delete: super::type_mapping::map_referential_action(&delete_rule),
                on_update: super::type_mapping::map_referential_action(&update_rule),
            });
        }

        Ok(foreign_keys)
    }

    /// Collects indexes for a specific table
    pub(crate) async fn collect_table_indexes(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Index>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        let idx_query = r#"
            SELECT
                i.relname::text as index_name,
                am.amname::text as index_type,
                ix.indisunique as is_unique,
                ix.indisprimary as is_primary,
                string_agg(a.attname::text, ',' ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                pg_get_indexdef(i.oid) as index_definition
            FROM pg_index ix
            JOIN pg_class t ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_am am ON am.oid = i.relam
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE t.relname = $1
            AND n.nspname = $2
            GROUP BY i.relname, am.amname, ix.indisunique, ix.indisprimary, i.oid
            ORDER BY i.relname
        "#;

        let idx_rows = sqlx::query(idx_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to collect indexes for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

        let mut indexes = Vec::new();

        for row in idx_rows {
            let name: String = row.get_field("index_name", Some(table_name))?;
            let index_type: String = row.get_field("index_type", Some(table_name))?;
            let is_unique: bool = row.get_field("is_unique", Some(table_name))?;
            let is_primary: bool = row.get_field("is_primary", Some(table_name))?;
            let columns_str: String = row.get_field("columns", Some(table_name))?;
            let index_definition: String = row.get_field("index_definition", Some(table_name))?;

            // Parse columns with sort order from index definition
            let columns: Vec<IndexColumn> = columns_str
                .split(',')
                .map(|col_name| {
                    let col_name = col_name.trim();
                    // Check if this column has DESC in the index definition
                    // Pattern: column_name DESC or "column_name" DESC
                    let sort_order = if index_definition.contains(&format!("{} DESC", col_name))
                        || index_definition.contains(&format!("\"{}\" DESC", col_name))
                    {
                        Some(SortOrder::Descending)
                    } else {
                        Some(SortOrder::Ascending)
                    };
                    IndexColumn {
                        name: col_name.to_string(),
                        sort_order,
                    }
                })
                .collect();

            indexes.push(Index {
                name,
                table_name: table_name.to_string(),
                schema: schema_name.clone(),
                columns,
                is_unique,
                is_primary,
                index_type: Some(index_type),
            });
        }

        Ok(indexes)
    }

    /// Collects constraints for a specific table
    pub(crate) async fn collect_table_constraints(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Constraint>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        let constraints_query = r#"
            SELECT
                tc.constraint_name::text,
                tc.constraint_type::text,
                cc.check_clause::text,
                COALESCE(
                    string_agg(kcu.column_name::text, ',' ORDER BY kcu.ordinal_position),
                    ''
                )::text as column_names
            FROM information_schema.table_constraints tc
            LEFT JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            LEFT JOIN information_schema.check_constraints cc
                ON tc.constraint_name = cc.constraint_name
                AND tc.table_schema = cc.constraint_schema
            WHERE tc.table_name = $1
            AND tc.table_schema = $2
            AND tc.constraint_type IN ('CHECK', 'UNIQUE', 'PRIMARY KEY', 'FOREIGN KEY')
            GROUP BY tc.constraint_name, tc.constraint_type, cc.check_clause
            ORDER BY tc.constraint_name
        "#;

        let constraint_rows = sqlx::query(constraints_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to collect constraints for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

        let mut constraints = Vec::new();

        for row in constraint_rows {
            let name: String = row.get_field("constraint_name", Some(table_name))?;
            let constraint_type_str: String = row.get_field("constraint_type", Some(table_name))?;
            let check_clause: Option<String> = row.get_field("check_clause", Some(table_name))?;
            let columns_str: String = row.get_field("column_names", Some(table_name))?;

            let constraint_type = match constraint_type_str.as_str() {
                "CHECK" => ConstraintType::Check,
                "UNIQUE" => ConstraintType::Unique,
                "PRIMARY KEY" => ConstraintType::PrimaryKey,
                "FOREIGN KEY" => ConstraintType::ForeignKey,
                _ => ConstraintType::Check,
            };

            let columns: Vec<String> = if columns_str.is_empty() {
                Vec::new()
            } else {
                columns_str.split(',').map(|s| s.to_string()).collect()
            };

            constraints.push(Constraint {
                name,
                table_name: table_name.to_string(),
                schema: schema_name.clone(),
                constraint_type,
                columns,
                check_clause,
            });
        }

        Ok(constraints)
    }
}
