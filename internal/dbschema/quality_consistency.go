package dbschema

import (
	"encoding/json"
	"slices"
	"strings"
)

// minTypesForInconsistency is the number of distinct non-null types a column
// must carry before it is inconsistent rather than merely typed.
const minTypesForInconsistency = 2

// dominantFormatShare is the fraction of recognizably formatted values one
// format must hold before the rest are reported as violations. Below it there is
// no dominant format, only a mixed column.
const dominantFormatShare = 0.5

// analyzeConsistency measures type and format uniformity within each column of a
// sample.
func analyzeConsistency(sample *TableSample) ConsistencyMetrics {
	columnNames := sample.ColumnNames()

	metrics := ConsistencyMetrics{
		Score:               1,
		TypeInconsistencies: []TypeInconsistency{},
		FormatViolations:    []FormatViolation{},
	}

	if len(columnNames) == 0 {
		return metrics
	}

	var inconsistentCells uint64

	for _, columnName := range columnNames {
		counts := columnProfile(sample.Rows, columnName)

		if inconsistency, ok := typeInconsistency(columnName, counts.types); ok {
			inconsistentCells += inconsistency.InconsistentCount
			metrics.TypeInconsistencies = append(metrics.TypeInconsistencies, inconsistency)
		}

		if violation, ok := formatViolation(columnName, counts.formats); ok {
			inconsistentCells += violation.ViolationCount
			metrics.FormatViolations = append(metrics.FormatViolations, violation)
		}
	}

	totalCells := uint64(len(sample.Rows)) * uint64(len(columnNames))
	if totalCells > 0 {
		metrics.Score = clamp01(1 - float64(inconsistentCells)/float64(totalCells))
	}

	return metrics
}

// columnCounts is the type and format distribution of one column.
type columnCounts struct {
	// types counts the JSON types present, excluding null.
	types map[string]uint64
	// formats counts the recognized string formats present.
	formats map[string]uint64
}

// columnProfile counts the JSON types and the recognized string formats present
// in one column. Nulls are excluded from the type counts: a null is the absence
// of a value, which completeness already reports, not a value of a competing
// type.
func columnProfile(rows []map[string]any, columnName string) columnCounts {
	counts := columnCounts{
		types:   make(map[string]uint64),
		formats: make(map[string]uint64),
	}

	for _, row := range rows {
		value, ok := row[columnName]
		if !ok {
			continue
		}

		kind := jsonTypeName(value)
		if kind != jsonTypeNull {
			counts.types[kind]++
		}

		if text, isString := value.(string); isString && text != "" {
			counts.formats[detectFormat(text)]++
		}
	}

	return counts
}

// typeInconsistency reports a column carrying more than one non-null type, with
// the most common one taken as expected.
func typeInconsistency(columnName string, typeCounts map[string]uint64) (TypeInconsistency, bool) {
	if len(typeCounts) < minTypesForInconsistency {
		return TypeInconsistency{}, false
	}

	expected := dominantKey(typeCounts)

	found := make([]string, 0, len(typeCounts)-1)

	var inconsistentCount uint64

	for kind, count := range typeCounts {
		if kind == expected {
			continue
		}

		found = append(found, kind)
		inconsistentCount += count
	}

	slices.Sort(found)

	return TypeInconsistency{
		ColumnName:        columnName,
		ExpectedType:      expected,
		FoundTypes:        found,
		InconsistentCount: inconsistentCount,
	}, true
}

// formatViolation reports values departing from a column's dominant format.
//
// Values matching no recognized format count as violations, which is the point:
// in a column of email addresses, the entry that is not one is what an operator
// wants surfaced.
func formatViolation(columnName string, formatCounts map[string]uint64) (FormatViolation, bool) {
	var total, recognized uint64

	for format, count := range formatCounts {
		total += count

		if format != formatUnknown {
			recognized += count
		}
	}

	if recognized == 0 || total == 0 {
		return FormatViolation{}, false
	}

	expected := dominantRecognizedFormat(formatCounts)
	if expected == formatUnknown {
		return FormatViolation{}, false
	}

	if float64(formatCounts[expected])/float64(total) <= dominantFormatShare {
		return FormatViolation{}, false
	}

	violationCount := total - formatCounts[expected]
	if violationCount == 0 {
		return FormatViolation{}, false
	}

	return FormatViolation{
		ColumnName:     columnName,
		ExpectedFormat: expected,
		ViolationCount: violationCount,
	}, true
}

// dominantKey returns the highest-count key, breaking ties by name so the result
// does not depend on map iteration order.
func dominantKey(counts map[string]uint64) string {
	best := ""
	bestCount := uint64(0)

	for key, count := range counts {
		if count > bestCount || (count == bestCount && key < best) {
			best, bestCount = key, count
		}
	}

	return best
}

// dominantRecognizedFormat returns the most common format other than
// formatUnknown.
func dominantRecognizedFormat(counts map[string]uint64) string {
	recognized := make(map[string]uint64, len(counts))
	for format, count := range counts {
		if format != formatUnknown {
			recognized[format] = count
		}
	}

	if len(recognized) == 0 {
		return formatUnknown
	}

	return dominantKey(recognized)
}

// The JSON type names reported in a TypeInconsistency.
const (
	jsonTypeNull    = "null"
	jsonTypeBoolean = "boolean"
	jsonTypeString  = "string"
	jsonTypeNumber  = "number"
	jsonTypeArray   = "array"
	jsonTypeObject  = "object"
)

// jsonTypeName names the JSON type a sampled value marshals to.
func jsonTypeName(value any) string {
	switch typed := value.(type) {
	case nil:
		return jsonTypeNull
	case bool:
		return jsonTypeBoolean
	case string:
		return jsonTypeString
	case json.Number:
		return jsonTypeNumber
	case []any:
		return jsonTypeArray
	case map[string]any:
		return jsonTypeObject
	default:
		if _, ok := numericValue(typed); ok {
			return jsonTypeNumber
		}

		return jsonTypeObject
	}
}

// The recognized string formats.
const (
	formatUnknown     = "unknown"
	formatEmail       = "email"
	formatUUID        = "uuid"
	formatISODate     = "iso_date"
	formatISODateTime = "iso_datetime"
)

// uuidSeparatorPositions are the byte offsets of the hyphens in the canonical
// 8-4-4-4-12 layout.
var uuidSeparatorPositions = []int{8, 13, 18, 23}

// Layout lengths of the recognized formats.
const (
	uuidLength        = 36
	isoDateLength     = 10
	isoDateTimeMinLen = 19
)

// detectFormat classifies a string by shape.
//
// The checks are deliberately loose: they answer "does this column look like it
// holds email addresses" for the purpose of flagging the one row that does not,
// not "is this a valid address" in the sense of RFC 5322. Tightening them would
// turn format detection into format validation, which is a different feature
// with a much larger surface.
func detectFormat(value string) string {
	switch {
	case looksLikeUUID(value):
		return formatUUID
	case looksLikeISODateTime(value):
		return formatISODateTime
	case looksLikeISODate(value):
		return formatISODate
	case looksLikeEmail(value):
		return formatEmail
	default:
		return formatUnknown
	}
}

func looksLikeUUID(value string) bool {
	if len(value) != uuidLength {
		return false
	}

	for i, r := range value {
		isSeparator := slices.Contains(uuidSeparatorPositions, i)
		if isSeparator != (r == '-') {
			return false
		}

		if !isSeparator && !isHexDigit(r) {
			return false
		}
	}

	return true
}

func isHexDigit(r rune) bool {
	return (r >= '0' && r <= '9') || (r >= 'a' && r <= 'f') || (r >= 'A' && r <= 'F')
}

func looksLikeISODate(value string) bool {
	return len(value) == isoDateLength && value[4] == '-' && value[7] == '-'
}

func looksLikeISODateTime(value string) bool {
	return len(value) >= isoDateTimeMinLen && strings.Contains(value, "T") && strings.Contains(value, ":")
}

func looksLikeEmail(value string) bool {
	at := strings.Index(value, "@")

	return at > 0 && strings.Contains(value[at+1:], ".")
}
