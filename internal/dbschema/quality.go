package dbschema

import (
	"errors"
	"fmt"
	"time"
)

// AnomalySensitivity selects how far from the mean a value must fall to count
// as an outlier.
type AnomalySensitivity string

// The anomaly detection sensitivities.
const (
	// SensitivityLow flags values beyond three standard deviations.
	SensitivityLow AnomalySensitivity = "low"
	// SensitivityMedium flags values beyond two and a half standard deviations.
	SensitivityMedium AnomalySensitivity = "medium"
	// SensitivityHigh flags values beyond two standard deviations.
	SensitivityHigh AnomalySensitivity = "high"
)

var anomalySensitivities = map[AnomalySensitivity]struct{}{
	SensitivityLow:    {},
	SensitivityMedium: {},
	SensitivityHigh:   {},
}

// Valid reports whether s is a recognized sensitivity.
func (s AnomalySensitivity) Valid() bool {
	_, ok := anomalySensitivities[s]

	return ok
}

// UnmarshalJSON rejects unrecognized sensitivities.
func (s *AnomalySensitivity) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "anomaly sensitivity", s)
}

// Z-score thresholds by sensitivity.
const (
	zScoreLow    = 3.0
	zScoreMedium = 2.5
	zScoreHigh   = 2.0
)

// ZScoreThreshold returns the number of standard deviations beyond which a value
// is an outlier. An unrecognized sensitivity uses the medium threshold.
func (s AnomalySensitivity) ZScoreThreshold() float64 {
	switch s {
	case SensitivityLow:
		return zScoreLow
	case SensitivityHigh:
		return zScoreHigh
	case SensitivityMedium:
		return zScoreMedium
	default:
		return zScoreMedium
	}
}

// AnomalyConfig controls statistical outlier detection.
type AnomalyConfig struct {
	Enabled     bool               `json:"enabled"`
	Sensitivity AnomalySensitivity `json:"sensitivity"`
}

// Default quality thresholds.
const (
	defaultCompletenessMin = 0.95
	// A uniqueness floor this high is deliberately strict. A low-cardinality
	// column such as status or category will violate it as a matter of course,
	// which is the signal an operator wants when the column was expected to be
	// an identifier and is not.
	defaultUniquenessMin  = 0.98
	defaultConsistencyMin = 0.90
)

// QualityConfig controls quality analysis: the thresholds a table is judged
// against and the relative weight of each score.
type QualityConfig struct {
	Enabled          bool          `json:"enabled"`
	CompletenessMin  float64       `json:"completeness_min"`
	UniquenessMin    float64       `json:"uniqueness_min"`
	ConsistencyMin   float64       `json:"consistency_min"`
	AnomalyDetection AnomalyConfig `json:"anomaly_detection"`
	// The weights of the three scores in the overall quality score. Setting one
	// to zero drops that dimension from the overall score without disabling the
	// per-dimension metrics or its threshold violation.
	CompletenessWeight float64 `json:"completeness_weight"`
	ConsistencyWeight  float64 `json:"consistency_weight"`
	UniquenessWeight   float64 `json:"uniqueness_weight"`
}

// DefaultQualityConfig returns the configuration used when an operator sets
// none.
func DefaultQualityConfig() QualityConfig {
	return QualityConfig{
		Enabled:         true,
		CompletenessMin: defaultCompletenessMin,
		UniquenessMin:   defaultUniquenessMin,
		ConsistencyMin:  defaultConsistencyMin,
		AnomalyDetection: AnomalyConfig{
			Enabled:     true,
			Sensitivity: SensitivityMedium,
		},
		CompletenessWeight: 1,
		ConsistencyWeight:  1,
		UniquenessWeight:   1,
	}
}

// Validate reports configuration that cannot produce a meaningful score.
//
// A configuration may arrive as a struct literal or from a decoded file, so the
// checks cannot assume any constructor ran.
func (c QualityConfig) Validate() error {
	var errs []error

	for _, threshold := range []struct {
		name  string
		value float64
	}{
		{"completeness_min", c.CompletenessMin},
		{"uniqueness_min", c.UniquenessMin},
		{"consistency_min", c.ConsistencyMin},
	} {
		if threshold.value < 0 || threshold.value > 1 {
			errs = append(errs, fmt.Errorf("%s must be in [0, 1], got %g", threshold.name, threshold.value))
		}
	}

	for _, weight := range []struct {
		name  string
		value float64
	}{
		{"completeness_weight", c.CompletenessWeight},
		{"consistency_weight", c.ConsistencyWeight},
		{"uniqueness_weight", c.UniquenessWeight},
	} {
		if weight.value < 0 {
			errs = append(errs, fmt.Errorf("%s must not be negative, got %g", weight.name, weight.value))
		}
	}

	if c.CompletenessWeight+c.ConsistencyWeight+c.UniquenessWeight <= 0 {
		errs = append(errs, errors.New("at least one quality score weight must be positive"))
	}

	if !c.AnomalyDetection.Sensitivity.Valid() {
		errs = append(errs, fmt.Errorf("unknown anomaly sensitivity %q", c.AnomalyDetection.Sensitivity))
	}

	return errors.Join(errs...)
}

// Analyzer scores sampled tables against a quality configuration.
type Analyzer struct {
	config QualityConfig
}

// NewAnalyzer returns an analyzer applying the given configuration.
func NewAnalyzer(config QualityConfig) *Analyzer {
	return &Analyzer{config: config}
}

// Config reports the configuration the analyzer applies.
func (a *Analyzer) Config() QualityConfig {
	return a.config
}

// Analyze scores one sampled table.
//
// analyzedAt is passed in rather than read from the clock so that a run's
// metrics all carry the same instant and a test can assert on the output
// exactly. Analysis cannot fail: a sample that carries no rows, or rows with no
// columns, produces defined scores rather than an error.
func (a *Analyzer) Analyze(sample *TableSample, analyzedAt time.Time) TableQualityMetrics {
	metrics := TableQualityMetrics{
		TableName:    sample.TableName,
		SchemaName:   sample.SchemaName,
		AnalyzedRows: uint64(len(sample.Rows)),
		Completeness: CompletenessMetrics{Score: 1, ColumnMetrics: []ColumnCompleteness{}},
		Consistency: ConsistencyMetrics{
			Score:               1,
			TypeInconsistencies: []TypeInconsistency{},
			FormatViolations:    []FormatViolation{},
		},
		Uniqueness:          UniquenessMetrics{Score: 1, DuplicateColumns: []ColumnDuplicates{}},
		QualityScore:        1,
		ThresholdViolations: []ThresholdViolation{},
		AnalyzedAt:          analyzedAt,
	}

	if !a.config.Enabled {
		return metrics
	}

	metrics.Completeness = analyzeCompleteness(sample)
	metrics.Consistency = analyzeConsistency(sample)
	metrics.Uniqueness = analyzeUniqueness(sample)

	if a.config.AnomalyDetection.Enabled {
		anomalies := analyzeAnomalies(sample, a.config.AnomalyDetection.Sensitivity)
		metrics.Anomalies = &anomalies
	}

	metrics.QualityScore = a.overallScore(metrics)
	metrics.ThresholdViolations = a.violations(metrics)

	return metrics
}

// AnalyzeAll scores every sampled table, in the order given.
func (a *Analyzer) AnalyzeAll(samples []TableSample, analyzedAt time.Time) []TableQualityMetrics {
	out := make([]TableQualityMetrics, 0, len(samples))
	for i := range samples {
		out = append(out, a.Analyze(&samples[i], analyzedAt))
	}

	return out
}

// overallScore is the weighted mean of the three dimension scores. A
// configuration whose weights sum to zero scores 0, which Validate rejects
// before it can be reached through a validated config.
func (a *Analyzer) overallScore(metrics TableQualityMetrics) float64 {
	completenessWeight := a.config.CompletenessWeight
	consistencyWeight := a.config.ConsistencyWeight
	uniquenessWeight := a.config.UniquenessWeight

	total := completenessWeight + consistencyWeight + uniquenessWeight
	if total <= 0 {
		return 0
	}

	weighted := metrics.Completeness.Score*completenessWeight +
		metrics.Consistency.Score*consistencyWeight +
		metrics.Uniqueness.Score*uniquenessWeight

	return clamp01(weighted / total)
}

// violations lists the dimensions whose score fell below its configured floor.
func (a *Analyzer) violations(metrics TableQualityMetrics) []ThresholdViolation {
	out := []ThresholdViolation{}

	if metrics.Completeness.Score < a.config.CompletenessMin {
		out = append(out, NewThresholdViolation("completeness", a.config.CompletenessMin, metrics.Completeness.Score))
	}

	if metrics.Consistency.Score < a.config.ConsistencyMin {
		out = append(out, NewThresholdViolation("consistency", a.config.ConsistencyMin, metrics.Consistency.Score))
	}

	if metrics.Uniqueness.Score < a.config.UniquenessMin {
		out = append(out, NewThresholdViolation("uniqueness", a.config.UniquenessMin, metrics.Uniqueness.Score))
	}

	return out
}
