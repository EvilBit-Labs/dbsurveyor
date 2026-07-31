package dbschema

import (
	"encoding/json"
	"fmt"
	"strconv"
)

// analyzeUniqueness measures duplication in a sample, per column and across
// whole rows.
func analyzeUniqueness(sample *TableSample) UniquenessMetrics {
	columnNames := sample.ColumnNames()

	metrics := UniquenessMetrics{Score: 1, DuplicateColumns: []ColumnDuplicates{}}
	if len(columnNames) == 0 {
		return metrics
	}

	totalRows := uint64(len(sample.Rows))

	for _, columnName := range columnNames {
		seen := make(map[string]struct{}, len(sample.Rows))

		var duplicateCount uint64

		for _, row := range sample.Rows {
			// A missing key compares equal to an explicit null, matching how
			// completeness treats the two.
			key := valueKey(row[columnName])
			if _, ok := seen[key]; ok {
				duplicateCount++

				continue
			}

			seen[key] = struct{}{}
		}

		if duplicateCount > 0 {
			metrics.DuplicateColumns = append(
				metrics.DuplicateColumns,
				NewColumnDuplicates(columnName, duplicateCount, totalRows),
			)
		}
	}

	metrics.DuplicateRowCount = countDuplicateRows(sample.Rows)
	metrics.Score = uniquenessScore(metrics, totalRows)

	return metrics
}

// uniquenessScore combines row-level and column-level uniqueness, taking the
// worse of the two.
//
// Only columns carrying duplicates contribute to the column average. Averaging
// over every column would let a wide table with one badly duplicated column
// score well, which is the opposite of what the metric is for.
func uniquenessScore(metrics UniquenessMetrics, totalRows uint64) float64 {
	rowUniqueness := 1.0
	if totalRows > 0 {
		rowUniqueness = float64(totalRows-metrics.DuplicateRowCount) / float64(totalRows)
	}

	columnUniqueness := 1.0

	if len(metrics.DuplicateColumns) > 0 {
		sum := 0.0
		for _, column := range metrics.DuplicateColumns {
			sum += column.Uniqueness
		}

		columnUniqueness = sum / float64(len(metrics.DuplicateColumns))
	}

	return clamp01(min(rowUniqueness, columnUniqueness))
}

// countDuplicateRows counts rows identical to an earlier row across every
// column.
func countDuplicateRows(rows []map[string]any) uint64 {
	seen := make(map[string]struct{}, len(rows))

	var duplicateCount uint64

	for _, row := range rows {
		key := rowKey(row)
		if _, ok := seen[key]; ok {
			duplicateCount++

			continue
		}

		seen[key] = struct{}{}
	}

	return duplicateCount
}

// rowKey renders a row as a comparable string. encoding/json sorts object keys,
// so two rows with the same fields compare equal regardless of insertion order.
func rowKey(row map[string]any) string {
	data, err := json.Marshal(row)
	if err != nil {
		// A row that will not marshal cannot reach an output file either, so
		// there is nothing to compare it against. Give it a key derived from its
		// Go rendering so it is not silently folded together with other
		// unmarshallable rows.
		return "unmarshallable:" + fmt.Sprintf("%#v", row)
	}

	return string(data)
}

// valueKey renders a sampled value as a comparable string, prefixed by its kind
// so that the number 1 and the string "1" do not collide.
func valueKey(value any) string {
	switch typed := value.(type) {
	case nil:
		return "null"
	case bool:
		return "bool:" + strconv.FormatBool(typed)
	case string:
		return "str:" + typed
	}

	if number, ok := numericValue(value); ok {
		return "num:" + strconv.FormatFloat(number, 'g', -1, 64)
	}

	data, err := json.Marshal(value)
	if err != nil {
		return "unmarshallable:" + fmt.Sprintf("%#v", value)
	}

	return "json:" + string(data)
}
