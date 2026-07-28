package postgres

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// CollectSchema reads the structure of the configured database.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.pool == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("postgres: invalid collection configuration: %w", err)
	}

	started := time.Now()

	schemas, err := a.schemas(ctx)
	if err != nil {
		return nil, err
	}

	document := dbschema.New(a.databaseInfo(ctx), cfg.CollectorVersion, started)

	if err := a.collectTables(ctx, document, schemas, cfg); err != nil {
		return nil, err
	}

	if err := a.collectOptionalObjects(ctx, document, schemas, cfg); err != nil {
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
	schemas []string,
	cfg dbadapter.CollectionConfig,
) error {
	if cfg.IncludeViews {
		if err := a.collectViews(ctx, document, schemas); err != nil {
			return err
		}
	}

	if cfg.IncludeRoutines {
		if err := a.collectRoutines(ctx, document, schemas); err != nil {
			return err
		}
	}

	if cfg.IncludeTriggers {
		if err := a.collectTriggers(ctx, document, schemas); err != nil {
			return err
		}
	}

	if cfg.IncludeTypes {
		if err := a.collectUserTypes(ctx, document, schemas); err != nil {
			return err
		}
	}

	return nil
}

// collectTables reads every table with its columns, keys, indexes, and
// constraints.
func (a *Adapter) collectTables(
	ctx context.Context,
	document *dbschema.Schema,
	schemas []string,
	cfg dbadapter.CollectionConfig,
) error {
	listing, err := a.readTables(ctx, schemas)
	if err != nil {
		return err
	}

	collected, err := a.collectMetadata(ctx, schemas, listing.keys, document.AddWarning)
	if err != nil {
		return err
	}

	for i := range listing.tables {
		table := &listing.tables[i]
		key := tableKey{schema: derefSchema(table.Schema), table: table.Name}

		table.Columns = collected.columns[key]
		table.PrimaryKey = collected.primaryKeys[key]
		table.ForeignKeys = collected.foreignKeys[key]

		if cfg.IncludeIndexes {
			table.Indexes = collected.indexes[key]
		}

		if cfg.IncludeConstraint {
			table.Constraints = collected.constraints[key]
		}

		if len(table.Columns) == 0 {
			document.AddWarning(fmt.Sprintf(
				"table %s.%s reported no columns; the credential may not be able to read it",
				key.schema, key.table))
		}
	}

	document.Tables = listing.tables

	return nil
}

// tableList is the tables of a survey together with their keys, returned as one
// value because both come from the same rows and the keys are what the metadata
// queries are scoped to.
type tableList struct {
	tables []dbschema.Table
	keys   []tableKey
}

// readTables reads the tables of the surveyed schemas.
func (a *Adapter) readTables(ctx context.Context, schemas []string) (tableList, error) {
	listing := tableList{tables: []dbschema.Table{}}

	err := a.eachRow(ctx, tablesQuery, []any{schemas}, func(rows pgx.Rows) error {
		var (
			key      tableKey
			estimate float64
			comment  *string
		)

		if err := rows.Scan(&key.schema, &key.table, &estimate, &comment); err != nil {
			return err
		}

		table := dbschema.Table{
			Name:        key.table,
			Schema:      schemaPointer(key.schema),
			Columns:     []dbschema.Column{},
			ForeignKeys: []dbschema.ForeignKey{},
			Indexes:     []dbschema.Index{},
			Constraints: []dbschema.Constraint{},
			Comment:     comment,
		}

		// reltuples is -1 on a table that has never been analyzed, and stale
		// whenever rows have been written since the last analyze. Both are
		// reported as an estimate of zero rather than as a negative count.
		table.RowCount = new(uint64)
		if estimate > 0 {
			*table.RowCount = uint64(estimate)
		}

		listing.tables = append(listing.tables, table)
		listing.keys = append(listing.keys, key)

		return nil
	})
	if err != nil {
		return tableList{}, fmt.Errorf("postgres: list tables of %q: %w", a.database, err)
	}

	return listing, nil
}

// schemas reports the namespaces a survey covers.
func (a *Adapter) schemas(ctx context.Context) ([]string, error) {
	if len(a.searchSchemas) > 0 {
		return a.searchSchemas, nil
	}

	var names []string

	err := a.eachRow(ctx, schemasQuery, nil, func(rows pgx.Rows) error {
		var name string
		if err := rows.Scan(&name); err != nil {
			return err
		}

		names = append(names, name)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("postgres: list schemas of %q: %w", a.database, err)
	}

	return names, nil
}

// databaseInfo describes the database being surveyed.
//
// Every field beyond the name is optional. A managed server can refuse
// pg_database_size or pg_get_userbyid to an ordinary role, and a survey that
// failed because it could not read the database's size would be refusing to do
// the thing it was asked for over a detail nobody needs.
func (a *Adapter) databaseInfo(ctx context.Context) dbschema.DatabaseInfo {
	info := dbschema.NewDatabaseInfo(a.database, dbschema.PostgreSQL)

	if version, err := a.version(ctx); err == nil {
		info.Version = &version
	}

	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var (
		encoding  *string
		collation *string
		owner     *string
		size      *int64
		template  bool
	)

	err := a.pool.QueryRow(queryCtx, databaseInfoQuery).
		Scan(&encoding, &collation, &owner, &size, &template)
	if err != nil {
		return info
	}

	info.Encoding = encoding
	info.Collation = collation
	info.Owner = owner
	info.SystemDatabase = template || isSystemDatabase(a.database)

	if size != nil && *size >= 0 {
		bytes := uint64(*size)
		info.SizeBytes = &bytes
	}

	return info
}

// version reads the server version string.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var version string
	if err := a.pool.QueryRow(queryCtx, versionQuery).Scan(&version); err != nil {
		return "", fmt.Errorf("postgres: read server version: %w", err)
	}

	return version, nil
}

// eachRow runs a query under the configured per-query timeout and hands every
// row to scan.
//
// The rows are consumed inside this function rather than returned so that the
// timeout's cancel runs after the last row is read. A helper that returned
// pgx.Rows would have to leak the cancel to its caller, and a caller that forgot
// would cancel the context while the rows were still being read.
func (a *Adapter) eachRow(
	ctx context.Context,
	statement string,
	args []any,
	scan func(pgx.Rows) error,
) error {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	rows, err := a.pool.Query(queryCtx, statement, args...)
	if err != nil {
		return err
	}

	defer rows.Close()

	for rows.Next() {
		if err := scan(rows); err != nil {
			return err
		}
	}

	return rows.Err()
}

// hasSequenceDefault reports whether a column default draws from a sequence,
// which is how a serial column differs from a plain integer.
func hasSequenceDefault(expression string) bool {
	return strings.HasPrefix(strings.TrimSpace(strings.ToLower(expression)), "nextval(")
}

// indexColumn turns one rendered index key element into a document column.
//
// pg_get_indexdef renders an ordinary column as a quoted or bare identifier and
// a functional index as the expression, in both cases followed by any
// non-default operator class and sort options. The trailing DESC is the part the
// document models; everything else is kept in the name as written, because an
// index on lower(email) is a fact a reader needs and inventing a column name for
// it would be worse than reporting the expression.
func indexColumn(rendered string) dbschema.IndexColumn {
	name := strings.TrimSpace(rendered)
	direction := dbschema.Ascending

	if trimmed, descending := strings.CutSuffix(name, " DESC"); descending {
		name = strings.TrimSpace(trimmed)
		direction = dbschema.Descending
	} else {
		name = strings.TrimSpace(strings.TrimSuffix(name, " ASC"))
	}

	name = strings.TrimSuffix(name, " NULLS FIRST")
	name = strings.TrimSuffix(name, " NULLS LAST")
	name = strings.TrimSpace(name)

	if unquoted, quoted := strings.CutPrefix(name, `"`); quoted {
		if closed, ok := strings.CutSuffix(unquoted, `"`); ok {
			name = strings.ReplaceAll(closed, `""`, `"`)
		}
	}

	return dbschema.IndexColumn{Name: name, SortOrder: &direction}
}

// referentialAction maps the single-character action code pg_constraint reports.
//
// PostgreSQL writes 'a' where a foreign key declared no action, which is not the
// same as an explicit NO ACTION in the other engines' catalogs but means the
// same thing, so it is reported as NoAction rather than as absent.
func referentialAction(code string) *dbschema.ReferentialAction {
	action, ok := referentialActions[code]
	if !ok {
		return nil
	}

	return &action
}

var referentialActions = map[string]dbschema.ReferentialAction{
	"a": dbschema.NoAction,
	"r": dbschema.Restrict,
	"c": dbschema.Cascade,
	"n": dbschema.SetNull,
	"d": dbschema.SetDefault,
}

// derefSchema reads an optional schema name, treating absence as the empty
// namespace.
func derefSchema(schema *string) string {
	if schema == nil {
		return ""
	}

	return *schema
}

// systemDatabases are the databases the server provides. They are the same on
// every PostgreSQL server, so including them in a multi-database survey drowns
// the databases an operator is actually looking at.
var systemDatabases = map[string]struct{}{
	"postgres":  {},
	"template0": {},
	"template1": {},
}

// isSystemDatabase reports whether a database is one the server provides.
func isSystemDatabase(name string) bool {
	_, system := systemDatabases[strings.ToLower(name)]

	return system
}
