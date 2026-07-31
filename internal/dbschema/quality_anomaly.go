package dbschema

import (
	"encoding/json"
	"math"
	"strconv"
)

// minAnomalySamples is the fewest numeric values a column needs before a mean
// and a standard deviation say anything. Below it the analyzer reports nothing
// rather than flagging noise.
const minAnomalySamples = 3

// negligibleStdDev is the spread below which a column is treated as constant.
// Dividing by a smaller value would make the z-score of any tiny deviation
// enormous and flag every row.
const negligibleStdDev = 1e-10

// analyzeAnomalies flags statistical outliers in the numeric columns of a
// sample, using the z-score threshold the sensitivity selects.
func analyzeAnomalies(sample *TableSample, sensitivity AnomalySensitivity) AnomalyMetrics {
	metrics := AnomalyMetrics{Outliers: []ColumnAnomaly{}}

	columnNames := sample.ColumnNames()
	if len(columnNames) == 0 {
		return metrics
	}

	threshold := sensitivity.ZScoreThreshold()

	for _, columnName := range columnNames {
		values := numericColumn(sample.Rows, columnName)
		if len(values) < minAnomalySamples {
			continue
		}

		spread := describe(values)
		if spread.stdDev < negligibleStdDev {
			continue
		}

		var outlierCount uint64

		for _, value := range values {
			if math.Abs(value-spread.mean)/spread.stdDev > threshold {
				outlierCount++
			}
		}

		if outlierCount == 0 {
			continue
		}

		metrics.OutlierCount += outlierCount
		metrics.Outliers = append(metrics.Outliers, ColumnAnomaly{
			ColumnName:      columnName,
			OutlierCount:    outlierCount,
			ZScoreThreshold: threshold,
			Mean:            spread.mean,
			StdDev:          spread.stdDev,
		})
	}

	return metrics
}

// numericColumn collects the finite numeric values of one column, skipping rows
// where the value is missing, null, or not numeric.
func numericColumn(rows []map[string]any, columnName string) []float64 {
	values := make([]float64, 0, len(rows))

	for _, row := range rows {
		if number, ok := numericValue(row[columnName]); ok {
			values = append(values, number)
		}
	}

	return values
}

// distribution is the center and spread of a numeric column.
type distribution struct {
	mean   float64
	stdDev float64
}

// describe returns the mean and the population standard deviation.
//
// The population form divides by n rather than n-1. On a sample this understates
// the spread slightly, which makes outlier detection marginally more eager -- an
// acceptable direction for a metric whose purpose is to draw attention.
func describe(values []float64) distribution {
	if len(values) == 0 {
		return distribution{}
	}

	count := float64(len(values))

	sum := 0.0
	for _, value := range values {
		sum += value
	}

	mean := sum / count

	variance := 0.0
	for _, value := range values {
		deviation := value - mean
		variance += deviation * deviation
	}

	return distribution{mean: mean, stdDev: math.Sqrt(variance / count)}
}

// numericValue extracts a finite number from a sampled value.
//
// Values arriving from a decoded document are float64 or json.Number; values
// built in Go before marshalling may be any integer or float type. Strings are
// parsed too, because several drivers report NUMERIC and DECIMAL columns as
// text to avoid losing precision. Non-finite results are rejected: a single NaN
// would make the mean and standard deviation of the whole column NaN.
func numericValue(value any) (float64, bool) {
	var number float64

	switch typed := value.(type) {
	case float64:
		number = typed
	case float32:
		number = float64(typed)
	case int:
		number = float64(typed)
	case int8:
		number = float64(typed)
	case int16:
		number = float64(typed)
	case int32:
		number = float64(typed)
	case int64:
		number = float64(typed)
	case uint:
		number = float64(typed)
	case uint8:
		number = float64(typed)
	case uint16:
		number = float64(typed)
	case uint32:
		number = float64(typed)
	case uint64:
		number = float64(typed)
	case json.Number:
		parsed, err := typed.Float64()
		if err != nil {
			return 0, false
		}

		number = parsed
	case string:
		parsed, err := strconv.ParseFloat(typed, 64)
		if err != nil {
			return 0, false
		}

		number = parsed
	default:
		return 0, false
	}

	if math.IsNaN(number) || math.IsInf(number, 0) {
		return 0, false
	}

	return number, true
}
