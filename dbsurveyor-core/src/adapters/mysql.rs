//! MySQL database adapter (placeholder implementation).
//!
//! This module will be fully implemented in subsequent tasks.
//! Currently provides a placeholder that returns appropriate errors.

use crate::define_placeholder_adapter;
use crate::models::DatabaseType;

define_placeholder_adapter!(
    MySqlAdapter,
    "MySQL",
    DatabaseType::MySQL,
    [
        SchemaCollection,
        DataSampling,
        MultiDatabase,
        ConnectionPooling,
        QueryTimeout,
        ReadOnlyMode
    ]
);
