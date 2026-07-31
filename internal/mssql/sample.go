package mssql

import (
	"context"
	"database/sql"
	"fmt"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// defaultSchema is the namespace a table reference falls back to when it names
// none.
const defaultSchema = "dbo"

// samplingColumn is what the sampler needs to know about a column.
type samplingColumn struct {
	name          string
	autoIncrement bool
	primaryKey    bool
}

// SampleTable reads rows from one table.
//
// The rows are the only part of a survey that touches user data, so the query is
// as narrow as it can be: one SELECT, ordered by the strategy the table's own
// structure supports, bounded by TOP. No temporary object is created and no
// session state is left behind.
func (a *Adapter) SampleTable(
	ctx context.Context,
	table dbadapter.TableRef,
	cfg dbadapter.SamplingConfig,
) (dbschema.TableSample, error) {
	if a.db == nil {
		return dbschema.TableSample{}, ErrClosed
	}

	if err := table.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mssql: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mssql: invalid sampling configuration: %w", err)
	}

	target := table
	if target.Schema == "" {
		target.Schema = defaultSchema
	}

	columns, err := a.describeForSampling(ctx, target)
	if err != nil {
		return dbschema.TableSample{}, err
	}

	ordering := orderingFor(columns, cfg)

	rows, err := a.selectRows(ctx, target, ordering, cfg)
	if err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mssql: sample %s: %w", target, err)
	}

	status := dbschema.Complete()
	sample := dbschema.TableSample{
		TableName:        target.Table,
		SchemaName:       &target.Schema,
		Rows:             rows,
		SampleSize:       sampledRows(rows),
		SamplingStrategy: dbschema.MostRecent(cfg.SampleSize),
		Ordering:         &ordering,
		CollectedAt:      time.Now(),
		Warnings:         []string{},
		Status:           &status,
	}

	if ordering.Kind == dbschema.OrderUnordered {
		// An unordered sample is still a sample, but "most recent" did not mean
		// anything for this table and a reader should not assume it did.
		sample.SamplingStrategy = dbschema.SamplingStrategy{
			Kind:  dbschema.SampleNone,
			Limit: &cfg.SampleSize,
		}
		sample.Warnings = append(sample.Warnings,
			"no usable ordering column; rows are in whatever order the engine returned them")
	}

	appendSensitiveWarnings(&sample, cfg)

	return sample, nil
}

// describeForSampling reads the columns of one table.
//
// An empty result means the table is not in the catalog, or the credential
// cannot see it. Either way it is an error: a sampler that read zero columns as
// success would go on to select from a table that is not there.
func (a *Adapter) describeForSampling(
	ctx context.Context,
	table dbadapter.TableRef,
) ([]samplingColumn, error) {
	var columns []samplingColumn

	args := []any{table.Schema, table.Table}

	err := a.eachRow(ctx, samplingColumnsQuery, args, func(rows *sql.Rows) error {
		var column samplingColumn
		if err := rows.Scan(&column.name, &column.autoIncrement, &column.primaryKey); err != nil {
			return err
		}

		columns = append(columns, column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mssql: describe %s for sampling: %w", table, err)
	}

	if len(columns) == 0 {
		return nil, fmt.Errorf("%w: %s", ErrUnknownTable, table)
	}

	return columns, nil
}

// orderingFor picks how to define "most recent" for a table.
//
// The preference order runs from the most meaningful signal to the least: an
// operator-named timestamp column says what they consider recent, an identity
// column approximates insertion order, and a primary key at least gives a stable
// order. A table with none of those is sampled unordered rather than not
// sampled.
func orderingFor(columns []samplingColumn, cfg dbadapter.SamplingConfig) dbschema.OrderingStrategy {
	if column, ok := timestampColumn(columns, cfg.TimestampColumns); ok {
		descending := dbschema.Descending

		return dbschema.OrderingStrategy{
			Kind:      dbschema.OrderTimestamp,
			Column:    &column,
			Direction: &descending,
		}
	}

	for _, column := range columns {
		if column.autoIncrement {
			name := column.name

			return dbschema.OrderingStrategy{Kind: dbschema.OrderAutoIncrement, Column: &name}
		}
	}

	var key []string

	for _, column := range columns {
		if column.primaryKey {
			key = append(key, column.name)
		}
	}

	if len(key) > 0 {
		return dbschema.OrderingStrategy{Kind: dbschema.OrderPrimaryKey, Columns: key}
	}

	return dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}
}

// timestampColumn finds the first configured timestamp column the table actually
// has, matched without regard to case because engines differ on how they fold
// identifiers.
func timestampColumn(columns []samplingColumn, preferred []string) (string, bool) {
	for _, want := range preferred {
		for _, column := range columns {
			if strings.EqualFold(column.name, want) {
				return column.name, true
			}
		}
	}

	return "", false
}

// selectRows runs the sampling query and materializes the rows.
//
// The row limit is rendered into TOP rather than bound, because SQL Server
// accepts a parameter there only in the parenthesized form and the value is a
// uint32 this package produced from its own validated configuration -- not a
// string from anywhere a caller controls. Rendering it through strconv rather
// than through fmt keeps that obvious.
func (a *Adapter) selectRows(
	ctx context.Context,
	table dbadapter.TableRef,
	ordering dbschema.OrderingStrategy,
	cfg dbadapter.SamplingConfig,
) ([]map[string]any, error) {
	timeout := cfg.QueryTimeout
	if timeout <= 0 {
		timeout = a.queryTimeout
	}

	queryCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	// The table name is interpolated because SQL cannot bind an identifier. It
	// travels through the per-engine quoting helper.
	//nolint:gosec // G202: the interpolated values are an escaped identifier and a validated row limit.
	statement := `SELECT TOP (` + strconv.FormatUint(uint64(cfg.SampleSize), 10) + `) * FROM ` +
		qualify(table.Schema, table.Table) + orderClause(ordering)

	rows, err := a.db.QueryContext(queryCtx, statement)
	if err != nil {
		return nil, err
	}

	defer func() { discardError(rows.Close()) }()

	names, err := rows.Columns()
	if err != nil {
		return nil, err
	}

	sampled := make([]map[string]any, 0, cfg.SampleSize)

	for rows.Next() {
		row, err := scanRow(rows, names)
		if err != nil {
			return nil, err
		}

		sampled = append(sampled, row)
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	return sampled, nil
}

// orderClause renders the ORDER BY for a strategy, or the empty string when the
// table has no usable ordering.
func orderClause(ordering dbschema.OrderingStrategy) string {
	var columns []string

	switch ordering.Kind {
	case dbschema.OrderPrimaryKey:
		columns = ordering.Columns
	case dbschema.OrderTimestamp, dbschema.OrderAutoIncrement, dbschema.OrderSystemRowID:
		if ordering.Column != nil {
			columns = []string{*ordering.Column}
		}
	case dbschema.OrderUnordered:
	}

	if len(columns) == 0 {
		return ""
	}

	quoted := make([]string, 0, len(columns))
	for _, column := range columns {
		quoted = append(quoted, quoteIdentifier(column)+" DESC")
	}

	return " ORDER BY " + strings.Join(quoted, ", ")
}

// scanRow reads one row into a column-name-keyed map.
func scanRow(rows *sql.Rows, names []string) (map[string]any, error) {
	cells := make([]any, len(names))
	targets := make([]any, len(names))

	for i := range cells {
		targets[i] = &cells[i]
	}

	if err := rows.Scan(targets...); err != nil {
		return nil, err
	}

	row := make(map[string]any, len(names))
	for i, name := range names {
		row[name] = normalizeValue(cells[i])
	}

	return row, nil
}

// normalizeValue converts a driver value into something that survives a JSON
// round trip as itself.
//
// A []byte marshals to base64, which is right for VARBINARY and wrong for the
// text some columns arrive as, so valid UTF-8 is reported as a string.
func normalizeValue(value any) any {
	raw, ok := value.([]byte)
	if !ok {
		return value
	}

	if utf8.Valid(raw) {
		return string(raw)
	}

	return raw
}

// sampledRows reports how many rows a sample holds in the width the document
// uses.
//
// The count cannot exceed the row limit the query was given, and that limit is
// itself bounded by dbadapter.MaxSampleSize, so the conversion is safe -- but
// the bound is stated here rather than assumed.
func sampledRows(rows []map[string]any) uint32 {
	if len(rows) > int(dbadapter.MaxSampleSize) {
		return dbadapter.MaxSampleSize
	}

	//nolint:gosec // G115: the line above bounds the count by MaxSampleSize.
	return uint32(len(rows))
}

// appendSensitiveWarnings records which columns hold values matching the
// configured patterns.
//
// The warning names the column and the rule, never the value. A warning that
// quoted what it found would put the credential it detected into the document
// that the credential scan then rejects.
func appendSensitiveWarnings(sample *dbschema.TableSample, cfg dbadapter.SamplingConfig) {
	if !cfg.WarnSensitive {
		return
	}

	matcher, err := cfg.Matcher()
	if err != nil || matcher.Len() == 0 {
		return
	}

	reported := map[string]struct{}{}

	for _, row := range sample.Rows {
		for _, name := range sample.ColumnNames() {
			text, ok := row[name].(string)
			if !ok {
				continue
			}

			description, matched := matcher.Match(text)
			if !matched {
				continue
			}

			key := name + ": " + description
			if _, seen := reported[key]; seen {
				continue
			}

			reported[key] = struct{}{}
			sample.Warnings = append(sample.Warnings,
				fmt.Sprintf("column %q holds values matching %s", name, description))
		}
	}
}
