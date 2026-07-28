package mysql

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
// Each reads INFORMATION_SCHEMA for the whole database at once rather than once
// per table. The join in keyColumnsQuery is what turns the constraint catalog
// into something usable: KEY_COLUMN_USAGE says which columns a constraint
// covers, TABLE_CONSTRAINTS says what kind of constraint it is, and
// REFERENTIAL_CONSTRAINTS says what a foreign key does on update and delete.
// None of the three is sufficient alone.
const (
	versionQuery = `SELECT VERSION()`

	tablesQuery = `SELECT TABLE_NAME, TABLE_ROWS, TABLE_COMMENT
FROM INFORMATION_SCHEMA.TABLES
WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'
ORDER BY TABLE_NAME`

	columnsQuery = `SELECT TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION, COLUMN_DEFAULT,
       IS_NULLABLE, DATA_TYPE, COLUMN_TYPE, CHARACTER_MAXIMUM_LENGTH,
       NUMERIC_PRECISION, COLUMN_KEY, EXTRA, COLUMN_COMMENT
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_SCHEMA = ?
ORDER BY TABLE_NAME, ORDINAL_POSITION`

	indexesQuery = `SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, SEQ_IN_INDEX,
       COLUMN_NAME, COLLATION, INDEX_TYPE
FROM INFORMATION_SCHEMA.STATISTICS
WHERE TABLE_SCHEMA = ?
ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX`

	keyColumnsQuery = `SELECT k.TABLE_NAME, k.CONSTRAINT_NAME, c.CONSTRAINT_TYPE, k.COLUMN_NAME,
       k.REFERENCED_TABLE_SCHEMA, k.REFERENCED_TABLE_NAME, k.REFERENCED_COLUMN_NAME,
       r.UPDATE_RULE, r.DELETE_RULE
FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS k
JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS c
  ON c.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA
 AND c.CONSTRAINT_NAME = k.CONSTRAINT_NAME
 AND c.TABLE_NAME = k.TABLE_NAME
LEFT JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS r
  ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA
 AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME
 AND r.TABLE_NAME = k.TABLE_NAME
WHERE k.TABLE_SCHEMA = ?
ORDER BY k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION`

	viewsQuery = `SELECT TABLE_NAME, VIEW_DEFINITION
FROM INFORMATION_SCHEMA.VIEWS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME`

	routinesQuery = `SELECT ROUTINE_NAME, ROUTINE_TYPE, DTD_IDENTIFIER, ROUTINE_DEFINITION, ROUTINE_BODY
FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME`

	triggersQuery = `SELECT TRIGGER_NAME, EVENT_OBJECT_TABLE, EVENT_MANIPULATION,
       ACTION_TIMING, ACTION_STATEMENT
FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME`
)

// CollectSchema reads the structure of the configured database.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mysql: invalid collection configuration: %w", err)
	}

	started := time.Now()

	info := dbschema.NewDatabaseInfo(a.database, dbschema.MySQL)
	info.SystemDatabase = isSystemDatabase(a.database)

	if version, err := a.version(ctx); err == nil {
		info.Version = &version
	}

	schema := dbschema.New(info, cfg.CollectorVersion, started)

	if err := a.collectTables(ctx, schema, cfg); err != nil {
		return nil, err
	}

	if err := a.collectOptionalObjects(ctx, schema, cfg); err != nil {
		return nil, err
	}

	schema.AggregateIndexesAndConstraints()
	schema.CollectionMetadata.SetDuration(time.Since(started))

	return schema, nil
}

// collectOptionalObjects reads the object kinds the configuration can turn off.
func (a *Adapter) collectOptionalObjects(
	ctx context.Context,
	schema *dbschema.Schema,
	cfg dbadapter.CollectionConfig,
) error {
	if cfg.IncludeViews {
		if err := a.collectViews(ctx, schema); err != nil {
			return err
		}
	}

	if cfg.IncludeRoutines {
		if err := a.collectRoutines(ctx, schema); err != nil {
			return err
		}
	}

	if cfg.IncludeTriggers {
		if err := a.collectTriggers(ctx, schema); err != nil {
			return err
		}
	}

	return nil
}

// version reads the server version string.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var version string
	if err := a.db.QueryRowContext(queryCtx, versionQuery).Scan(&version); err != nil {
		return "", fmt.Errorf("mysql: read server version: %w", err)
	}

	return version, nil
}

// collectTables reads every base table with its columns, keys, and indexes.
//
// The four catalog queries run once for the whole database and are grouped in
// memory, rather than once per table. On a database of any size the difference
// is the whole runtime of a survey: per-table metadata makes collection O(4N+1)
// round trips, and grouping makes it O(5).
func (a *Adapter) collectTables(
	ctx context.Context,
	schema *dbschema.Schema,
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

	keys, err := a.readKeyConstraints(ctx)
	if err != nil {
		return err
	}

	indexes := map[string][]dbschema.Index{}
	if cfg.IncludeIndexes {
		if indexes, err = a.readIndexes(ctx); err != nil {
			return err
		}
	}

	database := a.database

	for i := range tables {
		table := &tables[i]
		table.Schema = &database
		table.Columns = columns[table.Name]
		table.Indexes = indexes[table.Name]

		grouped := keys[table.Name]
		table.PrimaryKey = grouped.primaryKey
		table.ForeignKeys = grouped.foreignKeys

		if cfg.IncludeConstraint {
			table.Constraints = grouped.constraints
		}

		if len(table.Columns) == 0 {
			schema.AddWarning(fmt.Sprintf(
				"table %q reported no columns; the credential may not be able to read it", table.Name))
		}
	}

	schema.Tables = tables

	return nil
}

// readTables reads the base tables of the database.
func (a *Adapter) readTables(ctx context.Context) ([]dbschema.Table, error) {
	tables := []dbschema.Table{}

	err := a.eachRow(ctx, tablesQuery, []any{a.database}, func(rows *sql.Rows) error {
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
			Columns:     []dbschema.Column{},
			ForeignKeys: []dbschema.ForeignKey{},
			Indexes:     []dbschema.Index{},
			Constraints: []dbschema.Constraint{},
		}

		// TABLE_ROWS is NULL for a table the storage engine has no statistics
		// for, which covers a freshly created table, an empty one, and every
		// table on some engines. Scanning it into a plain integer would fail on
		// all of them, so it is nullable here and reported as zero.
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
		return nil, fmt.Errorf("mysql: list tables of %q: %w", a.database, err)
	}

	return tables, nil
}

// readColumns reads every column in the database, grouped by table.
//
// ORDINAL_POSITION is declared INT UNSIGNED and NON_UNIQUE in STATISTICS is
// declared signed INT. Under the binary protocol a prepared statement uses, the
// driver reports each with its declared signedness, so scanning one into the
// other's Go type is a decode error rather than a silent conversion. Each is
// scanned into the type its column actually has.
func (a *Adapter) readColumns(ctx context.Context) (map[string][]dbschema.Column, error) {
	grouped := map[string][]dbschema.Column{}

	err := a.eachRow(ctx, columnsQuery, []any{a.database}, func(rows *sql.Rows) error {
		var (
			table     string
			name      string
			position  uint32
			dflt      sql.NullString
			nullable  string
			dataType  string
			fullType  string
			maxLength sql.NullInt64
			precision sql.NullInt64
			keyKind   sql.NullString
			extra     sql.NullString
			comment   sql.NullString
		)

		err := rows.Scan(&table, &name, &position, &dflt, &nullable, &dataType, &fullType,
			&maxLength, &precision, &keyKind, &extra, &comment)
		if err != nil {
			return err
		}

		column := dbschema.Column{
			Name:            name,
			DataType:        mapDataType(describeType(dataType, fullType, maxLength, precision)),
			Nullable:        strings.EqualFold(nullable, "YES"),
			PrimaryKey:      keyKind.Valid && keyKind.String == "PRI",
			AutoGenerate:    extra.Valid && strings.Contains(strings.ToLower(extra.String), "auto_increment"),
			OrdinalPosition: position,
		}

		if dflt.Valid {
			column.Default = &dflt.String
		}

		if comment.Valid && comment.String != "" {
			column.Comment = &comment.String
		}

		grouped[table] = append(grouped[table], column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mysql: read columns of %q: %w", a.database, err)
	}

	return grouped, nil
}

// describeType assembles the type description the mapping needs from the
// nullable catalog columns.
func describeType(dataType, fullType string, maxLength, precision sql.NullInt64) columnType {
	described := columnType{DataType: dataType, FullType: fullType}

	if maxLength.Valid && maxLength.Int64 > 0 && maxLength.Int64 <= int64(^uint32(0)) {
		length := uint32(maxLength.Int64)
		described.MaxLength = &length
	}

	if precision.Valid && precision.Int64 > 0 && precision.Int64 <= int64(^uint8(0)) {
		digits := uint8(precision.Int64)
		described.Precision = &digits
	}

	return described
}

// readIndexes reads every index in the database, grouped by table.
func (a *Adapter) readIndexes(ctx context.Context) (map[string][]dbschema.Index, error) {
	grouped := map[string][]dbschema.Index{}
	position := map[string]int{}

	err := a.eachRow(ctx, indexesQuery, []any{a.database}, func(rows *sql.Rows) error {
		var (
			table string
			name  string
			// NON_UNIQUE is a signed INT in STATISTICS, unlike ORDINAL_POSITION
			// in COLUMNS. Scanning it as unsigned is the mistake this pairing
			// invites.
			nonUnique int64
			sequence  uint32
			column    sql.NullString
			collation sql.NullString
			indexType sql.NullString
		)

		err := rows.Scan(&table, &name, &nonUnique, &sequence, &column, &collation, &indexType)
		if err != nil {
			return err
		}

		key := table + "\x00" + name

		at, seen := position[key]
		if !seen {
			index := dbschema.Index{
				Name:      name,
				TableName: table,
				Schema:    &a.database,
				Columns:   []dbschema.IndexColumn{},
				Unique:    nonUnique == 0,
				Primary:   name == primaryIndexName,
			}

			if indexType.Valid {
				index.IndexType = &indexType.String
			}

			grouped[table] = append(grouped[table], index)
			at = len(grouped[table]) - 1
			position[key] = at
		}

		// A NULL column name is an index over an expression, which has no column
		// to record. The index still appears, with the columns it does name.
		if column.Valid {
			grouped[table][at].Columns = append(grouped[table][at].Columns, dbschema.IndexColumn{
				Name:      column.String,
				SortOrder: sortDirection(collation),
			})
		}

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mysql: read indexes of %q: %w", a.database, err)
	}

	return grouped, nil
}

// primaryIndexName is the name MySQL always gives the index backing a primary
// key. There is no other way to identify it in STATISTICS.
const primaryIndexName = "PRIMARY"

// sortDirection maps the STATISTICS collation column, which is "A" for
// ascending, "D" for descending, and NULL for an unordered index such as a hash.
func sortDirection(collation sql.NullString) *dbschema.SortDirection {
	if !collation.Valid {
		return nil
	}

	direction := dbschema.Ascending
	if collation.String == "D" {
		direction = dbschema.Descending
	}

	return &direction
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
