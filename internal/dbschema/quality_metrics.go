package dbschema

import "time"

// ViolationSeverity grades how far a measured score falls below its threshold.
type ViolationSeverity string

// The severities a threshold violation can carry.
const (
	// SeverityWarning marks a score below its threshold but within reach of it.
	SeverityWarning ViolationSeverity = "warning"
	// SeverityCritical marks a score far below its threshold.
	SeverityCritical ViolationSeverity = "critical"
)

var violationSeverities = map[ViolationSeverity]struct{}{
	SeverityWarning:  {},
	SeverityCritical: {},
}

// Valid reports whether v is a recognized severity.
func (v ViolationSeverity) Valid() bool {
	_, ok := violationSeverities[v]

	return ok
}

// UnmarshalJSON rejects unrecognized severities.
func (v *ViolationSeverity) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "violation severity", v)
}

// criticalSeverityRatio is the fraction of the threshold below which a
// violation is critical rather than a warning.
const criticalSeverityRatio = 0.8

// ThresholdViolation records a quality score that fell below its configured
// minimum.
type ThresholdViolation struct {
	// Metric names the score that fell short: "completeness", "consistency", or
	// "uniqueness".
	Metric    string            `json:"metric"`
	Threshold float64           `json:"threshold"`
	Actual    float64           `json:"actual"`
	Severity  ViolationSeverity `json:"severity"`
}

// NewThresholdViolation records a violation, grading it critical when the
// measured value is below criticalSeverityRatio of the threshold.
func NewThresholdViolation(metric string, threshold, actual float64) ThresholdViolation {
	severity := SeverityWarning
	if actual < threshold*criticalSeverityRatio {
		severity = SeverityCritical
	}

	return ThresholdViolation{
		Metric:    metric,
		Threshold: threshold,
		Actual:    actual,
		Severity:  severity,
	}
}

// ColumnCompleteness counts the missing values in one sampled column.
type ColumnCompleteness struct {
	ColumnName string `json:"column_name"`
	// NullCount counts values that are JSON null or absent from the row.
	NullCount uint64 `json:"null_count"`
	// EmptyCount counts values that are the empty string. They are recorded
	// apart from nulls because an empty string is a value the engine stored,
	// not a value it lacks.
	EmptyCount uint64 `json:"empty_count"`
	// Completeness is the fraction of sampled rows carrying a value, in [0, 1].
	Completeness float64 `json:"completeness"`
}

// NewColumnCompleteness computes the completeness ratio for one column.
//
// A column with no sampled rows scores 1.0: there is no evidence of missing
// data, which is different from evidence of complete data, and reporting 0.0
// would flag every empty table as a quality problem.
func NewColumnCompleteness(columnName string, nullCount, emptyCount, total uint64) ColumnCompleteness {
	completeness := 1.0

	if total > 0 {
		missing := nullCount + emptyCount
		if missing > total {
			missing = total
		}

		completeness = float64(total-missing) / float64(total)
	}

	return ColumnCompleteness{
		ColumnName:   columnName,
		NullCount:    nullCount,
		EmptyCount:   emptyCount,
		Completeness: clamp01(completeness),
	}
}

// CompletenessMetrics summarizes missing values across a sampled table.
type CompletenessMetrics struct {
	// Score is the mean of the per-column completeness ratios.
	Score          float64              `json:"score"`
	ColumnMetrics  []ColumnCompleteness `json:"column_metrics"`
	TotalNulls     uint64               `json:"total_nulls"`
	TotalEmpty     uint64               `json:"total_empty"`
	AnalyzedRows   uint64               `json:"analyzed_rows"`
	AnalyzedFields uint64               `json:"analyzed_fields"`
}

// TypeInconsistency records a column whose sampled values are not all of the
// same JSON type.
type TypeInconsistency struct {
	ColumnName string `json:"column_name"`
	// ExpectedType is the most common non-null type in the column.
	ExpectedType string `json:"expected_type"`
	// FoundTypes are the other types present, sorted.
	FoundTypes        []string `json:"found_types"`
	InconsistentCount uint64   `json:"inconsistent_count"`
}

// FormatViolation records a column whose values mostly follow one recognized
// format while some do not.
type FormatViolation struct {
	ColumnName     string `json:"column_name"`
	ExpectedFormat string `json:"expected_format"`
	ViolationCount uint64 `json:"violation_count"`
}

// ConsistencyMetrics summarizes type and format uniformity across a sampled
// table.
type ConsistencyMetrics struct {
	Score               float64             `json:"score"`
	TypeInconsistencies []TypeInconsistency `json:"type_inconsistencies"`
	FormatViolations    []FormatViolation   `json:"format_violations"`
}

// ColumnDuplicates counts repeated values in one sampled column.
type ColumnDuplicates struct {
	ColumnName string `json:"column_name"`
	// DuplicateCount counts occurrences after the first of each value.
	DuplicateCount uint64 `json:"duplicate_count"`
	UniqueCount    uint64 `json:"unique_count"`
	// Uniqueness is the fraction of sampled rows holding a first occurrence.
	Uniqueness float64 `json:"uniqueness"`
}

// NewColumnDuplicates computes the uniqueness ratio for one column.
func NewColumnDuplicates(columnName string, duplicateCount, total uint64) ColumnDuplicates {
	if duplicateCount > total {
		duplicateCount = total
	}

	uniqueCount := total - duplicateCount

	uniqueness := 1.0
	if total > 0 {
		uniqueness = float64(uniqueCount) / float64(total)
	}

	return ColumnDuplicates{
		ColumnName:     columnName,
		DuplicateCount: duplicateCount,
		UniqueCount:    uniqueCount,
		Uniqueness:     clamp01(uniqueness),
	}
}

// UniquenessMetrics summarizes duplication across a sampled table.
type UniquenessMetrics struct {
	Score float64 `json:"score"`
	// DuplicateColumns holds only the columns that carry duplicates.
	DuplicateColumns []ColumnDuplicates `json:"duplicate_columns"`
	// DuplicateRowCount counts rows identical to an earlier row across every
	// column.
	DuplicateRowCount uint64 `json:"duplicate_row_count"`
}

// ColumnAnomaly records statistical outliers in one numeric column.
//
// Mean and StdDev are aggregates rather than sampled values, but on a column
// such as salary or transaction amount they still describe the distribution.
// An operator working with sensitive numeric data should weigh that before
// sharing a quality report.
type ColumnAnomaly struct {
	ColumnName      string  `json:"column_name"`
	OutlierCount    uint64  `json:"outlier_count"`
	ZScoreThreshold float64 `json:"z_score_threshold"`
	Mean            float64 `json:"mean"`
	StdDev          float64 `json:"std_dev"`
}

// AnomalyMetrics summarizes statistical outliers across a sampled table.
type AnomalyMetrics struct {
	OutlierCount uint64          `json:"outlier_count"`
	Outliers     []ColumnAnomaly `json:"outliers"`
}

// TableQualityMetrics is the complete quality assessment of one sampled table.
//
// It carries counts, ratios, and column names only. No sampled value reaches it,
// so a quality report can be shared where the sample itself cannot.
type TableQualityMetrics struct {
	TableName  string  `json:"table_name"`
	SchemaName *string `json:"schema_name,omitempty"`
	// AnalyzedRows is the number of sampled rows the scores were computed from,
	// which is what makes a score interpretable: 1.0 over three rows is a much
	// weaker statement than 1.0 over a thousand.
	AnalyzedRows uint64              `json:"analyzed_rows"`
	Completeness CompletenessMetrics `json:"completeness"`
	Consistency  ConsistencyMetrics  `json:"consistency"`
	Uniqueness   UniquenessMetrics   `json:"uniqueness"`
	// Anomalies is nil when anomaly detection was disabled, which is distinct
	// from it having run and found nothing.
	Anomalies           *AnomalyMetrics      `json:"anomalies,omitempty"`
	QualityScore        float64              `json:"quality_score"`
	ThresholdViolations []ThresholdViolation `json:"threshold_violations"`
	AnalyzedAt          time.Time            `json:"analyzed_at"`
}

// clamp01 confines a score to [0, 1].
func clamp01(value float64) float64 {
	switch {
	case value < 0:
		return 0
	case value > 1:
		return 1
	default:
		return value
	}
}
