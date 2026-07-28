package oracle

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The catalog queries.
//
// They read the ALL_* views rather than the DBA_* ones. ALL_* shows exactly what
// the connecting credential can see, so a survey run by an ordinary application
// account works instead of failing on a privilege an operator was never going to
// be granted. The trade is that an object the credential cannot see is absent
// rather than reported as unreadable, which is the right way round: the document
// describes what was surveyed.
//
// Bind placeholders are :1 style, which is what go-ora expects.
const (
	// Both views are restricted on many installations, so a failure here is a
	// missing version string rather than a failed survey.
	versionQuery = `SELECT version FROM product_component_version WHERE ROWNUM = 1`

	tablesQuery = `
SELECT t.table_name, t.num_rows, c.comments
FROM all_tables t
LEFT JOIN all_tab_comments c ON c.owner = t.owner AND c.table_name = t.table_name
WHERE t.owner = :1
ORDER BY t.table_name`

	columnsQuery = `
SELECT c.table_name, c.column_name, c.data_type, c.char_length, c.data_precision,
       c.data_scale, c.nullable, c.data_default, m.comments
FROM all_tab_columns c
LEFT JOIN all_col_comments m
  ON m.owner = c.owner AND m.table_name = c.table_name AND m.column_name = c.column_name
WHERE c.owner = :1
ORDER BY c.table_name, c.column_id`

	// Constraint types: P primary key, R referential, U unique, C check. A NOT
	// NULL constraint is also recorded as C, and is filtered out below because
	// the column's own nullability already carries it.
	constraintsQuery = `
SELECT c.table_name, c.constraint_name, c.constraint_type, cc.column_name,
       c.search_condition_vc, c.r_constraint_name, c.delete_rule
FROM all_constraints c
JOIN all_cons_columns cc
  ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name
WHERE c.owner = :1 AND c.constraint_type IN ('P', 'R', 'U', 'C')
ORDER BY c.table_name, c.constraint_name, cc.position`

	// The referenced side of a foreign key is named by the constraint it points
	// at, so resolving it needs a second pass over the same view.
	referencedKeysQuery = `
SELECT c.constraint_name, c.table_name, cc.column_name
FROM all_constraints c
JOIN all_cons_columns cc
  ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name
WHERE c.owner = :1 AND c.constraint_type IN ('P', 'U')
ORDER BY c.constraint_name, cc.position`

	indexesQuery = `
SELECT i.table_name, i.index_name, i.uniqueness, i.index_type, ic.column_name, ic.descend
FROM all_indexes i
JOIN all_ind_columns ic
  ON ic.index_owner = i.owner AND ic.index_name = i.index_name
WHERE i.owner = :1
ORDER BY i.table_name, i.index_name, ic.column_position`

	viewsQuery = `SELECT view_name, text_vc FROM all_views WHERE owner = :1 ORDER BY view_name`

	routinesQuery = `
SELECT object_name, object_type
FROM all_objects
WHERE owner = :1 AND object_type IN ('PROCEDURE', 'FUNCTION')
ORDER BY object_name`

	triggersQuery = `
SELECT trigger_name, table_name, triggering_event, trigger_type
FROM all_triggers
WHERE owner = :1 AND base_object_type = 'TABLE'
ORDER BY trigger_name`

	userTypesQuery = `
SELECT type_name, typecode FROM all_types WHERE owner = :1 ORDER BY type_name`

	samplingColumnsQuery = `
SELECT c.column_name,
       CASE WHEN c.identity_column = 'YES' THEN 1 ELSE 0 END,
       CASE WHEN p.column_name IS NULL THEN 0 ELSE 1 END
FROM all_tab_columns c
LEFT JOIN (
    SELECT cc.table_name, cc.column_name
    FROM all_constraints ct
    JOIN all_cons_columns cc
      ON cc.owner = ct.owner AND cc.constraint_name = ct.constraint_name
    WHERE ct.owner = :1 AND ct.constraint_type = 'P'
) p ON p.table_name = c.table_name AND p.column_name = c.column_name
WHERE c.owner = :2 AND c.table_name = :3
ORDER BY c.column_id`
)

// CollectSchema reads the structure of the surveyed schema.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("oracle: invalid collection configuration: %w", err)
	}

	started := time.Now()

	info := dbschema.NewDatabaseInfo(a.owner, dbschema.Oracle)
	if version, err := a.version(ctx); err == nil {
		info.Version = &version
	}

	document := dbschema.New(info, cfg.CollectorVersion, started)

	if err := a.collectTables(ctx, document, cfg); err != nil {
		return nil, err
	}

	if err := a.collectOptionalObjects(ctx, document, cfg); err != nil {
		return nil, err
	}

	document.AggregateIndexesAndConstraints()
	document.CollectionMetadata.SetDuration(time.Since(started))

	return document, nil
}

// collectOptionalObjects reads the object kinds the configuration can turn off.
func (a *Adapter) collectOptionalObjects(
	ctx context.Context,
	document *dbschema.Schema,
	cfg dbadapter.CollectionConfig,
) error {
	if cfg.IncludeViews {
		if err := a.collectViews(ctx, document); err != nil {
			return err
		}
	}

	if cfg.IncludeRoutines {
		if err := a.collectRoutines(ctx, document); err != nil {
			return err
		}
	}

	if cfg.IncludeTriggers {
		if err := a.collectTriggers(ctx, document); err != nil {
			return err
		}
	}

	if cfg.IncludeTypes {
		if err := a.collectUserTypes(ctx, document); err != nil {
			return err
		}
	}

	return nil
}

// version reads the server version, which many installations restrict.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var version string
	if err := a.db.QueryRowContext(queryCtx, versionQuery).Scan(&version); err != nil {
		return "", fmt.Errorf("oracle: read server version: %w", err)
	}

	return version, nil
}

// collectTables reads every table with its columns, keys, indexes, and
// constraints.
func (a *Adapter) collectTables(
	ctx context.Context,
	document *dbschema.Schema,
	cfg dbadapter.CollectionConfig,
) error {
	tables, err := a.readTables(ctx)
	if err != nil {
		return err
	}

	columns, err := a.readColumns(ctx)
	if err != nil {
		return err
	}

	keys, err := a.readConstraints(ctx)
	if err != nil {
		return err
	}

	indexes := map[string][]dbschema.Index{}
	if cfg.IncludeIndexes {
		if indexes, err = a.readIndexes(ctx); err != nil {
			return err
		}
	}

	for i := range tables {
		table := &tables[i]

		table.Columns = columns[table.Name]
		table.Indexes = indexes[table.Name]
		table.PrimaryKey = keys[table.Name].primaryKey
		table.ForeignKeys = keys[table.Name].foreignKeys

		if cfg.IncludeConstraint {
			table.Constraints = keys[table.Name].constraints
		}

		if len(table.Columns) == 0 {
			document.AddWarning(fmt.Sprintf(
				"table %q reported no columns; the credential may not be able to read it", table.Name))
		}
	}

	document.Tables = tables

	return nil
}

// readTables reads the tables of the surveyed schema.
func (a *Adapter) readTables(ctx context.Context) ([]dbschema.Table, error) {
	tables := []dbschema.Table{}

	err := a.eachRow(ctx, tablesQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name     string
			estimate sql.NullInt64
			comment  sql.NullString
		)

		if err := rows.Scan(&name, &estimate, &comment); err != nil {
			return err
		}

		table := dbschema.Table{
			Name:        name,
			Schema:      ownerPointer(a.owner),
			Columns:     []dbschema.Column{},
			ForeignKeys: []dbschema.ForeignKey{},
			Indexes:     []dbschema.Index{},
			Constraints: []dbschema.Constraint{},
		}

		// NUM_ROWS is NULL until the optimizer has gathered statistics for the
		// table, which on a freshly loaded schema is every table. Scanning it
		// into a plain integer would fail on all of them.
		table.RowCount = new(uint64)
		if estimate.Valid && estimate.Int64 > 0 {
			*table.RowCount = uint64(estimate.Int64)
		}

		if comment.Valid && comment.String != "" {
			table.Comment = &comment.String
		}

		tables = append(tables, table)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: list tables of %q: %w", a.owner, err)
	}

	return tables, nil
}

// readColumns reads every column in the schema, grouped by table.
func (a *Adapter) readColumns(ctx context.Context) (map[string][]dbschema.Column, error) {
	grouped := map[string][]dbschema.Column{}

	err := a.eachRow(ctx, columnsQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			table     string
			name      string
			dataType  string
			length    sql.NullInt64
			precision sql.NullInt64
			scale     sql.NullInt64
			nullable  string
			dflt      sql.NullString
			comment   sql.NullString
		)

		err := rows.Scan(&table, &name, &dataType, &length, &precision,
			&scale, &nullable, &dflt, &comment)
		if err != nil {
			return err
		}

		column := dbschema.Column{
			Name:     name,
			DataType: mapDataType(describeType(dataType, length, precision, scale)),
			// Oracle spells nullability as Y or N rather than as a boolean.
			Nullable: strings.EqualFold(strings.TrimSpace(nullable), "Y"),
			//nolint:gosec // G115: bounded by one table's column count.
			OrdinalPosition: uint32(len(grouped[table])) + 1,
		}

		if dflt.Valid {
			// The default arrives with the trailing whitespace of the LONG
			// column it was stored in.
			trimmed := strings.TrimSpace(dflt.String)
			column.Default = &trimmed
		}

		if comment.Valid && comment.String != "" {
			column.Comment = &comment.String
		}

		grouped[table] = append(grouped[table], column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: read columns of %q: %w", a.owner, err)
	}

	return grouped, nil
}

// describeType assembles the type description the mapping needs from the
// nullable catalog columns.
//
// Precision and scale stay absent when the catalog reported them absent: a bare
// NUMBER is a different type from NUMBER(38,0), and defaulting either would
// collapse the two.
func describeType(dataType string, length, precision, scale sql.NullInt64) columnType {
	described := columnType{Name: dataType}

	if length.Valid && length.Int64 > 0 && length.Int64 <= int64(^uint32(0)) {
		size := uint32(length.Int64)
		described.Length = &size
	}

	if precision.Valid && precision.Int64 > 0 && precision.Int64 <= int64(^uint8(0)) {
		digits := uint8(precision.Int64)
		described.Precision = &digits
	}

	if scale.Valid && scale.Int64 >= -32768 && scale.Int64 <= 32767 {
		places := int16(scale.Int64)
		described.Scale = &places
	}

	return described
}

// readIndexes reads every index in the schema, grouped by table.
func (a *Adapter) readIndexes(ctx context.Context) (map[string][]dbschema.Index, error) {
	grouped := map[string][]dbschema.Index{}
	position := map[string]int{}

	err := a.eachRow(ctx, indexesQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			table      string
			name       string
			uniqueness string
			indexType  string
			column     string
			descend    sql.NullString
		)

		err := rows.Scan(&table, &name, &uniqueness, &indexType, &column, &descend)
		if err != nil {
			return err
		}

		lookup := table + "\x00" + name

		at, seen := position[lookup]
		if !seen {
			method := strings.TrimSpace(indexType)
			grouped[table] = append(grouped[table], dbschema.Index{
				Name:      name,
				TableName: table,
				Schema:    ownerPointer(a.owner),
				Columns:   []dbschema.IndexColumn{},
				// Oracle spells uniqueness as a word rather than a flag.
				Unique:    strings.EqualFold(strings.TrimSpace(uniqueness), "UNIQUE"),
				IndexType: &method,
			})
			at = len(grouped[table]) - 1
			position[lookup] = at
		}

		direction := dbschema.Ascending
		if descend.Valid && strings.EqualFold(strings.TrimSpace(descend.String), "DESC") {
			direction = dbschema.Descending
		}

		grouped[table][at].Columns = append(grouped[table][at].Columns, dbschema.IndexColumn{
			Name:      column,
			SortOrder: &direction,
		})

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: read indexes of %q: %w", a.owner, err)
	}

	return grouped, nil
}

// eachRow runs a query under the configured per-query timeout and hands every
// row to scan.
//
// The rows are consumed inside this function rather than returned so that the
// timeout's cancel runs after the last row is read. A helper that returned
// *sql.Rows would have to leak the cancel to its caller, and a caller that
// forgot would cancel the context while the rows were still being read.
func (a *Adapter) eachRow(
	ctx context.Context,
	statement string,
	args []any,
	scan func(*sql.Rows) error,
) error {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	rows, err := a.db.QueryContext(queryCtx, statement, args...)
	if err != nil {
		return err
	}

	defer func() { discardError(rows.Close()) }()

	for rows.Next() {
		if err := scan(rows); err != nil {
			return err
		}
	}

	return rows.Err()
}

// ownerPointer returns a pointer to a copy of the owner name, so that every
// object carrying one does not alias the same variable.
func ownerPointer(owner string) *string {
	name := owner

	return &name
}
