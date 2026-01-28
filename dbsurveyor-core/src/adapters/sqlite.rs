//! SQLite database adapter (placeholder implementation).
//!
//! This module will be fully implemented in subsequent tasks.
//! Currently provides a placeholder that returns appropriate errors.

use crate::define_placeholder_adapter;
use crate::models::DatabaseType;

define_placeholder_adapter!(
    SqliteAdapter,
    "SQLite",
    DatabaseType::SQLite,
    [SchemaCollection, DataSampling, QueryTimeout, ReadOnlyMode]
);
