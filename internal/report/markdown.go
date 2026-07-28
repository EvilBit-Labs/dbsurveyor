package report

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/nao1215/markdown"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// absent is what a table cell shows for a value the engine did not report.
//
// An em dash would read better and is not ASCII, which R18 forbids; a blank cell
// would be ambiguous with a value that is genuinely empty.
const absent = "-"

// Markdown renders a schema document as Markdown.
//
// The section order is the order an operator reads in: what this database is,
// what tables it has, then each table's detail, then the things that are absent
// from most databases and interesting when present.
//
// A section whose subject is absent is omitted entirely rather than rendered
// empty. A report with eleven empty headings is a report nobody scrolls through.
func Markdown(document *dbschema.Schema, options Options) (string, error) {
	var out strings.Builder

	builder := markdown.NewMarkdown(&out)

	renderOverview(builder, document)
	renderTableSummary(builder, document)
	renderTables(builder, document)
	renderViews(builder, document)
	renderRoutines(builder, document)
	renderTriggers(builder, document)
	renderUserTypes(builder, document)
	renderQuality(builder, document)
	renderSamples(builder, document, options)
	renderWarnings(builder, document)

	if err := builder.Build(); err != nil {
		return "", fmt.Errorf("render the report: %w", err)
	}

	return out.String(), nil
}

// renderOverview states what the database is and how it was surveyed.
func renderOverview(builder *markdown.Markdown, document *dbschema.Schema) {
	info := document.DatabaseInfo

	builder.H1f("Database: %s", info.Name).LF()

	rows := [][]string{
		{"Engine", info.Type.String()},
		{"Version", optional(info.Version)},
		{"Encoding", optional(info.Encoding)},
		{"Collation", optional(info.Collation)},
		{"Owner", optional(info.Owner)},
		{"Access level", string(info.AccessLevel)},
		{"Collected at", document.CollectionMetadata.CollectedAt.UTC().Format("2006-01-02 15:04:05 UTC")},
		{"Collector", document.CollectionMetadata.CollectorVersion},
		{"Objects", strconv.Itoa(document.ObjectCount())},
	}

	if info.SizeBytes != nil {
		rows = append(rows, []string{"Size", humanBytes(*info.SizeBytes)})
	}

	builder.Table(markdown.TableSet{Header: []string{"Property", "Value"}, Rows: rows}).LF()
}

// renderTableSummary is the index an operator scans before reading anything.
func renderTableSummary(builder *markdown.Markdown, document *dbschema.Schema) {
	if len(document.Tables) == 0 {
		return
	}

	builder.H2("Tables").LF()

	rows := make([][]string, 0, len(document.Tables))

	for i := range document.Tables {
		table := &document.Tables[i]

		rows = append(rows, []string{
			qualifiedName(table.Schema, table.Name),
			strconv.Itoa(len(table.Columns)),
			rowCount(table.RowCount),
			strconv.Itoa(len(table.Indexes)),
			strconv.Itoa(len(table.ForeignKeys)),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Table", "Columns", "Rows", "Indexes", "Foreign keys"},
		Rows:   rows,
	}).LF()
}

// renderTables writes one section per table.
func renderTables(builder *markdown.Markdown, document *dbschema.Schema) {
	for i := range document.Tables {
		table := &document.Tables[i]

		builder.H3(qualifiedName(table.Schema, table.Name)).LF()

		if table.Comment != nil && *table.Comment != "" {
			builder.PlainText(*table.Comment).LF()
		}

		renderColumns(builder, table)
		renderKeys(builder, table)
		renderIndexes(builder, table)
		renderConstraints(builder, table)
	}
}

// renderColumns writes a table's columns.
func renderColumns(builder *markdown.Markdown, table *dbschema.Table) {
	if len(table.Columns) == 0 {
		return
	}

	rows := make([][]string, 0, len(table.Columns))

	for _, column := range table.Columns {
		rows = append(rows, []string{
			strconv.FormatUint(uint64(column.OrdinalPosition), 10),
			column.Name,
			column.DataType.String(),
			yesNo(column.Nullable),
			markers(column),
			optional(column.Default),
			optional(column.Comment),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"#", "Column", "Type", "Nullable", "Key", "Default", "Comment"},
		Rows:   rows,
	}).LF()
}

// markers renders the flags a column carries, as text rather than as symbols so
// the report stays readable where a font does not cooperate.
func markers(column dbschema.Column) string {
	var flags []string

	if column.PrimaryKey {
		flags = append(flags, "PK")
	}

	if column.AutoGenerate {
		flags = append(flags, "generated")
	}

	if len(flags) == 0 {
		return absent
	}

	return strings.Join(flags, ", ")
}

// renderKeys writes a table's primary and foreign keys.
func renderKeys(builder *markdown.Markdown, table *dbschema.Table) {
	if table.PrimaryKey != nil && len(table.PrimaryKey.Columns) > 0 {
		builder.PlainTextf("**Primary key:** %s", strings.Join(table.PrimaryKey.Columns, ", ")).LF()
	}

	if len(table.ForeignKeys) == 0 {
		return
	}

	rows := make([][]string, 0, len(table.ForeignKeys))

	for _, key := range table.ForeignKeys {
		rows = append(rows, []string{
			optional(key.Name),
			strings.Join(key.Columns, ", "),
			qualifiedName(key.ReferencedSchema, key.ReferencedTable),
			strings.Join(key.ReferencedColumns, ", "),
			action(key.OnDelete),
			action(key.OnUpdate),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Foreign key", "Columns", "References", "Parent columns", "On delete", "On update"},
		Rows:   rows,
	}).LF()
}

// renderIndexes writes a table's indexes.
func renderIndexes(builder *markdown.Markdown, table *dbschema.Table) {
	if len(table.Indexes) == 0 {
		return
	}

	rows := make([][]string, 0, len(table.Indexes))

	for _, index := range table.Indexes {
		rows = append(rows, []string{
			index.Name,
			indexColumns(index.Columns),
			yesNo(index.Unique),
			yesNo(index.Primary),
			optional(index.IndexType),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Index", "Columns", "Unique", "Primary", "Type"},
		Rows:   rows,
	}).LF()
}

// indexColumns renders index columns with their direction, which is what decides
// whether an index can serve an ordered query.
func indexColumns(columns []dbschema.IndexColumn) string {
	rendered := make([]string, 0, len(columns))

	for _, column := range columns {
		if column.SortOrder != nil && *column.SortOrder == dbschema.Descending {
			rendered = append(rendered, column.Name+" DESC")

			continue
		}

		rendered = append(rendered, column.Name)
	}

	return strings.Join(rendered, ", ")
}

// renderConstraints writes the check and unique constraints a table declares.
//
// Primary and foreign keys are omitted here: they already have their own section
// above, and repeating them doubles the length of every table's entry.
func renderConstraints(builder *markdown.Markdown, table *dbschema.Table) {
	rows := make([][]string, 0, len(table.Constraints))

	for _, constraint := range table.Constraints {
		if constraint.ConstraintType == dbschema.ConstraintPrimaryKey ||
			constraint.ConstraintType == dbschema.ConstraintForeignKey {
			continue
		}

		rows = append(rows, []string{
			constraint.Name,
			string(constraint.ConstraintType),
			strings.Join(constraint.Columns, ", "),
			optional(constraint.CheckClause),
		})
	}

	if len(rows) == 0 {
		return
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Constraint", "Kind", "Columns", "Clause"},
		Rows:   rows,
	}).LF()
}

// renderViews writes the views the survey found.
func renderViews(builder *markdown.Markdown, document *dbschema.Schema) {
	if len(document.Views) == 0 {
		return
	}

	builder.H2("Views").LF()

	for _, view := range document.Views {
		builder.H3(qualifiedName(view.Schema, view.Name)).LF()

		if view.Comment != nil && *view.Comment != "" {
			builder.PlainText(*view.Comment).LF()
		}

		if len(view.Columns) > 0 {
			rows := make([][]string, 0, len(view.Columns))
			for _, column := range view.Columns {
				rows = append(rows, []string{column.Name, column.DataType.String(), yesNo(column.Nullable)})
			}

			builder.Table(markdown.TableSet{
				Header: []string{"Column", "Type", "Nullable"},
				Rows:   rows,
			}).LF()
		}

		if view.Definition != nil && *view.Definition != "" {
			builder.CodeBlocks(markdown.SyntaxHighlightSQL, strings.TrimSpace(*view.Definition)).LF()
		}
	}
}

// renderRoutines writes procedures and functions in one table each.
func renderRoutines(builder *markdown.Markdown, document *dbschema.Schema) {
	for _, section := range []struct {
		title    string
		routines []dbschema.Routine
	}{
		{"Procedures", document.Procedures},
		{"Functions", document.Functions},
	} {
		if len(section.routines) == 0 {
			continue
		}

		builder.H2(section.title).LF()

		rows := make([][]string, 0, len(section.routines))

		for _, routine := range section.routines {
			returns := absent
			if routine.ReturnType != nil {
				returns = routine.ReturnType.String()
			}

			rows = append(rows, []string{
				qualifiedName(routine.Schema, routine.Name),
				optional(routine.Language),
				strconv.Itoa(len(routine.Parameters)),
				returns,
			})
		}

		builder.Table(markdown.TableSet{
			Header: []string{"Name", "Language", "Parameters", "Returns"},
			Rows:   rows,
		}).LF()
	}
}

// renderTriggers writes the triggers the survey found.
func renderTriggers(builder *markdown.Markdown, document *dbschema.Schema) {
	if len(document.Triggers) == 0 {
		return
	}

	builder.H2("Triggers").LF()

	rows := make([][]string, 0, len(document.Triggers))

	for _, trigger := range document.Triggers {
		rows = append(rows, []string{
			trigger.Name,
			qualifiedName(trigger.Schema, trigger.TableName),
			string(trigger.Timing),
			string(trigger.Event),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Trigger", "Table", "Timing", "Event"},
		Rows:   rows,
	}).LF()
}

// renderUserTypes writes the user-defined types the survey found.
func renderUserTypes(builder *markdown.Markdown, document *dbschema.Schema) {
	if len(document.UserTypes) == 0 {
		return
	}

	builder.H2("User-defined types").LF()

	rows := make([][]string, 0, len(document.UserTypes))

	for _, userType := range document.UserTypes {
		rows = append(rows, []string{
			qualifiedName(userType.Schema, userType.Name),
			string(userType.Category),
			userType.Definition,
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Type", "Category", "Definition"},
		Rows:   rows,
	}).LF()
}

// renderQuality writes the quality scores, when analysis ran.
//
// A nil metrics list means analysis did not run, which is different from it
// running and finding nothing, so the section is absent rather than empty.
func renderQuality(builder *markdown.Markdown, document *dbschema.Schema) {
	if len(document.QualityMetrics) == 0 {
		return
	}

	builder.H2("Data quality").LF()
	builder.PlainText(
		"Scores run from 0 to 1 and are computed over the sampled rows only. " +
			"A score of 1.0 over three rows is a much weaker statement than the same score over a thousand.",
	).LF()

	rows := make([][]string, 0, len(document.QualityMetrics))

	for _, metrics := range document.QualityMetrics {
		rows = append(rows, []string{
			qualifiedName(metrics.SchemaName, metrics.TableName),
			strconv.FormatUint(metrics.AnalyzedRows, 10),
			score(metrics.QualityScore),
			score(metrics.Completeness.Score),
			score(metrics.Consistency.Score),
			score(metrics.Uniqueness.Score),
			strconv.Itoa(len(metrics.ThresholdViolations)),
		})
	}

	builder.Table(markdown.TableSet{
		Header: []string{"Table", "Rows", "Overall", "Completeness", "Consistency", "Uniqueness", "Violations"},
		Rows:   rows,
	}).LF()

	renderViolations(builder, document)
}

// renderViolations lists the thresholds a table failed, which is the actionable
// half of the analysis.
func renderViolations(builder *markdown.Markdown, document *dbschema.Schema) {
	rows := [][]string{}

	for _, metrics := range document.QualityMetrics {
		for _, violation := range metrics.ThresholdViolations {
			rows = append(rows, []string{
				qualifiedName(metrics.SchemaName, metrics.TableName),
				violation.Metric,
				string(violation.Severity),
				score(violation.Threshold),
				score(violation.Actual),
			})
		}
	}

	if len(rows) == 0 {
		return
	}

	builder.H3("Threshold violations").LF()
	builder.Table(markdown.TableSet{
		Header: []string{"Table", "Metric", "Severity", "Threshold", "Actual"},
		Rows:   rows,
	}).LF()
}

// renderSamples writes the sampled rows, when the options asked for them.
//
// The column order is sorted rather than taken from a row's own key order, since
// a row is a map and Go gives no stable iteration order -- a report whose columns
// reordered between runs would be undiffable.
func renderSamples(builder *markdown.Markdown, document *dbschema.Schema, options Options) {
	if !options.IncludeSamples || len(document.Samples) == 0 {
		return
	}

	builder.H2("Samples").LF()

	for i := range document.Samples {
		sample := &document.Samples[i]

		builder.H3(qualifiedName(sample.SchemaName, sample.TableName)).LF()

		if sample.Status != nil && sample.Status.State != dbschema.SampleComplete {
			builder.PlainTextf("Sampling was %s: %s",
				sample.Status.State, optional(sample.Status.Reason)).LF()
		}

		columns := sample.ColumnNames()
		if len(columns) == 0 {
			continue
		}

		rows := make([][]string, 0, len(sample.Rows))

		for _, row := range sample.Rows {
			cells := make([]string, 0, len(columns))
			for _, column := range columns {
				cells = append(cells, cell(row[column]))
			}

			rows = append(rows, cells)
		}

		builder.Table(markdown.TableSet{Header: columns, Rows: rows}).LF()
	}
}

// renderWarnings writes the non-fatal problems the survey recorded.
func renderWarnings(builder *markdown.Markdown, document *dbschema.Schema) {
	warnings := document.CollectionMetadata.Warnings
	if len(warnings) == 0 {
		return
	}

	builder.H2("Collection warnings").LF()
	builder.BulletList(warnings...).LF()
}

// cell renders a sampled value for a table cell.
//
// A pipe would end the cell and a newline would end the row, so both are
// escaped. This is not cosmetic: a value containing either would otherwise
// produce a table that renders as something other than the data.
func cell(value any) string {
	if value == nil {
		return absent
	}

	rendered := fmt.Sprintf("%v", value)
	rendered = strings.ReplaceAll(rendered, "|", `\|`)
	rendered = strings.ReplaceAll(rendered, "\r\n", " ")
	rendered = strings.ReplaceAll(rendered, "\n", " ")

	return rendered
}

// qualifiedName renders a schema-qualified object name.
func qualifiedName(schema *string, name string) string {
	if schema == nil || *schema == "" {
		return name
	}

	return *schema + "." + name
}

// optional renders an optional string, or the absence marker.
func optional(value *string) string {
	if value == nil || strings.TrimSpace(*value) == "" {
		return absent
	}

	return strings.TrimSpace(*value)
}

// action renders an optional referential action.
func action(value *dbschema.ReferentialAction) string {
	if value == nil {
		return absent
	}

	return string(*value)
}

// rowCount renders an optional row estimate.
//
// A nil count means the engine maintains no such statistic, which is a different
// thing from an empty table and is shown as such.
func rowCount(count *uint64) string {
	if count == nil {
		return absent
	}

	return strconv.FormatUint(*count, 10)
}

// yesNo renders a boolean as a word rather than a symbol.
func yesNo(value bool) string {
	if value {
		return "yes"
	}

	return "no"
}

// score renders a quality score to two decimal places.
func score(value float64) string {
	return strconv.FormatFloat(value, 'f', 2, 64)
}

// The thresholds at which a size is reported in a larger unit.
const (
	kibibyte = 1024
	unitStep = 1024.0
)

// humanBytes renders a byte count in the largest unit that keeps it readable.
func humanBytes(size uint64) string {
	if size < kibibyte {
		return strconv.FormatUint(size, 10) + " B"
	}

	value := float64(size)
	units := []string{"KiB", "MiB", "GiB", "TiB", "PiB"}

	for _, unit := range units {
		value /= unitStep
		if value < unitStep {
			return strconv.FormatFloat(value, 'f', 1, 64) + " " + unit
		}
	}

	return strconv.FormatFloat(value, 'f', 1, 64) + " EiB"
}
