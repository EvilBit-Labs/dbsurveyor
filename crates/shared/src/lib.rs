//! Shared data structures and types for the dbsurveyor toolchain
//!
//! This crate contains common data structures, types, and utilities
//! shared between the collector and postprocessor components.

use serde::{Deserialize, Serialize};

/// Version information for survey data format
pub const SURVEY_FORMAT_VERSION: &str = "1.0.0";

/// Common error types used across the toolchain
#[derive(thiserror::Error, Debug)]
pub enum SurveyError {
    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// File system or network I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error from other subsystems
    #[error("Generic error: {0}")]
    Generic(#[from] anyhow::Error),
}

/// Basic survey metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyMetadata {
    /// Format version for compatibility checking
    pub format_version: String,
    /// Timestamp when survey was created
    pub created_at: String,
    /// Tool version that created the survey
    pub tool_version: String,
    /// Database type that was surveyed
    pub database_type: String,
}

impl Default for SurveyMetadata {
    fn default() -> Self {
        Self {
            format_version: SURVEY_FORMAT_VERSION.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            database_type: "unknown".to_string(),
        }
    }
}

// TODO: Add more shared data structures as features are implemented
// - Database schema representations
// - Table/column metadata structures
// - Encryption/compression utilities
// - Classification types
