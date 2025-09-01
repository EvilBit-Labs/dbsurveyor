//! Core data structures and utilities for DBSurveyor.
//!
//! This crate provides the foundational types, traits, and utilities shared
//! between the collector and postprocessor binaries. It implements the
//! security-first, offline-capable architecture that defines DBSurveyor.
//!
//! # Security Guarantees
//! - No credentials stored or logged in any data structures
//! - AES-GCM encryption with random nonces for sensitive data
//! - All database operations are read-only
//! - Zero external network dependencies beyond target databases
//!
//! # Architecture
//! The core library follows these patterns:
//! - Repository pattern for database access abstraction
//! - Factory pattern for database adapter instantiation
//! - Comprehensive error handling with credential sanitization

pub mod adapters;
pub mod error;
pub mod models;
pub mod security;

// Re-export commonly used types
pub use adapters::{
    AdapterFeature, CollectionConfig, ConnectionConfig, DatabaseAdapter, OutputFormat,
    SamplingConfig, SensitivePattern,
};
pub use error::{DbSurveyorError, Result};
pub use models::{
    AccessLevel, CollectionMode, CollectionStatus, Column, DatabaseInfo, DatabaseSchema,
    DatabaseServerSchema, DatabaseType, OrderingStrategy, SamplingStrategy, ServerInfo,
    SortDirection, Table, TableSample, UnifiedDataType,
};

#[cfg(feature = "encryption")]
pub use security::encryption;
