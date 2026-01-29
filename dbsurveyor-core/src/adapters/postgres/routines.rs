//! PostgreSQL procedures and functions collection implementation.
//!
//! This module handles collection of stored procedures and functions from PostgreSQL,
//! including definitions, parameters, and return types.
//!
//! # PostgreSQL Routine Types
//! - `FUNCTION` - Returns a value, can be used in expressions
//! - `PROCEDURE` - Does not return a value (PostgreSQL 11+)

use crate::Result;
use crate::adapters::helpers::RowExt;
use crate::models::{Parameter, ParameterDirection, Procedure, UnifiedDataType};
use sqlx::PgPool;

/// Collects all functions from the PostgreSQL database.
///
/// This function queries `pg_proc` to enumerate all user-defined functions,
/// excluding system schemas (pg_catalog, information_schema) and aggregate/window functions.
///
/// # Arguments
/// * `pool` - PostgreSQL connection pool
///
/// # Returns
/// A vector of `Procedure` structs containing function metadata.
pub async fn collect_functions(pool: &PgPool) -> Result<Vec<Procedure>> {
    tracing::debug!("Starting function collection for PostgreSQL database");

    // Check PostgreSQL version to determine which query to use
    // prokind was added in PostgreSQL 11
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get PostgreSQL version", e)
        })?;

    let major_version = extract_pg_major_version(&version);

    // Use version-appropriate query
    // In PostgreSQL 11+:
    // - prokind = 'f' for functions, 'p' for procedures, 'a' for aggregates, 'w' for window functions
    // In PostgreSQL < 11:
    // - proisagg = true for aggregates
    // - proiswindow = true for window functions
    //
    // Note: We use unnest with ordinality to preserve the order and multiplicity of argument types
    let functions_query = if major_version >= 11 {
        r#"
        SELECT
            p.proname::text as function_name,
            n.nspname::text as schema_name,
            pg_get_functiondef(p.oid)::text as function_definition,
            l.lanname::text as language,
            obj_description(p.oid)::text as function_comment,
            t.typname::text as return_type,
            p.pronargs::integer as arg_count,
            p.proargnames::text[] as arg_names,
            (
                SELECT array_agg(pt.typname::text ORDER BY ord)
                FROM unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[])) WITH ORDINALITY AS u(type_oid, ord)
                JOIN pg_type pt ON pt.oid = u.type_oid
            ) as arg_types,
            p.proargmodes::text[] as arg_modes
        FROM pg_proc p
        JOIN pg_namespace n ON p.pronamespace = n.oid
        JOIN pg_language l ON p.prolang = l.oid
        LEFT JOIN pg_type t ON p.prorettype = t.oid
        WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
        AND p.prokind = 'f'
        ORDER BY n.nspname, p.proname
        "#
    } else {
        // For PostgreSQL < 11, use proisagg and proiswindow to filter out non-functions
        r#"
        SELECT
            p.proname::text as function_name,
            n.nspname::text as schema_name,
            pg_get_functiondef(p.oid)::text as function_definition,
            l.lanname::text as language,
            obj_description(p.oid)::text as function_comment,
            t.typname::text as return_type,
            p.pronargs::integer as arg_count,
            p.proargnames::text[] as arg_names,
            (
                SELECT array_agg(pt.typname::text ORDER BY ord)
                FROM unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[])) WITH ORDINALITY AS u(type_oid, ord)
                JOIN pg_type pt ON pt.oid = u.type_oid
            ) as arg_types,
            p.proargmodes::text[] as arg_modes
        FROM pg_proc p
        JOIN pg_namespace n ON p.pronamespace = n.oid
        JOIN pg_language l ON p.prolang = l.oid
        LEFT JOIN pg_type t ON p.prorettype = t.oid
        WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
        AND NOT p.proisagg
        AND NOT p.proiswindow
        ORDER BY n.nspname, p.proname
        "#
    };

    let function_rows = sqlx::query(functions_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enumerate functions: {}", e);
            crate::error::DbSurveyorError::collection_failed(
                "Failed to enumerate database functions",
                e,
            )
        })?;

    let mut functions = Vec::new();

    for row in &function_rows {
        let function_name: String = row.get_field("function_name", Some("pg_proc"))?;
        let schema_name: Option<String> = row.get_field("schema_name", Some("pg_proc"))?;
        let definition: Option<String> = row.get_field("function_definition", Some("pg_proc"))?;
        let language: Option<String> = row.get_field("language", Some("pg_proc"))?;
        let comment: Option<String> = row.get_field("function_comment", Some("pg_proc"))?;
        let return_type_name: Option<String> = row.get_field("return_type", Some("pg_proc"))?;
        let arg_names: Option<Vec<String>> = row.get_field("arg_names", Some("pg_proc"))?;
        let arg_types: Option<Vec<String>> = row.get_field("arg_types", Some("pg_proc"))?;
        let arg_modes: Option<Vec<String>> = row.get_field("arg_modes", Some("pg_proc"))?;

        // Parse parameters
        let parameters = parse_parameters(
            arg_names.as_deref(),
            arg_types.as_deref(),
            arg_modes.as_deref(),
        );

        // Map return type
        let return_type = return_type_name
            .as_ref()
            .map(|rt| map_pg_type_to_unified(rt));

        functions.push(Procedure {
            name: function_name.clone(),
            schema: schema_name,
            definition,
            parameters,
            return_type,
            language,
            comment,
        });

        tracing::debug!("Collected function '{}'", function_name);
    }

    tracing::info!("Successfully collected {} functions", functions.len());
    Ok(functions)
}

/// Collects all stored procedures from the PostgreSQL database (PostgreSQL 11+).
///
/// This function queries `pg_proc` to enumerate all user-defined procedures,
/// excluding system schemas.
///
/// # Arguments
/// * `pool` - PostgreSQL connection pool
///
/// # Returns
/// A vector of `Procedure` structs containing procedure metadata.
pub async fn collect_procedures(pool: &PgPool) -> Result<Vec<Procedure>> {
    tracing::debug!("Starting procedure collection for PostgreSQL database");

    // Check PostgreSQL version - procedures were introduced in PostgreSQL 11
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get PostgreSQL version", e)
        })?;

    // Extract major version number
    let major_version = extract_pg_major_version(&version);
    if major_version < 11 {
        tracing::info!(
            "PostgreSQL version {} does not support procedures (requires 11+)",
            major_version
        );
        return Ok(Vec::new());
    }

    let procedures_query = r#"
        SELECT
            p.proname::text as procedure_name,
            n.nspname::text as schema_name,
            pg_get_functiondef(p.oid)::text as procedure_definition,
            l.lanname::text as language,
            obj_description(p.oid)::text as procedure_comment,
            p.pronargs::integer as arg_count,
            p.proargnames::text[] as arg_names,
            ARRAY(
                SELECT typname::text
                FROM pg_type
                WHERE oid = ANY(COALESCE(p.proallargtypes, p.proargtypes::oid[]))
            ) as arg_types,
            p.proargmodes::text[] as arg_modes
        FROM pg_proc p
        JOIN pg_namespace n ON p.pronamespace = n.oid
        JOIN pg_language l ON p.prolang = l.oid
        WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
        AND p.prokind = 'p'
        ORDER BY n.nspname, p.proname
    "#;

    let procedure_rows = sqlx::query(procedures_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enumerate procedures: {}", e);
            crate::error::DbSurveyorError::collection_failed(
                "Failed to enumerate database procedures",
                e,
            )
        })?;

    let mut procedures = Vec::new();

    for row in &procedure_rows {
        let procedure_name: String = row.get_field("procedure_name", Some("pg_proc"))?;
        let schema_name: Option<String> = row.get_field("schema_name", Some("pg_proc"))?;
        let definition: Option<String> = row.get_field("procedure_definition", Some("pg_proc"))?;
        let language: Option<String> = row.get_field("language", Some("pg_proc"))?;
        let comment: Option<String> = row.get_field("procedure_comment", Some("pg_proc"))?;
        let arg_names: Option<Vec<String>> = row.get_field("arg_names", Some("pg_proc"))?;
        let arg_types: Option<Vec<String>> = row.get_field("arg_types", Some("pg_proc"))?;
        let arg_modes: Option<Vec<String>> = row.get_field("arg_modes", Some("pg_proc"))?;

        // Parse parameters
        let parameters = parse_parameters(
            arg_names.as_deref(),
            arg_types.as_deref(),
            arg_modes.as_deref(),
        );

        procedures.push(Procedure {
            name: procedure_name.clone(),
            schema: schema_name,
            definition,
            parameters,
            return_type: None, // Procedures don't have return types
            language,
            comment,
        });

        tracing::debug!("Collected procedure '{}'", procedure_name);
    }

    tracing::info!("Successfully collected {} procedures", procedures.len());
    Ok(procedures)
}

/// Parses function/procedure parameters from PostgreSQL metadata.
///
/// # Arguments
/// * `arg_names` - Optional slice of argument names
/// * `arg_types` - Optional slice of argument type names
/// * `arg_modes` - Optional slice of argument modes (i=IN, o=OUT, b=INOUT, v=VARIADIC, t=TABLE)
///
/// # Returns
/// A vector of `Parameter` structs.
fn parse_parameters(
    arg_names: Option<&[String]>,
    arg_types: Option<&[String]>,
    arg_modes: Option<&[String]>,
) -> Vec<Parameter> {
    let types = match arg_types {
        Some(t) if !t.is_empty() => t,
        _ => return Vec::new(),
    };

    let mut parameters = Vec::new();
    let names = arg_names.unwrap_or(&[]);
    let modes = arg_modes.unwrap_or(&[]);

    for (i, type_name) in types.iter().enumerate() {
        let name = if i < names.len() && !names[i].is_empty() {
            names[i].clone()
        } else {
            format!("$${}", i + 1)
        };

        let direction = if i < modes.len() {
            parse_parameter_mode(&modes[i])
        } else {
            ParameterDirection::In
        };

        let data_type = map_pg_type_to_unified(type_name);

        parameters.push(Parameter {
            name,
            data_type,
            direction,
            default_value: None, // PostgreSQL doesn't expose defaults easily
        });
    }

    parameters
}

/// Parses PostgreSQL parameter mode character to `ParameterDirection`.
///
/// # Arguments
/// * `mode` - Single character mode (i=IN, o=OUT, b=INOUT)
///
/// # Returns
/// The corresponding `ParameterDirection`.
fn parse_parameter_mode(mode: &str) -> ParameterDirection {
    match mode {
        "i" => ParameterDirection::In,
        "o" => ParameterDirection::Out,
        "b" => ParameterDirection::InOut,
        "v" => ParameterDirection::In,  // VARIADIC treated as IN
        "t" => ParameterDirection::Out, // TABLE treated as OUT
        _ => ParameterDirection::In,
    }
}

/// Maps a PostgreSQL type name to a unified data type.
///
/// This is a simplified mapping for routine parameters and return types.
///
/// # Arguments
/// * `pg_type` - PostgreSQL type name from pg_type.typname
///
/// # Returns
/// The corresponding `UnifiedDataType`.
fn map_pg_type_to_unified(pg_type: &str) -> UnifiedDataType {
    match pg_type.to_lowercase().as_str() {
        // String types
        "text" | "varchar" | "bpchar" | "name" => UnifiedDataType::String { max_length: None },

        // Integer types
        "int2" | "smallint" => UnifiedDataType::Integer {
            bits: 16,
            signed: true,
        },
        "int4" | "integer" | "int" => UnifiedDataType::Integer {
            bits: 32,
            signed: true,
        },
        "int8" | "bigint" => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },

        // Float types
        "float4" | "real" => UnifiedDataType::Float {
            precision: Some(24),
        },
        "float8" | "double precision" => UnifiedDataType::Float {
            precision: Some(53),
        },
        "numeric" => UnifiedDataType::Float { precision: None },

        // Boolean
        "bool" | "boolean" => UnifiedDataType::Boolean,

        // Date/Time
        "timestamp" | "timestamptz" => UnifiedDataType::DateTime {
            with_timezone: pg_type.contains("tz"),
        },
        "date" => UnifiedDataType::Date,
        "time" | "timetz" => UnifiedDataType::Time {
            with_timezone: pg_type.contains("tz"),
        },

        // JSON
        "json" | "jsonb" => UnifiedDataType::Json,

        // UUID
        "uuid" => UnifiedDataType::Uuid,

        // Binary
        "bytea" => UnifiedDataType::Binary { max_length: None },

        // Void (for procedures)
        "void" => UnifiedDataType::Custom {
            type_name: "void".to_string(),
        },

        // Trigger return type
        "trigger" => UnifiedDataType::Custom {
            type_name: "trigger".to_string(),
        },

        // Record types
        "record" => UnifiedDataType::Custom {
            type_name: "record".to_string(),
        },

        // Array types (starts with underscore in pg_type)
        t if t.starts_with('_') => {
            let element_type = map_pg_type_to_unified(&t[1..]);
            UnifiedDataType::Array {
                element_type: Box::new(element_type),
            }
        }

        // Custom/unknown types
        _ => UnifiedDataType::Custom {
            type_name: pg_type.to_string(),
        },
    }
}

/// Extracts the major version number from a PostgreSQL version string.
///
/// # Arguments
/// * `version_string` - Full version string from `SELECT version()`
///
/// # Returns
/// The major version number (e.g., 14, 15, 16).
fn extract_pg_major_version(version_string: &str) -> u32 {
    // PostgreSQL version strings look like:
    // "PostgreSQL 14.5 on x86_64-pc-linux-gnu, compiled by gcc..."
    // or "PostgreSQL 15.0 (Ubuntu 15.0-1.pgdg22.04+1) on x86_64..."

    let version_part = version_string
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()));

    if let Some(ver) = version_part {
        // Extract the major version (before the first dot)
        if let Some(major) = ver.split('.').next() {
            return major.parse().unwrap_or(0);
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pg_major_version() {
        assert_eq!(
            extract_pg_major_version("PostgreSQL 14.5 on x86_64-pc-linux-gnu"),
            14
        );
        assert_eq!(
            extract_pg_major_version("PostgreSQL 15.0 (Ubuntu 15.0-1.pgdg22.04+1)"),
            15
        );
        assert_eq!(extract_pg_major_version("PostgreSQL 11.1"), 11);
        assert_eq!(extract_pg_major_version("Unknown version"), 0);
    }

    #[test]
    fn test_parse_parameter_mode() {
        assert!(matches!(parse_parameter_mode("i"), ParameterDirection::In));
        assert!(matches!(parse_parameter_mode("o"), ParameterDirection::Out));
        assert!(matches!(
            parse_parameter_mode("b"),
            ParameterDirection::InOut
        ));
        assert!(matches!(parse_parameter_mode("v"), ParameterDirection::In));
        assert!(matches!(parse_parameter_mode("t"), ParameterDirection::Out));
        assert!(matches!(parse_parameter_mode("x"), ParameterDirection::In));
    }

    #[test]
    fn test_map_pg_type_to_unified() {
        assert!(matches!(
            map_pg_type_to_unified("int4"),
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
        assert!(matches!(
            map_pg_type_to_unified("text"),
            UnifiedDataType::String { max_length: None }
        ));
        assert!(matches!(
            map_pg_type_to_unified("bool"),
            UnifiedDataType::Boolean
        ));
        assert!(matches!(
            map_pg_type_to_unified("uuid"),
            UnifiedDataType::Uuid
        ));
        assert!(matches!(
            map_pg_type_to_unified("void"),
            UnifiedDataType::Custom { .. }
        ));
    }

    #[test]
    fn test_parse_parameters_empty() {
        let params = parse_parameters(None, None, None);
        assert!(params.is_empty());

        let params = parse_parameters(Some(&[]), Some(&[]), Some(&[]));
        assert!(params.is_empty());
    }

    #[test]
    fn test_parse_parameters_with_names() {
        let names = vec!["arg1".to_string(), "arg2".to_string()];
        let types = vec!["int4".to_string(), "text".to_string()];
        let modes = vec!["i".to_string(), "o".to_string()];

        let params = parse_parameters(Some(&names), Some(&types), Some(&modes));

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "arg1");
        assert!(matches!(params[0].direction, ParameterDirection::In));
        assert_eq!(params[1].name, "arg2");
        assert!(matches!(params[1].direction, ParameterDirection::Out));
    }

    #[test]
    fn test_parse_parameters_without_names() {
        let types = vec!["int4".to_string(), "text".to_string()];

        let params = parse_parameters(None, Some(&types), None);

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "$$1");
        assert_eq!(params[1].name, "$$2");
    }

    #[test]
    fn test_procedure_struct_creation() {
        let procedure = Procedure {
            name: "test_proc".to_string(),
            schema: Some("public".to_string()),
            definition: Some("BEGIN ... END".to_string()),
            parameters: vec![],
            return_type: None,
            language: Some("plpgsql".to_string()),
            comment: Some("Test procedure".to_string()),
        };

        assert_eq!(procedure.name, "test_proc");
        assert_eq!(procedure.schema, Some("public".to_string()));
        assert!(procedure.return_type.is_none());
    }
}
