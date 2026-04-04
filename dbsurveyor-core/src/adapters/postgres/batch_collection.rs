//! Batch schema collection for PostgreSQL.
//!
//! Fetches columns, primary keys, foreign keys, indexes, and constraints for
//! ALL tables in a single query per data type, then groups results in memory.
//! This eliminates the N+1 query pattern where each table required 5 separate
//! queries (columns, PK, FKs, indexes, constraints).
//!
//! With 1000 tables the old pattern issued 5001+ queries; this module issues 5.

use super::PostgresAdapter;
use super::row_ext::RowExt;
use super::type_mapping;
use crate::Result;
use crate::models::*;
use sqlx::PgPool;
use std::collections::HashMap;

/// Key used to group per-table results: (schema_name, table_name).
type TableKey = (String, String);

/// System schemas excluded from batch collection.
const EXCLUDED_SCHEMAS: &[&str] = &["information_schema", "pg_catalog", "pg_toast"];

// ---------------------------------------------------------------------------
// Batch column collection
// ---------------------------------------------------------------------------

/// Fetches columns for all user tables in one query and groups by table.
pub(crate) async fn batch_collect_columns(pool: &PgPool) -> Result<HashMap<TableKey, Vec<Column>>> {
    let query = r#"
        SELECT
            c.table_schema,
            c.table_name,
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
        LEFT JOIN pg_namespace pgn
            ON pgn.nspname = c.table_schema AND pgc.relnamespace = pgn.oid
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
        WHERE c.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
        ORDER BY c.table_schema, c.table_name, c.ordinal_position
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed("Batch column collection failed", e)
    })?;

    let mut map: HashMap<TableKey, Vec<Column>> = HashMap::new();

    for row in &rows {
        let schema: String = row.get_field("table_schema", Some("batch_columns"))?;
        let table: String = row.get_field("table_name", Some("batch_columns"))?;
        let column_name: String = row.get_field("column_name", Some("batch_columns"))?;
        let data_type: String = row.get_field("data_type", Some("batch_columns"))?;
        let udt_name: String = row.get_field("udt_name", Some("batch_columns"))?;
        let char_max_len: Option<i32> =
            row.get_field("character_maximum_length", Some("batch_columns"))?;
        let numeric_prec: Option<i32> =
            row.get_field("numeric_precision", Some("batch_columns"))?;
        let numeric_sc: Option<i32> = row.get_field("numeric_scale", Some("batch_columns"))?;
        let is_nullable: String = row.get_field("is_nullable", Some("batch_columns"))?;
        let column_default: Option<String> =
            row.get_field("column_default", Some("batch_columns"))?;
        let ordinal_position: i32 = row.get_field("ordinal_position", Some("batch_columns"))?;
        let column_comment: Option<String> =
            row.get_field("column_comment", Some("batch_columns"))?;
        let is_identity: String = row.get_field("is_identity", Some("batch_columns"))?;
        let array_element_type: Option<String> =
            row.get_field("array_element_type", Some("batch_columns"))?;
        let is_primary_key: bool = row.get_field("is_primary_key", Some("batch_columns"))?;

        let unified_data_type = PostgresAdapter::map_postgres_type_to_unified(
            &data_type,
            &udt_name,
            char_max_len,
            numeric_prec,
            numeric_sc,
            array_element_type.as_deref(),
        )?;

        let is_auto_increment = is_identity == "YES"
            || column_default.as_ref().is_some_and(|default| {
                default.starts_with("nextval(")
                    || default.contains("_seq'::regclass)")
                    || default.contains("::regclass")
            });

        let col = Column {
            name: column_name,
            data_type: unified_data_type,
            is_nullable: is_nullable == "YES",
            is_primary_key,
            is_auto_increment,
            default_value: column_default,
            comment: column_comment,
            ordinal_position: u32::try_from(ordinal_position).unwrap_or(0),
        };

        map.entry((schema, table)).or_default().push(col);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Batch primary key collection
// ---------------------------------------------------------------------------

/// Fetches primary keys for all user tables in one query.
pub(crate) async fn batch_collect_primary_keys(
    pool: &PgPool,
) -> Result<HashMap<TableKey, PrimaryKey>> {
    let query = r#"
        SELECT
            tc.table_schema,
            tc.table_name,
            tc.constraint_name,
            string_agg(kcu.column_name, ',' ORDER BY kcu.ordinal_position) as columns
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
        AND tc.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
        GROUP BY tc.table_schema, tc.table_name, tc.constraint_name
        ORDER BY tc.table_schema, tc.table_name
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed("Batch primary key collection failed", e)
    })?;

    let mut map: HashMap<TableKey, PrimaryKey> = HashMap::new();

    for row in &rows {
        let schema: String = row.get_field("table_schema", Some("batch_pks"))?;
        let table: String = row.get_field("table_name", Some("batch_pks"))?;
        let name: Option<String> = row.get_field("constraint_name", Some("batch_pks"))?;
        let columns_str: String = row.get_field("columns", Some("batch_pks"))?;
        let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

        map.insert((schema, table), PrimaryKey { name, columns });
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Batch foreign key collection
// ---------------------------------------------------------------------------

/// Raw row from the batch foreign key query, before grouping by constraint.
struct FkRawRow {
    schema: String,
    table: String,
    constraint_name: String,
    update_rule: String,
    delete_rule: String,
    column_name: String,
    referenced_table_schema: Option<String>,
    referenced_table_name: String,
    referenced_column_name: String,
}

/// Fetches foreign keys for all user tables in one query.
pub(crate) async fn batch_collect_foreign_keys(
    pool: &PgPool,
) -> Result<HashMap<TableKey, Vec<ForeignKey>>> {
    let query = r#"
        SELECT
            ns.nspname::text as table_schema,
            cl.relname::text as table_name,
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
        AND ns.nspname NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
        AND a.attnum = ANY(con.conkey)
        AND fa.attnum = ANY(con.confkey)
        AND array_position(con.conkey, a.attnum) = array_position(con.confkey, fa.attnum)
        ORDER BY ns.nspname, cl.relname, con.conname, array_position(con.conkey, a.attnum)
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed("Batch foreign key collection failed", e)
    })?;

    // Parse into intermediate structs
    let mut raw_rows = Vec::with_capacity(rows.len());
    for row in &rows {
        raw_rows.push(FkRawRow {
            schema: row.get_field("table_schema", Some("batch_fks"))?,
            table: row.get_field("table_name", Some("batch_fks"))?,
            constraint_name: row.get_field("constraint_name", Some("batch_fks"))?,
            update_rule: row.get_field("update_rule", Some("batch_fks"))?,
            delete_rule: row.get_field("delete_rule", Some("batch_fks"))?,
            column_name: row.get_field("column_name", Some("batch_fks"))?,
            referenced_table_schema: row.get_field("referenced_table_schema", Some("batch_fks"))?,
            referenced_table_name: row.get_field("referenced_table_name", Some("batch_fks"))?,
            referenced_column_name: row.get_field("referenced_column_name", Some("batch_fks"))?,
        });
    }

    // Group: (schema, table) -> constraint_name -> Vec<FkRawRow>
    let mut grouped: HashMap<TableKey, HashMap<String, Vec<FkRawRow>>> = HashMap::new();
    for raw in raw_rows {
        let key = (raw.schema.clone(), raw.table.clone());
        let constraint = raw.constraint_name.clone();
        grouped
            .entry(key)
            .or_default()
            .entry(constraint)
            .or_default()
            .push(raw);
    }

    let mut map: HashMap<TableKey, Vec<ForeignKey>> = HashMap::new();

    for (table_key, constraints) in grouped {
        let mut fks = Vec::new();
        for (constraint_name, fk_rows) in constraints {
            if fk_rows.is_empty() {
                continue;
            }
            let first = &fk_rows[0];
            let referenced_schema = first
                .referenced_table_schema
                .as_ref()
                .filter(|s| s.as_str() != "public")
                .cloned();
            let referenced_table = first.referenced_table_name.clone();
            let update_rule = &first.update_rule;
            let delete_rule = &first.delete_rule;

            let mut columns = Vec::new();
            let mut referenced_columns = Vec::new();
            for r in &fk_rows {
                columns.push(r.column_name.clone());
                referenced_columns.push(r.referenced_column_name.clone());
            }

            fks.push(ForeignKey {
                name: Some(constraint_name),
                columns,
                referenced_table,
                referenced_schema,
                referenced_columns,
                on_delete: type_mapping::map_referential_action(delete_rule),
                on_update: type_mapping::map_referential_action(update_rule),
            });
        }
        map.insert(table_key, fks);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Batch index collection
// ---------------------------------------------------------------------------

/// Fetches indexes for all user tables in one query.
pub(crate) async fn batch_collect_indexes(pool: &PgPool) -> Result<HashMap<TableKey, Vec<Index>>> {
    let query = r#"
        SELECT
            n.nspname::text as table_schema,
            t.relname::text as table_name,
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
        WHERE n.nspname NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
        GROUP BY n.nspname, t.relname, i.relname, am.amname, ix.indisunique, ix.indisprimary, i.oid
        ORDER BY n.nspname, t.relname, i.relname
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed("Batch index collection failed", e)
    })?;

    let mut map: HashMap<TableKey, Vec<Index>> = HashMap::new();

    for row in &rows {
        let schema: String = row.get_field("table_schema", Some("batch_indexes"))?;
        let table: String = row.get_field("table_name", Some("batch_indexes"))?;
        let name: String = row.get_field("index_name", Some("batch_indexes"))?;
        let index_type: String = row.get_field("index_type", Some("batch_indexes"))?;
        let is_unique: bool = row.get_field("is_unique", Some("batch_indexes"))?;
        let is_primary: bool = row.get_field("is_primary", Some("batch_indexes"))?;
        let columns_str: String = row.get_field("columns", Some("batch_indexes"))?;
        let index_definition: String = row.get_field("index_definition", Some("batch_indexes"))?;

        let columns: Vec<IndexColumn> = columns_str
            .split(',')
            .map(|col_name| {
                let col_name = col_name.trim();
                let sort_order = if index_definition.contains(&format!("{col_name} DESC"))
                    || index_definition.contains(&format!("\"{col_name}\" DESC"))
                {
                    Some(SortDirection::Descending)
                } else {
                    Some(SortDirection::Ascending)
                };
                IndexColumn {
                    name: col_name.to_string(),
                    sort_order,
                }
            })
            .collect();

        let idx = Index {
            name,
            table_name: table.clone(),
            schema: Some(schema.clone()),
            columns,
            is_unique,
            is_primary,
            index_type: Some(index_type),
        };

        map.entry((schema, table)).or_default().push(idx);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Batch constraint collection
// ---------------------------------------------------------------------------

/// Fetches constraints for all user tables in one query.
pub(crate) async fn batch_collect_constraints(
    pool: &PgPool,
) -> Result<HashMap<TableKey, Vec<Constraint>>> {
    let query = r#"
        SELECT
            tc.table_schema::text,
            tc.table_name::text,
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
        WHERE tc.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
        AND tc.constraint_type IN ('CHECK', 'UNIQUE', 'PRIMARY KEY', 'FOREIGN KEY')
        GROUP BY tc.table_schema, tc.table_name, tc.constraint_name,
                 tc.constraint_type, cc.check_clause
        ORDER BY tc.table_schema, tc.table_name, tc.constraint_name
    "#;

    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        crate::error::DbSurveyorError::collection_failed("Batch constraint collection failed", e)
    })?;

    let mut map: HashMap<TableKey, Vec<Constraint>> = HashMap::new();

    for row in &rows {
        let schema: String = row.get_field("table_schema", Some("batch_constraints"))?;
        let table: String = row.get_field("table_name", Some("batch_constraints"))?;
        let name: String = row.get_field("constraint_name", Some("batch_constraints"))?;
        let constraint_type_str: String =
            row.get_field("constraint_type", Some("batch_constraints"))?;
        let check_clause: Option<String> =
            row.get_field("check_clause", Some("batch_constraints"))?;
        let columns_str: String = row.get_field("column_names", Some("batch_constraints"))?;

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

        let constraint = Constraint {
            name,
            table_name: table.clone(),
            schema: Some(schema.clone()),
            constraint_type,
            columns,
            check_clause,
        };

        map.entry((schema, table)).or_default().push(constraint);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Orchestrator: run all 5 batch queries concurrently
// ---------------------------------------------------------------------------

/// Results of the batch collection: all per-table data pre-grouped by (schema, table).
pub(crate) struct BatchCollectionResult {
    pub columns: HashMap<TableKey, Vec<Column>>,
    pub primary_keys: HashMap<TableKey, PrimaryKey>,
    pub foreign_keys: HashMap<TableKey, Vec<ForeignKey>>,
    pub indexes: HashMap<TableKey, Vec<Index>>,
    pub constraints: HashMap<TableKey, Vec<Constraint>>,
}

/// Run all 5 batch collection queries concurrently and return grouped results.
///
/// If any individual batch query fails, the entire batch is considered failed
/// so the caller can fall back to per-table collection.
pub(crate) async fn collect_all_batch(pool: &PgPool) -> Result<BatchCollectionResult> {
    tracing::info!("Starting batch schema collection (5 queries for all tables)");
    let start = std::time::Instant::now();

    let (columns_res, pks_res, fks_res, indexes_res, constraints_res) = tokio::join!(
        batch_collect_columns(pool),
        batch_collect_primary_keys(pool),
        batch_collect_foreign_keys(pool),
        batch_collect_indexes(pool),
        batch_collect_constraints(pool),
    );

    let result = BatchCollectionResult {
        columns: columns_res?,
        primary_keys: pks_res?,
        foreign_keys: fks_res?,
        indexes: indexes_res?,
        constraints: constraints_res?,
    };

    let elapsed = start.elapsed();
    tracing::info!(
        "Batch schema collection completed in {:.2}s (columns for {} tables, {} PKs, {} FK groups, {} index groups, {} constraint groups)",
        elapsed.as_secs_f64(),
        result.columns.len(),
        result.primary_keys.len(),
        result.foreign_keys.len(),
        result.indexes.len(),
        result.constraints.len(),
    );

    Ok(result)
}

/// Assemble a `Table` from the pre-fetched batch data.
///
/// Looks up (schema, table_name) in each HashMap and returns owned data.
/// Missing entries produce empty vectors / None (not errors).
pub(crate) fn assemble_table_from_batch(
    batch: &mut BatchCollectionResult,
    table_name: &str,
    schema_name: &Option<String>,
    comment: Option<String>,
    estimated_rows: Option<i64>,
) -> Table {
    let schema = schema_name.as_deref().unwrap_or("public").to_string();
    let key = (schema, table_name.to_string());

    let columns = batch.columns.remove(&key).unwrap_or_default();
    let primary_key = batch.primary_keys.remove(&key);
    let foreign_keys = batch.foreign_keys.remove(&key).unwrap_or_default();
    let indexes = batch.indexes.remove(&key).unwrap_or_default();
    let constraints = batch.constraints.remove(&key).unwrap_or_default();

    Table {
        name: table_name.to_string(),
        schema: schema_name.clone(),
        columns,
        primary_key,
        foreign_keys,
        indexes,
        constraints,
        comment,
        row_count: estimated_rows.map(|r| r.max(0) as u64),
    }
}

// Suppress unused warning for the constant -- it documents intent even if
// the WHERE clauses inline the list.
#[allow(dead_code)]
const _: &[&str] = EXCLUDED_SCHEMAS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_table_defaults_on_empty_batch() {
        let mut batch = BatchCollectionResult {
            columns: HashMap::new(),
            primary_keys: HashMap::new(),
            foreign_keys: HashMap::new(),
            indexes: HashMap::new(),
            constraints: HashMap::new(),
        };

        let table = assemble_table_from_batch(
            &mut batch,
            "nonexistent",
            &Some("public".to_string()),
            Some("test comment".to_string()),
            Some(42),
        );

        assert_eq!(table.name, "nonexistent");
        assert_eq!(table.schema, Some("public".to_string()));
        assert_eq!(table.comment, Some("test comment".to_string()));
        assert_eq!(table.row_count, Some(42));
        assert!(table.columns.is_empty());
        assert!(table.primary_key.is_none());
        assert!(table.foreign_keys.is_empty());
        assert!(table.indexes.is_empty());
        assert!(table.constraints.is_empty());
    }

    #[test]
    fn assemble_table_uses_public_schema_default() {
        let mut batch = BatchCollectionResult {
            columns: HashMap::new(),
            primary_keys: HashMap::new(),
            foreign_keys: HashMap::new(),
            indexes: HashMap::new(),
            constraints: HashMap::new(),
        };

        // Insert data under ("public", "users")
        batch.primary_keys.insert(
            ("public".to_string(), "users".to_string()),
            PrimaryKey {
                name: Some("users_pkey".to_string()),
                columns: vec!["id".to_string()],
            },
        );

        // schema_name is None -> should default to "public" for the lookup key
        let table = assemble_table_from_batch(&mut batch, "users", &None, None, None);

        assert!(table.primary_key.is_some());
        assert_eq!(
            table.primary_key.unwrap().name,
            Some("users_pkey".to_string())
        );
    }

    #[test]
    fn assemble_table_negative_rows_clamped_to_zero() {
        let mut batch = BatchCollectionResult {
            columns: HashMap::new(),
            primary_keys: HashMap::new(),
            foreign_keys: HashMap::new(),
            indexes: HashMap::new(),
            constraints: HashMap::new(),
        };

        let table =
            assemble_table_from_batch(&mut batch, "t", &Some("public".to_string()), None, Some(-5));

        assert_eq!(table.row_count, Some(0));
    }
}
