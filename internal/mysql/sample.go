package mysql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// samplingColumnsQuery describes one table well enough to choose an ordering.
const samplingColumnsQuery = `SELECT COLUMN_NAME, COLUMN_KEY, EXTRA
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
ORDER BY ORDINAL_POSITION`

// samplingColumn is what the sampler needs to know about a column.
type samplingColumn struct {
	name          string
	primaryKey    bool
	autoIncrement bool
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
	if a.db == nil {
		return dbschema.TableSample{}, ErrClosed
	}

	if err := table.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mysql: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mysql: invalid sampling configuration: %w", err)
	}

	target := table
	if target.Schema == "" {
		target.Schema = a.database
	}

	columns, err := a.describeForSampling(ctx, target)
	if err != nil {
		return dbschema.TableSample{}, err
	}

	ordering := orderingFor(columns, cfg)

	rows, err := a.selectRows(ctx, target, ordering, cfg)
	if err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mysql: sample %s: %w", target, err)
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

	err := a.eachRow(ctx, samplingColumnsQuery, []any{table.Schema, table.Table}, func(rows *sql.Rows) error {
		var (
			name    string
			keyKind sql.NullString
			extra   sql.NullString
		)

		if err := rows.Scan(&name, &keyKind, &extra); err != nil {
			return err
		}

		columns = append(columns, samplingColumn{
			name:          name,
			primaryKey:    keyKind.Valid && keyKind.String == "PRI",
			autoIncrement: extra.Valid && strings.Contains(strings.ToLower(extra.String), "auto_increment"),
		})

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mysql: describe %s for sampling: %w", table, err)
	}

	if len(columns) == 0 {
		return nil, fmt.Errorf("%w: %s", ErrUnknownTable, table)
	}

	return columns, nil
}

// orderingFor picks how to define "most recent" for a table.
//
// The preference order runs from the most meaningful signal to the least: an
// operator-named timestamp column says what they consider recent, an
// auto-increment key approximates insertion order, and a primary key at least
// gives a stable order. A table with none of those is sampled unordered rather
// than not sampled.
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
	//nolint:gosec // G202: the only interpolated values are identifiers, each escaped by qualify.
	statement := `SELECT * FROM ` + qualify(table.Schema, table.Table) +
		orderClause(ordering) + ` LIMIT ?`

	rows, err := a.db.QueryContext(queryCtx, statement, cfg.SampleSize)
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
//
// The direction is DESC for every ordered case. "Most recent" is the point of
// the strategy: the newest rows are the ones an operator is trying to see, and
// under a primary key or an auto-increment column the newest rows are the
// highest ones.
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
// The MySQL protocol hands back most values as bytes, and a []byte marshals to
// base64 -- right for a BLOB, wrong for the TEXT and VARCHAR columns that arrive
// the same way. Valid UTF-8 is therefore reported as a string. The heuristic can
// misjudge binary that happens to be valid UTF-8; that is a cosmetic error in a
// sample, where reporting every string column as base64 would be a legibility
// failure in every document.
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
