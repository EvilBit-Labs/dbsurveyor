package dbschema

// analyzeCompleteness measures how much of a sample is actually populated.
//
// A missing key and an explicit JSON null are both counted as null: an engine
// that omits a column from a row and one that reports it as null are saying the
// same thing about the data.
func analyzeCompleteness(sample *TableSample) CompletenessMetrics {
	columnNames := sample.ColumnNames()

	metrics := CompletenessMetrics{
		Score:         1,
		ColumnMetrics: []ColumnCompleteness{},
		AnalyzedRows:  uint64(len(sample.Rows)),
	}

	if len(columnNames) == 0 {
		return metrics
	}

	totalRows := uint64(len(sample.Rows))
	metrics.AnalyzedFields = totalRows * uint64(len(columnNames))

	sum := 0.0

	for _, columnName := range columnNames {
		var nullCount, emptyCount uint64

		for _, row := range sample.Rows {
			value, ok := row[columnName]

			switch {
			case !ok || value == nil:
				nullCount++
			case value == "":
				emptyCount++
			}
		}

		metrics.TotalNulls += nullCount
		metrics.TotalEmpty += emptyCount

		column := NewColumnCompleteness(columnName, nullCount, emptyCount, totalRows)
		metrics.ColumnMetrics = append(metrics.ColumnMetrics, column)
		sum += column.Completeness
	}

	metrics.Score = clamp01(sum / float64(len(metrics.ColumnMetrics)))

	return metrics
}
