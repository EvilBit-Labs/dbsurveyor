# Implement ObjectFailure Model and Collection Metadata Extensions

## Overview

Extend the data model to support granular object-level failure tracking for partial schema collections. This enables targeted re-runs and provides machine-actionable failure metadata for automation workflows.

## Scope

**What's Included**:

- Define `ObjectFailure` struct in `file:dbsurveyor-core/src/models.rs` with fields:
  - `object_type: ObjectType` (Table, View, Index, Constraint, Procedure, Function, Trigger, CustomType)
  - `object_name: String`
  - `schema_name: Option<String>`
  - `stage: CollectionStage` (EnumerateSchemas, EnumerateTables, CollectColumns, CollectConstraints, CollectIndexes, CollectViews, CollectRoutines, CollectTriggers, Other)
  - `error_category: ErrorCategory` (Permission, Timeout, NotFound, InvalidData, Other)
  - `error_message: String`
  - `retry_attempts: u32`
  - `final_backoff_ms: Option<u64>`
- Define supporting enums: `ObjectType`, `CollectionStage`, `ErrorCategory`
- Extend `CollectionMetadata` with `object_failures: Vec<ObjectFailure>` field (with `#[serde(default)]` for backward compatibility)
- Add JSON serialization/deserialization support with serde
- Update JSON Schema specification in `file:dbsurveyor-core/schemas/dbsurveyor-schema-v1.0.json`
- Unit tests for serialization/deserialization of all enum variants

**What's Explicitly Out**:

- Actual population of object failures during collection (handled in `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/7`)
- Postprocessor display of object failures (future work)
- Retry logic implementation (handled in `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/2`)

## Data Model

```rust
pub struct ObjectFailure {
    pub object_type: ObjectType,
    pub object_name: String,
    pub schema_name: Option<String>,
    pub stage: CollectionStage,
    pub error_category: ErrorCategory,
    pub error_message: String,
    pub retry_attempts: u32,
    pub final_backoff_ms: Option<u64>,
}

pub enum ObjectType {
    Table, View, Index, Constraint,
    Procedure, Function, Trigger, CustomType,
}

pub enum CollectionStage {
    EnumerateSchemas, EnumerateTables, CollectColumns,
    CollectConstraints, CollectIndexes, CollectViews,
    CollectRoutines, CollectTriggers, Other,
}

pub enum ErrorCategory {
    Permission, Timeout, NotFound, InvalidData, Other,
}
```

## Acceptance Criteria

- [ ] `ObjectFailure` struct compiles and serializes to JSON correctly
- [ ] `CollectionMetadata.object_failures` field is optional and backward-compatible with existing v1.0 schemas
- [ ] All enum variants (`ObjectType`, `CollectionStage`, `ErrorCategory`) serialize correctly
- [ ] JSON Schema updated to include new structures with proper validation rules
- [ ] Unit tests verify serialization/deserialization round-trip for all enum variants
- [ ] Existing schema files can be deserialized without errors (backward compatibility verified)

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - Data Model section)
- **Epic Brief**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/64fc1d47-e1e3-40db-a5dc-8dc9c248814c` (v1.0 Must Have - machine-actionable failure metadata)
