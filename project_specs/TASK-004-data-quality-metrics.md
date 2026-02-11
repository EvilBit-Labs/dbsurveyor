# Technical Specification: TASK-004 - Data Quality Metrics and Analysis

## Issue Summary

| Field | Value |
|-------|-------|
| **Issue** | [#4 - Data Quality Metrics and Analysis with Configurable Thresholds](https://github.com/EvilBit-Labs/dbsurveyor/issues/4) |
| **Task ID** | TASK-004 |
| **Priority** | High |
| **Milestone** | v0.1 |
| **Labels** | enhancement, analysis, quality |
| **Assignee** | unclesp1d3r |

## Problem Statement

DBSurveyor currently lacks comprehensive data quality assessment capabilities. Red team operators need to quickly identify data anomalies, assess completeness, and validate data consistency across database systems during security assessments. The existing sampling logic provides raw data but no analysis of data quality, integrity, or anomalies.

## Technical Approach

### Architecture Overview

The data quality module will integrate with the existing sampling pipeline, processing `TableSample` objects to compute quality metrics. The module follows DBSurveyor's established patterns:

```text
┌─────────────────────────────────────────────────────────────────┐
│                      Collection Pipeline                        │
├─────────────────────────────────────────────────────────────────┤
│  collect_schema() → sample_table() → analyze_quality()          │
│                                           │                     │
│                                           ▼                     │
│                              ┌─────────────────────┐            │
│                              │  QualityAnalyzer    │            │
│                              ├─────────────────────┤            │
│                              │ - completeness()    │            │
│                              │ - consistency()     │            │
│                              │ - uniqueness()      │            │
│                              │ - anomalies()       │            │
│                              └─────────────────────┘            │
│                                           │                     │
│                                           ▼                     │
│                              TableQualityMetrics                │
└─────────────────────────────────────────────────────────────────┘
```

### Design Principles

1. **KISS**: Simple, focused metrics with clear semantics
2. **TDD**: Test-first development for all quality functions
3. **Security-First**: No PII in outputs, offline-only operation
4. **Database-Agnostic**: Unified metrics across all database types
5. **Performance**: O(n) algorithms where n = sample size

## Implementation Plan

### Phase 1: Core Models and Configuration

**1.1 Quality Configuration** (`dbsurveyor-core/src/quality/config.rs`)

```rust
/// Quality assessment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    /// Enable quality analysis
    pub enabled: bool,
    /// Minimum completeness threshold (0.0-1.0)
    pub completeness_min: f64,
    /// Minimum uniqueness threshold (0.0-1.0)
    pub uniqueness_min: f64,
    /// Minimum consistency threshold (0.0-1.0)
    pub consistency_min: f64,
    /// Anomaly detection settings
    pub anomaly_detection: AnomalyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    pub enabled: bool,
    /// Sensitivity: low (3.0), medium (2.5), high (2.0) standard deviations
    pub sensitivity: AnomalySensitivity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AnomalySensitivity {
    Low,    // 3.0 std deviations
    Medium, // 2.5 std deviations
    High,   // 2.0 std deviations
}
```

**1.2 Quality Metrics Models** (`dbsurveyor-core/src/quality/models.rs`)

```rust
/// Quality metrics for a single table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableQualityMetrics {
    pub table_name: String,
    pub schema_name: Option<String>,
    pub analyzed_rows: u64,
    pub completeness: CompletenessMetrics,
    pub consistency: ConsistencyMetrics,
    pub uniqueness: UniquenessMetrics,
    pub anomalies: Option<AnomalyMetrics>,
    pub quality_score: f64,
    pub threshold_violations: Vec<ThresholdViolation>,
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletenessMetrics {
    /// Overall completeness score (0.0-1.0)
    pub score: f64,
    /// Per-column null/empty counts
    pub column_metrics: Vec<ColumnCompleteness>,
    /// Total null values across all columns
    pub total_nulls: u64,
    /// Total empty string values
    pub total_empty: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnCompleteness {
    pub column_name: String,
    pub null_count: u64,
    pub empty_count: u64,
    pub completeness: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyMetrics {
    pub score: f64,
    /// Columns with mixed type values (e.g., "123" vs 123)
    pub type_inconsistencies: Vec<TypeInconsistency>,
    /// Format pattern violations (dates, emails, etc.)
    pub format_violations: Vec<FormatViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniquenessMetrics {
    pub score: f64,
    /// Columns with duplicate values
    pub duplicate_columns: Vec<ColumnDuplicates>,
    /// Total duplicate rows (exact matches)
    pub duplicate_row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyMetrics {
    pub outlier_count: u64,
    pub outliers: Vec<ColumnAnomaly>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    pub metric: String,
    pub threshold: f64,
    pub actual: f64,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}
```

### Phase 2: Quality Analysis Engine

**2.1 Completeness Analyzer** (`dbsurveyor-core/src/quality/completeness.rs`)

Analyzes null and empty value distribution:
- Count null values per column
- Count empty strings per column
- Calculate completeness ratio: `(total - nulls - empty) / total`

**2.2 Consistency Analyzer** (`dbsurveyor-core/src/quality/consistency.rs`)

Validates data format consistency:
- Type consistency within columns (all values same JSON type)
- Format pattern matching (dates, UUIDs, emails)
- Length consistency for fixed-format fields

**2.3 Uniqueness Analyzer** (`dbsurveyor-core/src/quality/uniqueness.rs`)

Detects duplicates:
- Per-column uniqueness ratio
- Full row duplicate detection
- Primary key uniqueness validation

**2.4 Anomaly Detector** (`dbsurveyor-core/src/quality/anomaly.rs`)

Statistical outlier detection:
- Z-score calculation for numeric columns
- IQR method as fallback
- Configurable sensitivity thresholds

**2.5 Quality Analyzer Facade** (`dbsurveyor-core/src/quality/analyzer.rs`)

```rust
pub struct QualityAnalyzer {
    config: QualityConfig,
}

impl QualityAnalyzer {
    pub fn new(config: QualityConfig) -> Self;

    /// Analyze a table sample and return quality metrics
    pub fn analyze(&self, sample: &TableSample) -> Result<TableQualityMetrics>;

    /// Calculate overall quality score from individual metrics
    fn calculate_quality_score(
        &self,
        completeness: f64,
        consistency: f64,
        uniqueness: f64,
    ) -> f64;
}
```

### Phase 3: Integration

**3.1 Model Updates** (`dbsurveyor-core/src/models.rs`)

Add to `DatabaseSchema`:
```rust
pub quality_metrics: Option<Vec<TableQualityMetrics>>,
```

**3.2 Config Updates** (`dbsurveyor-core/src/adapters/config/collection.rs`)

Add to `CollectionConfig`:
```rust
pub quality_config: Option<QualityConfig>,
```

**3.3 CLI Updates** (`dbsurveyor-collect/src/main.rs`)

New flags:
- `--enable-quality`: Enable quality analysis (default: disabled)
- `--quality-threshold <metric:value>`: Override threshold (e.g., `completeness:0.9`)
- `--disable-anomaly-detection`: Disable statistical anomaly detection

### Phase 4: Output Integration

Quality metrics will be embedded in the JSON output under a `quality_metrics` key, following the existing pattern for optional data like `samples`.

## Test Plan

### Unit Tests

| Test | Location | Purpose |
|------|----------|---------|
| `test_completeness_all_present` | `quality/completeness.rs` | 100% completeness |
| `test_completeness_with_nulls` | `quality/completeness.rs` | Null detection |
| `test_completeness_with_empty` | `quality/completeness.rs` | Empty string detection |
| `test_consistency_uniform_types` | `quality/consistency.rs` | All values same type |
| `test_consistency_mixed_types` | `quality/consistency.rs` | Type inconsistency detection |
| `test_uniqueness_all_unique` | `quality/uniqueness.rs` | 100% uniqueness |
| `test_uniqueness_with_duplicates` | `quality/uniqueness.rs` | Duplicate detection |
| `test_anomaly_no_outliers` | `quality/anomaly.rs` | Normal distribution |
| `test_anomaly_with_outliers` | `quality/anomaly.rs` | Outlier detection |
| `test_quality_score_calculation` | `quality/analyzer.rs` | Score aggregation |
| `test_threshold_violations` | `quality/analyzer.rs` | Violation detection |
| `test_config_validation` | `quality/config.rs` | Invalid config rejection |
| `test_config_defaults` | `quality/config.rs` | Default values |

### Integration Tests

| Test | Location | Purpose |
|------|----------|---------|
| `test_postgres_quality_analysis` | `tests/postgres_quality.rs` | PostgreSQL integration |
| `test_mysql_quality_analysis` | `tests/mysql_quality.rs` | MySQL integration |
| `test_sqlite_quality_analysis` | `tests/sqlite_quality.rs` | SQLite integration |
| `test_mongodb_quality_analysis` | `tests/mongodb_quality.rs` | MongoDB integration |
| `test_quality_output_format` | `tests/quality_output.rs` | JSON output structure |

## Files to Modify/Create

### New Files

| File | Purpose | Lines (est.) |
|------|---------|--------------|
| `dbsurveyor-core/src/quality/mod.rs` | Module declaration | 30 |
| `dbsurveyor-core/src/quality/config.rs` | Configuration types | 150 |
| `dbsurveyor-core/src/quality/models.rs` | Metric data structures | 200 |
| `dbsurveyor-core/src/quality/completeness.rs` | Completeness analysis | 150 |
| `dbsurveyor-core/src/quality/consistency.rs` | Consistency analysis | 180 |
| `dbsurveyor-core/src/quality/uniqueness.rs` | Uniqueness analysis | 150 |
| `dbsurveyor-core/src/quality/anomaly.rs` | Anomaly detection | 200 |
| `dbsurveyor-core/src/quality/analyzer.rs` | Main analyzer facade | 150 |
| `dbsurveyor-core/tests/quality_*.rs` | Integration tests | 300 |

### Modified Files

| File | Change |
|------|--------|
| `dbsurveyor-core/src/lib.rs` | Add `pub mod quality;` |
| `dbsurveyor-core/src/models.rs` | Add `quality_metrics` field to `DatabaseSchema` |
| `dbsurveyor-core/src/adapters/config/collection.rs` | Add `quality_config` field |
| `dbsurveyor-core/src/adapters/config/mod.rs` | Re-export quality config |
| `dbsurveyor-collect/src/main.rs` | Add CLI flags and orchestration |
| `dbsurveyor-core/Cargo.toml` | Feature flag `data-quality` |

## Success Criteria

- [ ] All quality metrics (completeness, consistency, uniqueness) computed correctly
- [ ] Anomaly detection identifies statistical outliers
- [ ] Configurable thresholds work via CLI and config file
- [ ] Quality metrics integrated into JSON output format
- [ ] All database adapters produce quality metrics from samples
- [ ] `just ci-check` passes with zero warnings
- [ ] Test coverage meets 80% threshold
- [ ] Documentation updated for new features
- [ ] Performance: Analysis completes in < 100ms for 1000 row samples

## Out of Scope

- Real-time quality monitoring (this is batch analysis)
- Machine learning-based anomaly detection (statistical methods only)
- Data correction or repair functionality
- Cross-table referential integrity validation (constraint-based only)
- Historical quality trend analysis
- Custom quality rule DSL (future enhancement)
- SQL Server support (not yet implemented in codebase)

## Dependencies

No new external dependencies required. Uses:
- `serde` (existing) - Serialization
- `chrono` (existing) - Timestamps
- `serde_json` (existing) - JSON value analysis
- Standard library for statistics (mean, std deviation)

## Security Considerations

- Quality metrics must not expose actual data values (counts and ratios only)
- Anomaly details should not include the actual outlier values
- All outputs sanitized per existing security patterns
- No network calls - purely local analysis of sampled data

## Performance Considerations

- Analysis is O(n) where n = sample size (typically 100-1000 rows)
- Hash-based duplicate detection for O(1) average lookups
- Statistics computed in single pass where possible
- Memory usage bounded by sample size (already loaded for sampling)
