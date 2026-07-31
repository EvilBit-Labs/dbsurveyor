package dbschema

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func sampleOf(rows []map[string]any) *TableSample {
	return &TableSample{
		TableName:        "users",
		SchemaName:       ptr("public"),
		Rows:             rows,
		SampleSize:       uint32(len(rows)),
		SamplingStrategy: MostRecent(uint32(len(rows))),
		CollectedAt:      testTime,
		Warnings:         []string{},
	}
}

func TestCompletenessScoresAKnownNullRatio(t *testing.T) {
	rows := make([]map[string]any, 0, 10)
	for i := range 10 {
		row := map[string]any{"id": i, "email": "user@example.com"}
		if i < 2 {
			row["email"] = nil
		}

		rows = append(rows, row)
	}

	metrics := analyzeCompleteness(sampleOf(rows))

	assert.InDelta(t, 0.9, metrics.Score, 1e-9, "id is complete and email is 80% complete")
	assert.Equal(t, uint64(2), metrics.TotalNulls)
	assert.Equal(t, uint64(0), metrics.TotalEmpty)
	require.Len(t, metrics.ColumnMetrics, 2)

	byName := map[string]ColumnCompleteness{}
	for _, column := range metrics.ColumnMetrics {
		byName[column.ColumnName] = column
	}

	assert.InDelta(t, 0.8, byName["email"].Completeness, 1e-9)
	assert.InDelta(t, 1.0, byName["id"].Completeness, 1e-9)
}

// A row missing a key entirely is malformed -- Validate rejects it -- but the
// analyzers must still behave when one reaches them, since a schema can be
// analyzed before it is validated.
func TestCompletenessTreatsAnAbsentKeyAsNull(t *testing.T) {
	metrics := analyzeCompleteness(sampleOf([]map[string]any{
		{"id": 1, "email": "ada@example.com"},
		{"id": 2},
	}))

	assert.Equal(t, uint64(1), metrics.TotalNulls)
	assert.InDelta(t, 0.75, metrics.Score, 1e-9)
}

// An empty string is a value the engine stored, not a value it lacks, so it is
// counted apart from a null.
func TestCompletenessCountsEmptyStringsSeparately(t *testing.T) {
	metrics := analyzeCompleteness(sampleOf([]map[string]any{
		{"note": ""},
		{"note": nil},
		{"note": "present"},
		{"note": "present"},
	}))

	assert.Equal(t, uint64(1), metrics.TotalEmpty)
	assert.Equal(t, uint64(1), metrics.TotalNulls)
	assert.InDelta(t, 0.5, metrics.Score, 1e-9)
}

// An empty sample must produce a defined score rather than dividing by zero.
func TestCompletenessOfAnEmptySampleIsDefined(t *testing.T) {
	for name, rows := range map[string][]map[string]any{
		"no rows":             {},
		"nil rows":            nil,
		"one row, no columns": {{}},
	} {
		t.Run(name, func(t *testing.T) {
			metrics := analyzeCompleteness(sampleOf(rows))

			assert.InDelta(t, 1.0, metrics.Score, 1e-9)
			assert.Empty(t, metrics.ColumnMetrics)
			assert.NotNil(t, metrics.ColumnMetrics, "an empty list, never null, so the document round-trips")
		})
	}
}

func TestUniquenessFlagsAnAllIdenticalColumn(t *testing.T) {
	rows := make([]map[string]any, 0, 5)
	for i := range 5 {
		rows = append(rows, map[string]any{"id": i, "status": "active"})
	}

	metrics := analyzeUniqueness(sampleOf(rows))

	require.Len(t, metrics.DuplicateColumns, 1, "only columns carrying duplicates are reported")
	assert.Equal(t, "status", metrics.DuplicateColumns[0].ColumnName)
	assert.Equal(t, uint64(4), metrics.DuplicateColumns[0].DuplicateCount)
	assert.InDelta(t, 0.2, metrics.DuplicateColumns[0].Uniqueness, 1e-9)
	assert.Equal(t, uint64(0), metrics.DuplicateRowCount, "the rows differ by id")
	assert.InDelta(t, 0.2, metrics.Score, 1e-9)
}

func TestUniquenessCountsIdenticalRows(t *testing.T) {
	row := map[string]any{"a": 1, "b": "x"}
	metrics := analyzeUniqueness(sampleOf([]map[string]any{row, {"b": "x", "a": 1}, {"a": 2, "b": "y"}}))

	assert.Equal(t, uint64(1), metrics.DuplicateRowCount, "field order must not affect row identity")
}

// The number 1 and the string "1" are different values and must not be folded
// together when counting duplicates.
func TestUniquenessDistinguishesValueKinds(t *testing.T) {
	metrics := analyzeUniqueness(sampleOf([]map[string]any{
		{"v": 1},
		{"v": "1"},
		{"v": true},
		{"v": nil},
	}))

	assert.Empty(t, metrics.DuplicateColumns)
	assert.InDelta(t, 1.0, metrics.Score, 1e-9)
}

func TestAnomalyFlagsAValueBeyondTheThreshold(t *testing.T) {
	rows := []map[string]any{
		{"amount": 10.0},
		{"amount": 10.0},
		{"amount": 10.0},
		{"amount": 11.0},
		{"amount": 9.0},
		{"amount": 10.0},
		{"amount": 1000.0},
	}

	high := analyzeAnomalies(sampleOf(rows), SensitivityHigh)
	require.Len(t, high.Outliers, 1)
	assert.Equal(t, "amount", high.Outliers[0].ColumnName)
	assert.Equal(t, uint64(1), high.Outliers[0].OutlierCount)
	assert.InDelta(t, zScoreHigh, high.Outliers[0].ZScoreThreshold, 1e-9)
	assert.Equal(t, uint64(1), high.OutlierCount)

	// The same distribution is within tolerance at the least sensitive setting,
	// which is what the sensitivity dial is for.
	low := analyzeAnomalies(sampleOf(rows), SensitivityLow)
	assert.Empty(t, low.Outliers)
}

func TestAnomalySkipsColumnsThatCannotSupportStatistics(t *testing.T) {
	constant := analyzeAnomalies(sampleOf([]map[string]any{
		{"v": 10}, {"v": 10}, {"v": 10}, {"v": 10},
	}), SensitivityHigh)
	assert.Empty(t, constant.Outliers, "a column with no spread has no outliers")

	tooFew := analyzeAnomalies(sampleOf([]map[string]any{{"v": 1}, {"v": 1000}}), SensitivityHigh)
	assert.Empty(t, tooFew.Outliers, "two values cannot establish a distribution")

	nonNumeric := analyzeAnomalies(sampleOf([]map[string]any{
		{"v": "a"}, {"v": "b"}, {"v": "c"}, {"v": "d"},
	}), SensitivityHigh)
	assert.Empty(t, nonNumeric.Outliers)
}

// A single NaN would poison the mean and standard deviation of a whole column.
func TestAnomalyRejectsNonFiniteValues(t *testing.T) {
	_, ok := numericValue("NaN")
	assert.False(t, ok)

	_, ok = numericValue("+Inf")
	assert.False(t, ok)

	value, ok := numericValue(json.Number("42.5"))
	require.True(t, ok)
	assert.InDelta(t, 42.5, value, 1e-9)
}

func TestConsistencyFlagsAColumnOfMixedTypes(t *testing.T) {
	metrics := analyzeConsistency(sampleOf([]map[string]any{
		{"v": "a"}, {"v": "b"}, {"v": "c"}, {"v": 4}, {"v": nil},
	}))

	require.Len(t, metrics.TypeInconsistencies, 1)
	assert.Equal(t, "string", metrics.TypeInconsistencies[0].ExpectedType)
	assert.Equal(t, []string{"number"}, metrics.TypeInconsistencies[0].FoundTypes)
	assert.Equal(t, uint64(1), metrics.TypeInconsistencies[0].InconsistentCount)
	assert.InDelta(t, 0.8, metrics.Score, 1e-9, "one cell of five is inconsistent")
}

func TestConsistencyFlagsAValueDepartingFromTheColumnFormat(t *testing.T) {
	metrics := analyzeConsistency(sampleOf([]map[string]any{
		{"email": "ada@example.com"},
		{"email": "grace@example.com"},
		{"email": "alan@example.com"},
		{"email": "not-an-address"},
	}))

	require.Len(t, metrics.FormatViolations, 1)
	assert.Equal(t, formatEmail, metrics.FormatViolations[0].ExpectedFormat)
	assert.Equal(t, uint64(1), metrics.FormatViolations[0].ViolationCount)
}

func TestConsistencyOfAUniformColumnIsPerfect(t *testing.T) {
	metrics := analyzeConsistency(sampleOf([]map[string]any{{"v": "a"}, {"v": "b"}}))

	assert.InDelta(t, 1.0, metrics.Score, 1e-9)
	assert.Empty(t, metrics.TypeInconsistencies)
	assert.Empty(t, metrics.FormatViolations)
}

func TestAnalyzeProducesViolationsAgainstTheConfiguredFloors(t *testing.T) {
	rows := make([]map[string]any, 0, 5)
	for range 5 {
		rows = append(rows, map[string]any{"status": "active"})
	}

	metrics := NewAnalyzer(DefaultQualityConfig()).Analyze(sampleOf(rows), testTime)

	assert.Equal(t, "users", metrics.TableName)
	assert.Equal(t, uint64(5), metrics.AnalyzedRows)
	assert.Equal(t, testTime, metrics.AnalyzedAt)
	require.Len(t, metrics.ThresholdViolations, 1)
	assert.Equal(t, "uniqueness", metrics.ThresholdViolations[0].Metric)
	assert.Equal(t, SeverityCritical, metrics.ThresholdViolations[0].Severity)
	assert.NotNil(t, metrics.Anomalies, "anomaly detection ran and found nothing, which is not the same as not running")
}

func TestAnalyzeWithDetectionDisabledOmitsAnomalies(t *testing.T) {
	config := DefaultQualityConfig()
	config.AnomalyDetection.Enabled = false

	metrics := NewAnalyzer(config).Analyze(sampleOf([]map[string]any{{"v": 1}}), testTime)
	assert.Nil(t, metrics.Anomalies)
}

func TestAnalyzeDisabledReturnsNeutralMetrics(t *testing.T) {
	config := DefaultQualityConfig()
	config.Enabled = false

	metrics := NewAnalyzer(config).Analyze(sampleOf([]map[string]any{{"v": "same"}, {"v": "same"}}), testTime)

	assert.InDelta(t, 1.0, metrics.QualityScore, 1e-9)
	assert.Empty(t, metrics.ThresholdViolations)
	assert.Equal(t, uint64(2), metrics.AnalyzedRows, "the row count is still reported")
}

func TestAnalyzeWeightsDropADimensionWithoutHidingIt(t *testing.T) {
	rows := []map[string]any{{"status": "a"}, {"status": "a"}}

	config := DefaultQualityConfig()
	config.UniquenessWeight = 0

	metrics := NewAnalyzer(config).Analyze(sampleOf(rows), testTime)

	assert.InDelta(t, 1.0, metrics.QualityScore, 1e-9, "uniqueness no longer contributes")
	assert.InDelta(t, 0.5, metrics.Uniqueness.Score, 1e-9, "but it is still measured")
	require.Len(t, metrics.ThresholdViolations, 1, "and still reported as a violation")
}

func TestThresholdViolationSeverity(t *testing.T) {
	cases := []struct {
		name   string
		actual float64
		want   ViolationSeverity
	}{
		{"just below the threshold", 0.90, SeverityWarning},
		{"at the critical boundary", 0.95 * criticalSeverityRatio, SeverityWarning},
		{"below the critical boundary", 0.75, SeverityCritical},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			violation := NewThresholdViolation("completeness", 0.95, testCase.actual)
			assert.Equal(t, testCase.want, violation.Severity)
		})
	}
}

func TestQualityConfigValidate(t *testing.T) {
	require.NoError(t, DefaultQualityConfig().Validate())

	cases := map[string]func(*QualityConfig){
		"completeness_min must be in [0, 1]":      func(c *QualityConfig) { c.CompletenessMin = 1.5 },
		"uniqueness_min must be in [0, 1]":        func(c *QualityConfig) { c.UniquenessMin = -0.1 },
		"consistency_weight must not be negative": func(c *QualityConfig) { c.ConsistencyWeight = -1 },
		"at least one quality score weight must be positive": func(c *QualityConfig) {
			c.CompletenessWeight, c.ConsistencyWeight, c.UniquenessWeight = 0, 0, 0
		},
		"unknown anomaly sensitivity": func(c *QualityConfig) { c.AnomalyDetection.Sensitivity = "extreme" },
	}

	for want, mutate := range cases {
		t.Run(want, func(t *testing.T) {
			config := DefaultQualityConfig()
			mutate(&config)
			require.ErrorContains(t, config.Validate(), want)
		})
	}
}

// A validated config never reaches the zero-weight guard, but a struct literal
// can, and it must not divide by zero.
func TestOverallScoreWithNoWeightsIsZero(t *testing.T) {
	analyzer := NewAnalyzer(QualityConfig{Enabled: true})
	metrics := analyzer.Analyze(sampleOf([]map[string]any{{"v": 1}}), testTime)

	assert.InDelta(t, 0.0, metrics.QualityScore, 1e-9)
}

// Quality metrics are shareable where the sample is not, which only holds if no
// sampled value reaches them.
func TestQualityMetricsCarryNoSampledValues(t *testing.T) {
	const secret = "s3cret-payload-value"

	rows := []map[string]any{
		{"id": 1, "note": secret},
		{"id": 2, "note": secret},
		{"id": 3, "note": 7},
	}

	metrics := NewAnalyzer(DefaultQualityConfig()).Analyze(sampleOf(rows), testTime)

	encoded, err := json.Marshal(metrics)
	require.NoError(t, err)
	assert.NotContains(t, string(encoded), secret)
	assert.Contains(t, string(encoded), `"note"`, "column names are reported, values are not")
}

func TestAnalyzeAllPreservesOrder(t *testing.T) {
	samples := []TableSample{
		*sampleOf([]map[string]any{{"v": 1}}),
		*sampleOf([]map[string]any{{"v": 2}}),
	}
	samples[0].TableName = "first"
	samples[1].TableName = "second"

	metrics := NewAnalyzer(DefaultQualityConfig()).AnalyzeAll(samples, testTime)

	require.Len(t, metrics, 2)
	assert.Equal(t, "first", metrics[0].TableName)
	assert.Equal(t, "second", metrics[1].TableName)
}

func TestQualityMetricsRoundTrip(t *testing.T) {
	metrics := NewAnalyzer(DefaultQualityConfig()).Analyze(sampleOf([]map[string]any{
		{"id": 1, "status": "a"},
		{"id": 2, "status": "a"},
	}), testTime)

	encoded, err := json.Marshal(metrics)
	require.NoError(t, err)

	var decoded TableQualityMetrics
	require.NoError(t, json.Unmarshal(encoded, &decoded))
	assert.Equal(t, metrics, decoded)
}
