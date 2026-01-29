//! PostgreSQL trigger collection implementation.
//!
//! This module handles collection of database triggers from PostgreSQL,
//! including trigger definitions, timing, and events.
//!
//! # PostgreSQL Trigger Types
//! - `BEFORE` - Fires before the triggering event
//! - `AFTER` - Fires after the triggering event
//! - `INSTEAD OF` - Fires instead of the triggering event (for views)
//!
//! # Trigger Events
//! - `INSERT` - Row insertion
//! - `UPDATE` - Row update
//! - `DELETE` - Row deletion
//! - `TRUNCATE` - Table truncation (PostgreSQL 8.4+)

use crate::Result;
use crate::adapters::helpers::RowExt;
use crate::models::{Trigger, TriggerEvent, TriggerTiming};
use sqlx::PgPool;

/// Collects all triggers from the PostgreSQL database.
///
/// This function queries `pg_trigger` to enumerate all user-defined triggers,
/// excluding internal triggers (used for constraints).
///
/// # Arguments
/// * `pool` - PostgreSQL connection pool
///
/// # Returns
/// A vector of `Trigger` structs containing trigger metadata.
pub async fn collect_triggers(pool: &PgPool) -> Result<Vec<Trigger>> {
    tracing::debug!("Starting trigger collection for PostgreSQL database");

    let triggers_query = r#"
        SELECT
            t.tgname::text as trigger_name,
            c.relname::text as table_name,
            n.nspname::text as schema_name,
            t.tgtype::integer as trigger_type,
            pg_get_triggerdef(t.oid, true)::text as trigger_definition,
            obj_description(t.oid)::text as trigger_comment,
            p.proname::text as function_name
        FROM pg_trigger t
        JOIN pg_class c ON t.tgrelid = c.oid
        JOIN pg_namespace n ON c.relnamespace = n.oid
        LEFT JOIN pg_proc p ON t.tgfoid = p.oid
        WHERE NOT t.tgisinternal
        AND n.nspname NOT IN ('pg_catalog', 'information_schema')
        ORDER BY n.nspname, c.relname, t.tgname
    "#;

    let trigger_rows = sqlx::query(triggers_query)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enumerate triggers: {}", e);
            crate::error::DbSurveyorError::collection_failed(
                "Failed to enumerate database triggers",
                e,
            )
        })?;

    let mut triggers = Vec::new();

    for row in &trigger_rows {
        let trigger_name: String = row.get_field("trigger_name", Some("pg_trigger"))?;
        let table_name: String = row.get_field("table_name", Some("pg_trigger"))?;
        let schema_name: Option<String> = row.get_field("schema_name", Some("pg_trigger"))?;
        let trigger_type: i32 = row.get_field("trigger_type", Some("pg_trigger"))?;
        let definition: Option<String> = row.get_field("trigger_definition", Some("pg_trigger"))?;

        // Parse trigger type bitmask
        let (timing, event) = parse_trigger_type(trigger_type);

        triggers.push(Trigger {
            name: trigger_name.clone(),
            table_name,
            schema: schema_name,
            event,
            timing,
            definition,
        });

        tracing::debug!("Collected trigger '{}'", trigger_name);
    }

    tracing::info!("Successfully collected {} triggers", triggers.len());
    Ok(triggers)
}

/// Parses PostgreSQL trigger type bitmask to timing and event.
///
/// PostgreSQL stores trigger configuration as a bitmask (tgtype) with the following bits:
/// - Bit 0 (1): ROW trigger (vs STATEMENT)
/// - Bit 1 (2): BEFORE trigger
/// - Bit 2 (4): INSERT event
/// - Bit 3 (8): DELETE event
/// - Bit 4 (16): UPDATE event
/// - Bit 5 (32): TRUNCATE event
/// - Bit 6 (64): INSTEAD OF trigger
///
/// # Arguments
/// * `tgtype` - The trigger type bitmask from pg_trigger.tgtype
///
/// # Returns
/// A tuple of (TriggerTiming, TriggerEvent).
fn parse_trigger_type(tgtype: i32) -> (TriggerTiming, TriggerEvent) {
    // Determine timing
    let timing = if (tgtype & TRIGGER_TYPE_INSTEAD) != 0 {
        TriggerTiming::InsteadOf
    } else if (tgtype & TRIGGER_TYPE_BEFORE) != 0 {
        TriggerTiming::Before
    } else {
        TriggerTiming::After
    };

    // Determine event (prioritize in order: INSERT, UPDATE, DELETE)
    // Note: A trigger can fire on multiple events, but our model supports one
    // We pick the first one found for simplicity
    let event = if (tgtype & TRIGGER_TYPE_INSERT) != 0 {
        TriggerEvent::Insert
    } else if (tgtype & TRIGGER_TYPE_UPDATE) != 0 {
        TriggerEvent::Update
    } else if (tgtype & TRIGGER_TYPE_DELETE) != 0 {
        TriggerEvent::Delete
    } else {
        // Default to Insert if no event bits are set (shouldn't happen)
        TriggerEvent::Insert
    };

    (timing, event)
}

// PostgreSQL trigger type bitmask constants
const TRIGGER_TYPE_BEFORE: i32 = 2;
const TRIGGER_TYPE_INSERT: i32 = 4;
const TRIGGER_TYPE_DELETE: i32 = 8;
const TRIGGER_TYPE_UPDATE: i32 = 16;
const TRIGGER_TYPE_INSTEAD: i32 = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trigger_type_before_insert() {
        // BEFORE INSERT ROW trigger: 1 (ROW) + 2 (BEFORE) + 4 (INSERT) = 7
        let (timing, event) = parse_trigger_type(7);
        assert!(matches!(timing, TriggerTiming::Before));
        assert!(matches!(event, TriggerEvent::Insert));
    }

    #[test]
    fn test_parse_trigger_type_after_update() {
        // AFTER UPDATE ROW trigger: 1 (ROW) + 16 (UPDATE) = 17
        let (timing, event) = parse_trigger_type(17);
        assert!(matches!(timing, TriggerTiming::After));
        assert!(matches!(event, TriggerEvent::Update));
    }

    #[test]
    fn test_parse_trigger_type_before_delete() {
        // BEFORE DELETE ROW trigger: 1 (ROW) + 2 (BEFORE) + 8 (DELETE) = 11
        let (timing, event) = parse_trigger_type(11);
        assert!(matches!(timing, TriggerTiming::Before));
        assert!(matches!(event, TriggerEvent::Delete));
    }

    #[test]
    fn test_parse_trigger_type_instead_of_insert() {
        // INSTEAD OF INSERT ROW trigger: 1 (ROW) + 64 (INSTEAD) + 4 (INSERT) = 69
        let (timing, event) = parse_trigger_type(69);
        assert!(matches!(timing, TriggerTiming::InsteadOf));
        assert!(matches!(event, TriggerEvent::Insert));
    }

    #[test]
    fn test_parse_trigger_type_after_insert() {
        // AFTER INSERT ROW trigger: 1 (ROW) + 4 (INSERT) = 5
        let (timing, event) = parse_trigger_type(5);
        assert!(matches!(timing, TriggerTiming::After));
        assert!(matches!(event, TriggerEvent::Insert));
    }

    #[test]
    fn test_parse_trigger_type_multiple_events() {
        // BEFORE INSERT OR UPDATE ROW trigger: 1 + 2 + 4 + 16 = 23
        // Should return INSERT as it has higher priority
        let (timing, event) = parse_trigger_type(23);
        assert!(matches!(timing, TriggerTiming::Before));
        assert!(matches!(event, TriggerEvent::Insert));
    }

    #[test]
    fn test_trigger_struct_creation() {
        let trigger = Trigger {
            name: "test_trigger".to_string(),
            table_name: "users".to_string(),
            schema: Some("public".to_string()),
            event: TriggerEvent::Insert,
            timing: TriggerTiming::Before,
            definition: Some("CREATE TRIGGER ...".to_string()),
        };

        assert_eq!(trigger.name, "test_trigger");
        assert_eq!(trigger.table_name, "users");
        assert!(matches!(trigger.timing, TriggerTiming::Before));
        assert!(matches!(trigger.event, TriggerEvent::Insert));
    }

    #[test]
    fn test_trigger_type_constants() {
        // Verify the constants are correct
        assert_eq!(TRIGGER_TYPE_BEFORE, 2);
        assert_eq!(TRIGGER_TYPE_INSERT, 4);
        assert_eq!(TRIGGER_TYPE_DELETE, 8);
        assert_eq!(TRIGGER_TYPE_UPDATE, 16);
        assert_eq!(TRIGGER_TYPE_INSTEAD, 64);
    }
}
