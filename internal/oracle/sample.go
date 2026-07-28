package oracle

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
// structure supports, bounded by a bound row limit. No temporary object is
// created and no session state is left behind.
func (a *Adapter) SampleTable(
	ctx context.Context,
	table dbadapter.TableRef,
	cfg dbadapter.SamplingConfig,
) (dbschema.TableSample, error) {
	if a.db == nil {
		return dbschema.TableSample{}, ErrClosed
	}

	if err := table.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("oracle: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("oracle: invalid sampling configuration: %w", err)
	}

	// An unquoted identifier was folded to upper case on the way in, so a
	// lookup by the name as an operator typed it finds nothing.
	target := dbadapter.TableRef{
		Schema: normalize(table.Schema),
		Table:  normalize(table.Table),
	}
	if target.Schema == "" {
		target.Schema = a.owner
	}

	columns, err := a.describeForSampling(ctx, target)
	if err != nil {
		return dbschema.TableSample{}, err
	}

	ordering := orderingFor(columns, cfg)

	rows, err := a.selectRows(ctx, target, ordering, cfg)
	if err != nil {
		return dbschema.TableSample{}, fmt.Errorf("oracle: sample %s: %w", target, err)
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

	args := []any{table.Schema, table.Schema, table.Table}

	err := a.eachRow(ctx, samplingColumnsQuery, args, func(rows *sql.Rows) error {
		var (
			column   samplingColumn
			identity int64
			inTheKey int64
		)

		if err := rows.Scan(&column.name, &identity, &inTheKey); err != nil {
			return err
		}

		column.autoIncrement = identity == 1
		column.primaryKey = inTheKey == 1
		columns = append(columns, column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: describe %s for sampling: %w", table, err)
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
//
// Oracle's ROWID is deliberately not used as a fallback. It encodes physical
// placement rather than insertion order, and a row that has been updated can
// move, so ordering by it would produce a "most recent" that means nothing.
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
// has, matched without regard to case because Oracle stores an unquoted name
// folded to upper case and an operator writes it however they like.
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
// FETCH FIRST n ROWS ONLY is 12.1 syntax and is why this package declares that
// floor. The older ROWNUM rewrite cannot be combined with ORDER BY without a
// subquery, because ROWNUM is assigned before the sort -- so a naive
// "WHERE ROWNUM <= n ORDER BY ..." returns an arbitrary n rows and then sorts
// them, which is not the most recent n at all.
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
		orderClause(ordering) + ` FETCH FIRST :1 ROWS ONLY`

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
// A []byte marshals to base64, which is right for a RAW or BLOB and wrong for
// the text some columns arrive as, so valid UTF-8 is reported as a string.
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
