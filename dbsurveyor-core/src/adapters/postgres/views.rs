//! PostgreSQL view collection implementation.
//!
//! This module handles collection of database views from PostgreSQL,
//! including view definitions and column metadata.

use crate::Result;
use crate::adapters::helpers::RowExt;
use crate::models::{Column, View};
use sqlx::PgPool;

/// Collects all views from the PostgreSQL database.
///
/// This function queries `pg_views` to enumerate all user-defined views,
/// excluding system schemas (pg_catalog, information_schema).
///
/// # Arguments
/// * `pool` - PostgreSQL connection pool
///
/// # Returns
/// A vector of `View` structs containing view metadata and column information.
pub async fn collect_views(pool: &PgPool) -> Result<Vec<View>> {
    tracing::debug!("Starting view collection for PostgreSQL database");

    let views_query = r#"
        SELECT
            v.schemaname::text as schema_name,
            v.viewname::text as view_name,
            v.definition::text as view_definition,
            obj_description(c.oid)::text as view_comment
        FROM pg_views v
        LEFT JOIN pg_class c ON c.relname = v.viewname
        LEFT JOIN pg_namespace n ON n.nspname = v.schemaname AND c.relnamespace = n.oid
        WHERE v.schemaname NOT IN ('pg_catalog', 'information_schema')
        ORDER BY v.schemaname, v.viewname
    "#;

    let view_rows = sqlx::query(views_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enumerate views: {}", e);
            crate::error::DbSurveyorError::collection_failed(
                "Failed to enumerate database views",
                e,
            )
        })?;

    let mut views = Vec::new();

    for row in &view_rows {
        let schema_name: Option<String> = row.get_field("schema_name", Some("pg_views"))?;
        let view_name: String = row.get_field("view_name", Some("pg_views"))?;
        let definition: Option<String> = row.get_field("view_definition", Some("pg_views"))?;
        let comment: Option<String> = row.get_field("view_comment", Some("pg_views"))?;

        // Collect view columns
        let columns = collect_view_columns(pool, &view_name, &schema_name).await?;

        views.push(View {
            name: view_name.clone(),
            schema: schema_name,
            definition,
            columns,
            comment,
        });

        tracing::debug!(
            "Collected view '{}' with {} columns",
            view_name,
            views.last().map(|v| v.columns.len()).unwrap_or(0)
        );
    }

    tracing::info!("Successfully collected {} views", views.len());
    Ok(views)
}

/// Collects column metadata for a specific view.
///
/// # Arguments
/// * `pool` - PostgreSQL connection pool
/// * `view_name` - Name of the view
/// * `schema_name` - Optional schema name
///
/// # Returns
/// A vector of `Column` structs for the view.
async fn collect_view_columns(
    pool: &PgPool,
    view_name: &str,
    schema_name: &Option<String>,
) -> Result<Vec<Column>> {
    let schema = schema_name.as_deref().unwrap_or("public");

    let columns_query = r#"
        SELECT
            c.column_name::text,
            c.data_type::text,
            c.udt_name::text,
            c.character_maximum_length,
            c.numeric_precision,
            c.numeric_scale,
            c.is_nullable::text,
            c.column_default::text,
            c.ordinal_position::integer,
            col_description(pgc.oid, c.ordinal_position)::text as column_comment
        FROM information_schema.columns c
        LEFT JOIN pg_class pgc ON pgc.relname = c.table_name
        LEFT JOIN pg_namespace pgn ON pgn.nspname = c.table_schema AND pgc.relnamespace = pgn.oid
        WHERE c.table_name = $1
        AND c.table_schema = $2
        ORDER BY c.ordinal_position
    "#;

    let column_rows = sqlx::query(columns_query)
        .bind(view_name)
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to collect columns for view '{}.{}'",
                    schema, view_name
                ),
                e,
            )
        })?;

    let mut columns = Vec::new();

    for row in column_rows.iter() {
        let column_name: String = row.get_field("column_name", Some(view_name))?;
        let data_type: String = row.get_field("data_type", Some(view_name))?;
        let udt_name: String = row.get_field("udt_name", Some(view_name))?;
        let character_maximum_length: Option<i32> =
            row.get_field("character_maximum_length", Some(view_name))?;
        let numeric_precision: Option<i32> = row.get_field("numeric_precision", Some(view_name))?;
        let numeric_scale: Option<i32> = row.get_field("numeric_scale", Some(view_name))?;
        let is_nullable: String = row.get_field("is_nullable", Some(view_name))?;
        let column_default: Option<String> = row.get_field("column_default", Some(view_name))?;
        let ordinal_position: i32 = row.get_field("ordinal_position", Some(view_name))?;
        let column_comment: Option<String> = row.get_field("column_comment", Some(view_name))?;

        // Map PostgreSQL data type to unified data type
        let unified_data_type = super::PostgresAdapter::map_postgres_type_to_unified(
            &data_type,
            &udt_name,
            character_maximum_length,
            numeric_precision,
            numeric_scale,
            None, // Views don't typically have array element type info here
        )?;

        columns.push(Column {
            name: column_name,
            data_type: unified_data_type,
            is_nullable: is_nullable == "YES",
            is_primary_key: false, // Views don't have primary keys
            is_auto_increment: false,
            default_value: column_default,
            comment: column_comment,
            ordinal_position: ordinal_position as u32,
        });
    }

    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_struct_creation() {
        let view = View {
            name: "test_view".to_string(),
            schema: Some("public".to_string()),
            definition: Some("SELECT * FROM users".to_string()),
            columns: vec![],
            comment: Some("Test view".to_string()),
        };

        assert_eq!(view.name, "test_view");
        assert_eq!(view.schema, Some("public".to_string()));
        assert!(view.definition.is_some());
    }
}
