//! Configuration types for database adapters.
//!
//! This module contains all configuration structures used by database adapters:
//! - `ConnectionConfig`: Database connection settings
//! - `SamplingConfig`: Data sampling configuration
//! - `CollectionConfig`: Schema collection settings
//! - `OutputFormat`: Output format options
//!
//! # Security
//! These configuration structs intentionally do NOT store passwords or credentials.
//! Credentials must be handled separately through the security module.

mod collection;
mod connection;
mod sampling;

pub use collection::{CollectionConfig, OutputFormat};
pub use connection::ConnectionConfig;
pub use sampling::{SamplingConfig, SensitivePattern};
