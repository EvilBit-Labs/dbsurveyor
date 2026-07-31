package sqlite

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// rowidTablesQuery reports whether a name belongs to an ordinary table that has
// a rowid.
//
// Both conditions matter. A WITHOUT ROWID table reports wr = 1, and a view or a
// virtual table has no rowid at all whatever wr says. Ordering by a column the
// object does not have is a syntax error rather than a fallback, so the check is
// made before the ordering is chosen and not after the query fails.
const rowidTablesQuery = `SELECT 1 FROM pragma_table_list
WHERE name = ? AND type = 'table' AND wr = 0`

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
		return dbschema.TableSample{}, fmt.Errorf("sqlite: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("sqlite: invalid sampling configuration: %w", err)
	}

	shape, err := a.collectColumns(ctx, table.Table)
	if err != nil {
		return dbschema.TableSample{}, fmt.Errorf("sqlite: describe %s for sampling: %w", table, err)
	}

	ordering, err := a.orderingFor(ctx, table.Table, shape, cfg)
	if err != nil {
		return dbschema.TableSample{}, err
	}

	rows, err := a.selectRows(ctx, table, ordering, cfg)
	if err != nil {
		return dbschema.TableSample{}, fmt.Errorf("sqlite: sample %s: %w", table, err)
	}

	status := dbschema.Complete()
	sample := dbschema.TableSample{
		TableName:        table.Table,
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

// orderingFor picks how to define "most recent" for a table.
//
// The preference order runs from the most meaningful signal to the least: an
// operator-named timestamp column says what they consider recent, an
// auto-increment key approximates insertion order, a primary key at least gives
// a stable order, and the rowid is the engine's own insertion counter. A table
// with none of those is sampled unordered rather than not sampled.
func (a *Adapter) orderingFor(
	ctx context.Context,
	table string,
	shape tableShape,
	cfg dbadapter.SamplingConfig,
) (dbschema.OrderingStrategy, error) {
	if column, ok := timestampColumn(shape.columns, cfg.TimestampColumns); ok {
		descending := dbschema.Descending

		return dbschema.OrderingStrategy{
			Kind:      dbschema.OrderTimestamp,
			Column:    &column,
			Direction: &descending,
		}, nil
	}

	for _, column := range shape.columns {
		if column.AutoGenerate {
			name := column.Name

			return dbschema.OrderingStrategy{Kind: dbschema.OrderAutoIncrement, Column: &name}, nil
		}
	}

	if shape.primaryKey != nil && len(shape.primaryKey.Columns) > 0 {
		return dbschema.OrderingStrategy{
			Kind:    dbschema.OrderPrimaryKey,
			Columns: shape.primaryKey.Columns,
		}, nil
	}

	hasRowid, err := a.hasRowid(ctx, table)
	if err != nil {
		return dbschema.OrderingStrategy{}, fmt.Errorf("sqlite: check rowid of %q: %w", table, err)
	}

	if hasRowid {
		rowid := "rowid"

		return dbschema.OrderingStrategy{Kind: dbschema.OrderSystemRowID, Column: &rowid}, nil
	}

	return dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}, nil
}

// hasRowid reports whether the table has the engine's implicit row identifier.
func (a *Adapter) hasRowid(ctx context.Context, table string) (bool, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var present int
	err := a.db.QueryRowContext(queryCtx, rowidTablesQuery, table).Scan(&present)

	switch {
	case err == nil:
		return true, nil
	case isNoRows(err):
		return false, nil
	default:
		return false, err
	}
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

	columns, err := rows.Columns()
	if err != nil {
		return nil, err
	}

	sampled := make([]map[string]any, 0, cfg.SampleSize)

	for rows.Next() {
		row, err := scanRow(rows, columns)
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
// under a primary key or a rowid the newest rows are the highest ones.
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
func scanRow(rows *sql.Rows, columns []string) (map[string]any, error) {
	cells := make([]any, len(columns))
	targets := make([]any, len(columns))

	for i := range cells {
		targets[i] = &cells[i]
	}

	if err := rows.Scan(targets...); err != nil {
		return nil, err
	}

	row := make(map[string]any, len(columns))
	for i, name := range columns {
		row[name] = normalizeValue(cells[i])
	}

	return row, nil
}

// normalizeValue converts a driver value into something that survives a JSON
// round trip as itself.
//
// A []byte marshals to base64, which is right for a BLOB and wrong for the TEXT
// value some drivers also hand back as bytes. Valid UTF-8 is therefore reported
// as a string. The heuristic can misjudge binary that happens to be valid UTF-8;
// that is a cosmetic error in a sample, where reporting every string column as
// base64 would be a legibility failure in every document.
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

// timestampColumn finds the first configured timestamp column the table
// actually has, matched without regard to case because engines differ on how
// they fold identifiers.
func timestampColumn(columns []dbschema.Column, preferred []string) (string, bool) {
	for _, want := range preferred {
		for _, column := range columns {
			if strings.EqualFold(column.Name, want) {
				return column.Name, true
			}
		}
	}

	return "", false
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
	if err != nil {
		return
	}

	if matcher.Len() == 0 {
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

// isNoRows reports whether err is database/sql's empty-result sentinel.
func isNoRows(err error) bool {
	return errors.Is(err, sql.ErrNoRows)
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
