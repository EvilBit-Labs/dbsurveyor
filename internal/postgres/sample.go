package postgres

import (
	"context"
	"fmt"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/jackc/pgx/v5"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// defaultSchema is the namespace a table reference falls back to when it names
// none. It is not read from search_path: a survey that silently picked up a
// server-side setting would collect a different table on a different server.
const defaultSchema = "public"

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
// structure supports, bounded by a bound LIMIT. No temporary object is created
// and no session state is left behind.
func (a *Adapter) SampleTable(
	ctx context.Context,
	table dbadapter.TableRef,
	cfg dbadapter.SamplingConfig,
) (dbschema.TableSample, error) {
	if a.pool == nil {
		return dbschema.TableSample{}, ErrClosed
	}

	if err := table.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("postgres: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("postgres: invalid sampling configuration: %w", err)
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
		return dbschema.TableSample{}, fmt.Errorf("postgres: sample %s: %w", target, err)
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

	err := a.eachRow(ctx, samplingColumnsQuery, []any{table.Schema, table.Table}, func(rows pgx.Rows) error {
		var column samplingColumn
		if err := rows.Scan(&column.name, &column.autoIncrement, &column.primaryKey); err != nil {
			return err
		}

		columns = append(columns, column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("postgres: describe %s for sampling: %w", table, err)
	}

	if len(columns) == 0 {
		return nil, fmt.Errorf("%w: %s", ErrUnknownTable, table)
	}

	return columns, nil
}

// orderingFor picks how to define "most recent" for a table.
//
// The preference order runs from the most meaningful signal to the least: an
// operator-named timestamp column says what they consider recent, a generated
// key approximates insertion order, and a primary key at least gives a stable
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

// timestampColumn finds the first configured timestamp column the table
// actually has, matched without regard to case because engines differ on how
// they fold identifiers.
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
	// travels through the per-engine quoting helper, and the row limit is bound
	// as a parameter rather than formatted in.

	statement := `SELECT * FROM ` + qualify(table.Schema, table.Table) +
		orderClause(ordering) + ` LIMIT $1`

	rows, err := a.pool.Query(queryCtx, statement, int64(cfg.SampleSize))
	if err != nil {
		return nil, err
	}

	defer rows.Close()

	sampled := make([]map[string]any, 0, cfg.SampleSize)

	for rows.Next() {
		values, err := rows.Values()
		if err != nil {
			return nil, err
		}

		row := make(map[string]any, len(values))
		for i, field := range rows.FieldDescriptions() {
			row[field.Name] = normalizeValue(values[i])
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
//
// The direction is DESC for every ordered case. "Most recent" is the point of
// the strategy: the newest rows are the ones an operator is trying to see, and
// under a primary key or a generated key the newest rows are the highest ones.
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

// normalizeValue converts a driver value into something that survives a JSON
// round trip as itself.
//
// pgx decodes most types into Go values already, so the only case needing help
// is bytea, which arrives as a []byte and marshals to base64. That is right for
// binary and wrong for the text a bytea column occasionally holds, so valid
// UTF-8 is reported as a string.
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

// sampledRows reports how many rows a sample holds in the width the document
// uses.
//
// The count cannot exceed the row limit the query was given, and that limit is
// itself bounded by dbadapter.MaxSampleSize, so the conversion is safe -- but
// the bound is stated here rather than assumed, because it lives two files away
// from the conversion.
func sampledRows(rows []map[string]any) uint32 {
	if len(rows) > int(dbadapter.MaxSampleSize) {
		return dbadapter.MaxSampleSize
	}

	//nolint:gosec // G115: the line above bounds the count by MaxSampleSize.
	return uint32(len(rows))
}
