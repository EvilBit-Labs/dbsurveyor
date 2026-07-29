package sqlite

import (
	"context"
	"database/sql"
	"fmt"
	"regexp"
	"strings"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The catalog queries.
//
// Every PRAGMA is reached through its pragma_* table-valued function rather than
// through PRAGMA statement syntax, so the table name travels as a bound
// parameter. PRAGMA statement syntax cannot bind, which would leave a table name
// interpolated into a string literal under a different escaping rule from the
// identifier rule -- the same name needing two escapings in one file is how the
// wrong one gets used. Binding removes the second context entirely.
const (
	versionQuery = `SELECT sqlite_version()`

	// The sqlite_% prefix names the engine's own catalog objects. The escape
	// clause is required: an unescaped underscore is a single-character
	// wildcard, which would also match a user table called "sqliteXfoo".
	tableNamesQuery = `SELECT name FROM sqlite_master
WHERE type = 'table' AND name NOT LIKE 'sqlite\_%' ESCAPE '\'
ORDER BY name`

	viewsQuery = `SELECT name, sql FROM sqlite_master WHERE type = 'view' ORDER BY name`

	triggersQuery = `SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name`

	columnsQuery = `SELECT cid, name, type, "notnull", dflt_value, pk
FROM pragma_table_info(?) ORDER BY cid`

	foreignKeysQuery = `SELECT id, "table", "from", "to", on_update, on_delete
FROM pragma_foreign_key_list(?) ORDER BY id, seq`

	indexListQuery = `SELECT name, "unique", origin FROM pragma_index_list(?) ORDER BY seq`

	indexColumnsQuery = `SELECT name FROM pragma_index_info(?) ORDER BY seqno`
)

// triggerHeaderPattern extracts the timing and event from a CREATE TRIGGER
// statement.
//
// SQLite stores the original statement text and exposes no parsed form, so the
// two fields the schema document wants have to be read back out of the source.
// Timing is optional in the grammar and defaults to BEFORE.
var triggerHeaderPattern = regexp.MustCompile(
	`(?is)\bTRIGGER\b.*?\b(?:(BEFORE|AFTER|INSTEAD\s+OF)\s+)?(DELETE|INSERT|UPDATE)\b`)

// CollectSchema reads the structure of the database file.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("sqlite: invalid collection configuration: %w", err)
	}

	started := time.Now()

	info := dbschema.NewDatabaseInfo(databaseName(a.path), dbschema.SQLite)
	if version, err := a.version(ctx); err == nil {
		info.Version = &version
	}

	schema := dbschema.New(info, cfg.CollectorVersion, started)

	if err := a.collectTables(ctx, schema); err != nil {
		return nil, err
	}

	if cfg.IncludeViews {
		if err := a.collectViews(ctx, schema); err != nil {
			return nil, err
		}
	}

	if cfg.IncludeTriggers {
		if err := a.collectTriggers(ctx, schema); err != nil {
			return nil, err
		}
	}

	schema.AggregateIndexesAndConstraints()
	schema.CollectionMetadata.SetDuration(time.Since(started))

	return schema, nil
}

// version reads the engine version string.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var version string
	if err := a.db.QueryRowContext(queryCtx, versionQuery).Scan(&version); err != nil {
		return "", fmt.Errorf("sqlite: read engine version: %w", err)
	}

	return version, nil
}

// collectTables reads every user table into schema.
func (a *Adapter) collectTables(ctx context.Context, schema *dbschema.Schema) error {
	var names []string

	err := a.eachRow(ctx, tableNamesQuery, nil, func(rows *sql.Rows) error {
		var name string
		if err := rows.Scan(&name); err != nil {
			return err
		}

		names = append(names, name)

		return nil
	})
	if err != nil {
		return fmt.Errorf("sqlite: list tables: %w", err)
	}

	for _, name := range names {
		table, err := a.collectTable(ctx, schema, name)
		if err != nil {
			return err
		}

		schema.Tables = append(schema.Tables, table)
	}

	return nil
}

// collectTable assembles one table. Metadata failures are errors rather than
// warnings: SQLite has no per-object permissions, so a failure here means the
// file is unreadable or corrupt, and a schema missing half a table silently is
// worse than no schema at all.
func (a *Adapter) collectTable(
	ctx context.Context,
	schema *dbschema.Schema,
	name string,
) (dbschema.Table, error) {
	shape, err := a.collectColumns(ctx, name)
	if err != nil {
		return dbschema.Table{}, fmt.Errorf("sqlite: collect columns of %q: %w", name, err)
	}

	foreignKeys, err := a.collectForeignKeys(ctx, name)
	if err != nil {
		return dbschema.Table{}, fmt.Errorf("sqlite: collect foreign keys of %q: %w", name, err)
	}

	indexes, err := a.collectIndexes(ctx, name, shape.primaryKey)
	if err != nil {
		return dbschema.Table{}, fmt.Errorf("sqlite: collect indexes of %q: %w", name, err)
	}

	table := dbschema.Table{
		Name:        name,
		Columns:     shape.columns,
		PrimaryKey:  shape.primaryKey,
		ForeignKeys: foreignKeys,
		Indexes:     indexes.indexes,
		Constraints: indexes.constraints,
	}

	count, err := a.rowCount(ctx, name)
	if err != nil {
		// A table declared WITHOUT ROWID has no rowid to take a maximum of. That
		// is a property of the table, not a failure of the survey, so it is
		// recorded and the table is kept.
		schema.AddWarning(fmt.Sprintf("no row estimate for table %q: %v", name, err))
	} else {
		table.RowCount = &count
	}

	return table, nil
}

// tableShape is the column list of a table together with the primary key the
// same query reports, returned as one value because the two are read from the
// same rows and separating them would mean querying twice.
type tableShape struct {
	columns    []dbschema.Column
	primaryKey *dbschema.PrimaryKey
}

// collectColumns reads a table's or view's columns.
//
// The name and declared type are scanned into plain strings on purpose. A NULL
// in either is a scan error that aborts the collection, which is what should
// happen: substituting a zero value would put a column with an empty name into
// the document, and every consumer downstream would then be describing a table
// that does not exist.
func (a *Adapter) collectColumns(ctx context.Context, table string) (tableShape, error) {
	var (
		shape    tableShape
		keyParts []keyPart
	)

	err := a.eachRow(ctx, columnsQuery, []any{table}, func(rows *sql.Rows) error {
		var (
			// cid is read because the query orders by it; the ordinal position
			// is counted from the row order rather than taken from it.
			cid       int64
			name      string
			declared  string
			notNull   int64
			dflt      sql.NullString
			keyColumn int64
		)

		if err := rows.Scan(&cid, &name, &declared, &notNull, &dflt, &keyColumn); err != nil {
			return err
		}

		column := dbschema.Column{
			Name:       name,
			DataType:   mapDataType(declared),
			Nullable:   notNull == 0,
			PrimaryKey: keyColumn > 0,
			// A SQLite column is auto-generated exactly when it is the single
			// INTEGER PRIMARY KEY, which is an alias for the rowid.
			AutoGenerate: keyColumn == 1 && strings.EqualFold(strings.TrimSpace(declared), "integer"),
			// PRAGMA table_info numbers columns from zero and the document
			// requires one-based positions, so the position is counted rather
			// than converted. Counting also survives a catalog that reports a
			// cid this code did not expect.
			//nolint:gosec // G115: bounded by the column count of one table.
			OrdinalPosition: uint32(len(shape.columns)) + 1,
		}

		if dflt.Valid {
			column.Default = &dflt.String
		}

		if keyColumn > 0 {
			keyParts = append(keyParts, keyPart{position: keyColumn, column: name})
		}

		shape.columns = append(shape.columns, column)

		return nil
	})
	if err != nil {
		return tableShape{}, err
	}

	if len(shape.columns) == 0 {
		return tableShape{}, fmt.Errorf("%w: %q", ErrUnknownTable, table)
	}

	shape.primaryKey = assemblePrimaryKey(keyParts)

	// The single-column INTEGER PRIMARY KEY is only an alias for the rowid when
	// the table has exactly one key column.
	if shape.primaryKey != nil && len(shape.primaryKey.Columns) > 1 {
		for i := range shape.columns {
			shape.columns[i].AutoGenerate = false
		}
	}

	return shape, nil
}

// keyPart is one column of a primary key with its position in the key, which
// PRAGMA table_info reports separately from the column order.
type keyPart struct {
	position int64
	column   string
}

// collectForeignKeys reads a table's foreign keys, grouping the per-column rows
// the pragma returns back into one constraint per key.
func (a *Adapter) collectForeignKeys(ctx context.Context, table string) ([]dbschema.ForeignKey, error) {
	var (
		keys []dbschema.ForeignKey
		byID = map[int64]int{}
	)

	err := a.eachRow(ctx, foreignKeysQuery, []any{table}, func(rows *sql.Rows) error {
		var (
			id       int64
			parent   string
			from     string
			to       sql.NullString
			onUpdate string
			onDelete string
		)

		if err := rows.Scan(&id, &parent, &from, &to, &onUpdate, &onDelete); err != nil {
			return err
		}

		index, seen := byID[id]
		if !seen {
			keys = append(keys, dbschema.ForeignKey{
				ReferencedTable: parent,
				OnDelete:        referentialAction(onDelete),
				OnUpdate:        referentialAction(onUpdate),
			})
			index = len(keys) - 1
			byID[id] = index
		}

		keys[index].Columns = append(keys[index].Columns, from)

		// A NULL parent column means the key references the parent's primary key
		// implicitly. The document pairs local and parent columns positionally,
		// so an unnamed parent column is left out rather than padded with an
		// empty string that would pair wrongly.
		if to.Valid {
			keys[index].ReferencedColumns = append(keys[index].ReferencedColumns, to.String)
		}

		return nil
	})
	if err != nil {
		return nil, err
	}

	return keys, nil
}

// indexSet is the indexes of a table together with the constraints they back,
// returned as one value because SQLite reports both from the same pragma.
type indexSet struct {
	indexes     []dbschema.Index
	constraints []dbschema.Constraint
}

// collectIndexes reads a table's indexes and derives its unique and primary-key
// constraints from them, which is the only place SQLite records them.
func (a *Adapter) collectIndexes(
	ctx context.Context,
	table string,
	primaryKey *dbschema.PrimaryKey,
) (indexSet, error) {
	var (
		set     indexSet
		listing []indexListing
	)

	err := a.eachRow(ctx, indexListQuery, []any{table}, func(rows *sql.Rows) error {
		var entry indexListing
		if err := rows.Scan(&entry.name, &entry.unique, &entry.origin); err != nil {
			return err
		}

		listing = append(listing, entry)

		return nil
	})
	if err != nil {
		return indexSet{}, err
	}

	if primaryKey != nil {
		set.constraints = append(set.constraints, dbschema.Constraint{
			Name:           primaryKeyConstraintName(primaryKey, table),
			TableName:      table,
			ConstraintType: dbschema.ConstraintPrimaryKey,
			Columns:        primaryKey.Columns,
		})
	}

	for _, entry := range listing {
		columns, err := a.indexColumns(ctx, entry.name)
		if err != nil {
			return indexSet{}, err
		}

		set.indexes = append(set.indexes, dbschema.Index{
			Name:      entry.name,
			TableName: table,
			Columns:   columns,
			Unique:    entry.unique != 0,
			Primary:   entry.origin == originPrimaryKey,
		})

		// origin "u" is a UNIQUE clause in the table definition; "c" is a
		// CREATE INDEX statement, which is an index and not a constraint.
		if entry.origin == originUniqueConstraint {
			set.constraints = append(set.constraints, dbschema.Constraint{
				Name:           entry.name,
				TableName:      table,
				ConstraintType: dbschema.ConstraintUnique,
				Columns:        columnNames(columns),
			})
		}
	}

	return set, nil
}

// The values PRAGMA index_list reports in its origin column.
const (
	originPrimaryKey       = "pk"
	originUniqueConstraint = "u"
)

// indexListing is one row of PRAGMA index_list.
type indexListing struct {
	name   string
	unique int64
	origin string
}

// indexColumns reads the columns of one index in key order.
func (a *Adapter) indexColumns(ctx context.Context, index string) ([]dbschema.IndexColumn, error) {
	var columns []dbschema.IndexColumn

	err := a.eachRow(ctx, indexColumnsQuery, []any{index}, func(rows *sql.Rows) error {
		var name sql.NullString
		if err := rows.Scan(&name); err != nil {
			return err
		}

		// An index over an expression rather than a column reports a NULL name.
		// There is no column to record, so the entry is skipped; the index still
		// appears, with the columns it does name.
		if name.Valid {
			columns = append(columns, dbschema.IndexColumn{Name: name.String})
		}

		return nil
	})
	if err != nil {
		return nil, err
	}

	return columns, nil
}

// rowCount estimates a table's row count from the maximum rowid, which is an
// index seek rather than a scan.
//
// The maximum is NULL for an empty table and for one whose rows were all
// deleted, so it is scanned as a nullable value and reported as zero. Scanning
// it into a plain integer would fail on every empty table in the database.
func (a *Adapter) rowCount(ctx context.Context, table string) (uint64, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	statement := `SELECT MAX(rowid) FROM ` + quoteIdentifier(table)

	var maximum sql.NullInt64
	if err := a.db.QueryRowContext(queryCtx, statement).Scan(&maximum); err != nil {
		return 0, err
	}

	if !maximum.Valid || maximum.Int64 < 0 {
		return 0, nil
	}

	return uint64(maximum.Int64), nil
}

// collectViews reads every view, with its definition and columns.
func (a *Adapter) collectViews(ctx context.Context, schema *dbschema.Schema) error {
	type listing struct {
		name       string
		definition sql.NullString
	}

	var views []listing

	err := a.eachRow(ctx, viewsQuery, nil, func(rows *sql.Rows) error {
		var entry listing
		if err := rows.Scan(&entry.name, &entry.definition); err != nil {
			return err
		}

		views = append(views, entry)

		return nil
	})
	if err != nil {
		return fmt.Errorf("sqlite: list views: %w", err)
	}

	for _, entry := range views {
		view := dbschema.View{Name: entry.name}
		if entry.definition.Valid {
			view.Definition = &entry.definition.String
		}

		// A view is resolved, not declared: SQLite reports its columns by
		// planning the SELECT, so a view over a dropped table fails here even
		// though sqlite_master still lists it. That is a property of the one
		// view, not a failure of the survey, so it is recorded and the view is
		// kept without columns -- the same demotion collectTable applies to a
		// missing row estimate. Aborting would mean one dangling view denies an
		// operator the whole schema.
		shape, err := a.collectColumns(ctx, entry.name)
		if err != nil {
			schema.AddWarning(fmt.Sprintf("no columns for view %q: %v", entry.name, err))
		} else {
			view.Columns = shape.columns
		}

		schema.Views = append(schema.Views, view)
	}

	return nil
}

// collectTriggers reads every trigger.
func (a *Adapter) collectTriggers(ctx context.Context, schema *dbschema.Schema) error {
	err := a.eachRow(ctx, triggersQuery, nil, func(rows *sql.Rows) error {
		var (
			name       string
			table      string
			definition sql.NullString
		)

		if err := rows.Scan(&name, &table, &definition); err != nil {
			return err
		}

		trigger := dbschema.Trigger{
			Name:      name,
			TableName: table,
			Event:     dbschema.TriggerInsert,
			Timing:    dbschema.TimingBefore,
		}

		if definition.Valid {
			trigger.Definition = &definition.String
			trigger.Timing, trigger.Event = parseTriggerHeader(definition.String)
		}

		schema.Triggers = append(schema.Triggers, trigger)

		return nil
	})
	if err != nil {
		return fmt.Errorf("sqlite: list triggers: %w", err)
	}

	return nil
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

// assemblePrimaryKey orders the key parts by their declared position.
func assemblePrimaryKey(parts []keyPart) *dbschema.PrimaryKey {
	if len(parts) == 0 {
		return nil
	}

	columns := make([]string, 0, len(parts))

	// The parts arrive in column order, not key order, and a compound key can
	// declare its columns in any order relative to the table.
	for position := int64(1); position <= int64(len(parts)); position++ {
		for _, part := range parts {
			if part.position == position {
				columns = append(columns, part.column)
			}
		}
	}

	if len(columns) != len(parts) {
		// A gap in the reported positions means the pragma said something this
		// code does not model. Falling back to column order keeps every column
		// rather than dropping the ones that did not line up.
		columns = columns[:0]
		for _, part := range parts {
			columns = append(columns, part.column)
		}
	}

	return &dbschema.PrimaryKey{Columns: columns}
}

// primaryKeyConstraintName names the primary-key constraint, which SQLite does
// not name itself.
func primaryKeyConstraintName(key *dbschema.PrimaryKey, table string) string {
	if key.Name != nil {
		return *key.Name
	}

	return table + "_pkey"
}

// columnNames reduces index columns to their names.
func columnNames(columns []dbschema.IndexColumn) []string {
	names := make([]string, 0, len(columns))
	for _, column := range columns {
		names = append(names, column.Name)
	}

	return names
}

// referentialAction maps the action text the pragma reports.
//
// SQLite reports "NO ACTION" where the table declared nothing, so unlike engines
// that report an empty action this one never yields nil.
func referentialAction(action string) *dbschema.ReferentialAction {
	mapped, ok := referentialActions[strings.ToUpper(strings.TrimSpace(action))]
	if !ok {
		return nil
	}

	return &mapped
}

var referentialActions = map[string]dbschema.ReferentialAction{
	"CASCADE":     dbschema.Cascade,
	"SET NULL":    dbschema.SetNull,
	"SET DEFAULT": dbschema.SetDefault,
	"RESTRICT":    dbschema.Restrict,
	"NO ACTION":   dbschema.NoAction,
}

// parseTriggerHeader reads the timing and event out of a CREATE TRIGGER
// statement, defaulting to the grammar's own default of BEFORE.
func parseTriggerHeader(definition string) (dbschema.TriggerTiming, dbschema.TriggerEvent) {
	match := triggerHeaderPattern.FindStringSubmatch(definition)
	if match == nil {
		return dbschema.TimingBefore, dbschema.TriggerInsert
	}

	timing := dbschema.TimingBefore

	switch strings.ToUpper(strings.Join(strings.Fields(match[1]), " ")) {
	case "AFTER":
		timing = dbschema.TimingAfter
	case "INSTEAD OF":
		timing = dbschema.TimingInsteadOf
	}

	event := dbschema.TriggerInsert

	switch strings.ToUpper(match[2]) {
	case "UPDATE":
		event = dbschema.TriggerUpdate
	case "DELETE":
		event = dbschema.TriggerDelete
	}

	return timing, event
}
