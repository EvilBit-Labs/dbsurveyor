//! Data quality assessment module.
//!
//! This module provides data quality analysis capabilities including:
//! - **Completeness**: Identify missing, null, or empty values
//! - **Consistency**: Detect data format inconsistencies
//! - **Uniqueness**: Find duplicate records
//! - **Anomaly Detection**: Statistical outlier identification
//!
//! # Security Guarantees
//! - Quality metrics expose counts and ratios only, never actual data values
//! - No PII in outputs - anomaly details exclude actual outlier values
//! - Offline-only operation with no network dependencies
//!
//! # Example
//! ```rust,ignore
//! use dbsurveyor_core::quality::{QualityAnalyzer, QualityConfig};
//!
//! let config = QualityConfig::default();
//! let analyzer = QualityAnalyzer::new(config);
//! let metrics = analyzer.analyze(&table_sample)?;
//! println!("Quality score: {:.2}%", metrics.quality_score * 100.0);
//! ```

mod analyzer;
mod anomaly;
mod completeness;
mod config;
mod consistency;
mod models;
mod uniqueness;

// Re-export public API
pub use analyzer::QualityAnalyzer;
pub use config::{AnomalyConfig, AnomalySensitivity, ConfigValidationError, QualityConfig};
pub use models::{
    AnomalyMetrics, ColumnAnomaly, ColumnCompleteness, ColumnDuplicates, CompletenessMetrics,
    ConsistencyMetrics, FormatViolation, TableQualityMetrics, ThresholdViolation,
    TypeInconsistency, UniquenessMetrics, ViolationSeverity,
};
