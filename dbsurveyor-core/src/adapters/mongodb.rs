//! MongoDB database adapter (placeholder implementation).
//!
//! This module will be fully implemented in subsequent tasks.
//! Currently provides a placeholder that returns appropriate errors.

use crate::define_placeholder_adapter;
use crate::models::DatabaseType;

define_placeholder_adapter!(
    MongoAdapter,
    "MongoDB",
    DatabaseType::MongoDB,
    [SchemaCollection, DataSampling, QueryTimeout]
);
