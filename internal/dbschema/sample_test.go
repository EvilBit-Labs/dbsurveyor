package dbschema

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSampleStatusEmitsOnlyItsOwnPayload pins the wire shape of each variant.
// A reader distinguishes "sampling was cut short" from "sampling never ran" by
// the presence of these fields, so emitting a zero-valued original_limit on a
// skipped sample would be actively misleading.
func TestSampleStatusEmitsOnlyItsOwnPayload(t *testing.T) {
	tests := map[string]struct {
		status  SampleStatus
		present string
		absent  []string
	}{
		"complete":      {Complete(), "", []string{"original_limit", "reason"}},
		"partial_retry": {PartialRetry(500), "original_limit", []string{"reason"}},
		"skipped":       {Skipped("table too large"), "reason", []string{"original_limit"}},
	}

	for name, tc := range tests {
		t.Run(name, func(t *testing.T) {
			encoded, err := json.Marshal(tc.status)
			require.NoError(t, err)

			var fields map[string]any
			require.NoError(t, json.Unmarshal(encoded, &fields))

			assert.Contains(t, fields, "state")

			for _, field := range tc.absent {
				assert.NotContains(t, fields, field)
			}

			if tc.present != "" {
				assert.Contains(t, fields, tc.present)
			}

			var decoded SampleStatus
			require.NoError(t, json.Unmarshal(encoded, &decoded))
			assert.Equal(t, tc.status, decoded)
		})
	}
}

func TestSampleStatusRejectsUnknownState(t *testing.T) {
	var status SampleStatus

	err := json.Unmarshal([]byte(`{"state":"in_progress"}`), &status)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "in_progress",
		"the error should name the offending value so an operator can find it")
}

// TestNilStatusIsDistinctFromComplete guards the meaning of a nil status: an
// adapter that reports nothing is not the same as one reporting success.
func TestNilStatusIsDistinctFromComplete(t *testing.T) {
	sample := TableSample{
		TableName:        "orders",
		Rows:             []map[string]any{},
		SamplingStrategy: NoSampling(),
		CollectedAt:      testTime,
		Warnings:         []string{},
	}

	encoded, err := json.Marshal(sample)
	require.NoError(t, err)

	var fields map[string]any
	require.NoError(t, json.Unmarshal(encoded, &fields))
	assert.NotContains(t, fields, "status")

	var decoded TableSample
	require.NoError(t, json.Unmarshal(encoded, &decoded))
	assert.Nil(t, decoded.Status)
}

func TestSamplingStrategyOmitsLimitWhenNotSampling(t *testing.T) {
	encoded, err := json.Marshal(NoSampling())
	require.NoError(t, err)

	assert.JSONEq(t, `{"kind":"none"}`, string(encoded))

	encoded, err = json.Marshal(Random(250))
	require.NoError(t, err)

	assert.JSONEq(t, `{"kind":"random","limit":250}`, string(encoded))
}

func TestColumnNamesReadsTheFirstRow(t *testing.T) {
	sample := TableSample{Rows: []map[string]any{{"id": 1, "name": "x"}}}

	assert.ElementsMatch(t, []string{"id", "name"}, sample.ColumnNames())
}

func TestColumnNamesIsNilForAnEmptySample(t *testing.T) {
	sample := TableSample{Rows: []map[string]any{}}

	assert.Nil(t, sample.ColumnNames())
}

func TestUnifiedDataTypeRoundTripsIncludingNestedArrays(t *testing.T) {
	nested := ArrayType(ArrayType(IntegerType(32, false)))

	encoded, err := json.Marshal(nested)
	require.NoError(t, err)

	var decoded UnifiedDataType
	require.NoError(t, json.Unmarshal(encoded, &decoded))

	assert.Equal(t, nested, decoded)
	assert.Equal(t, "array<array<u32>>", decoded.String())
}

func TestUnifiedDataTypeOmitsPayloadOfOtherKinds(t *testing.T) {
	encoded, err := json.Marshal(BooleanType())
	require.NoError(t, err)

	assert.JSONEq(t, `{"kind":"boolean"}`, string(encoded))
}

func TestUnifiedDataTypeStringRendersDeclaredWidths(t *testing.T) {
	tests := map[string]struct {
		dataType UnifiedDataType
		want     string
	}{
		"sized string":     {StringType(ptr(uint32(255))), "string(255)"},
		"unsized string":   {StringType(nil), "string"},
		"signed integer":   {IntegerType(64, true), "i64"},
		"unsigned integer": {IntegerType(16, false), "u16"},
		"float precision":  {FloatType(ptr(uint8(53))), "float(53)"},
		"timestamptz":      {DateTimeType(true), "datetimetz"},
		"timestamp":        {DateTimeType(false), "datetime"},
		"custom":           {CustomType("hstore"), "hstore"},
		"date":             {DateType(), "date"},
	}

	for name, tc := range tests {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, tc.want, tc.dataType.String())
		})
	}
}
