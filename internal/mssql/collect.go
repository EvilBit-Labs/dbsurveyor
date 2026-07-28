package mssql

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
// They read the sys.* catalog views rather than INFORMATION_SCHEMA. The catalog
// views carry what the document needs and the standard views do not: an identity
// flag, a maintained row count, an index's access method, and a referential
// action already spelled out rather than encoded.
//
// Each covers the whole database at once and is grouped in memory, so collection
// is a fixed number of round trips rather than a multiple of the table count.
const (
	versionQuery = `SELECT @@VERSION, DB_NAME()`

	tablesQuery = `
SELECT s.name, t.name,
       (SELECT SUM(p.rows) FROM sys.partitions AS p
         WHERE p.object_id = t.object_id AND p.index_id IN (0, 1)),
       CAST(ep.value AS nvarchar(max))
FROM sys.tables AS t
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
LEFT JOIN sys.extended_properties AS ep
  ON ep.major_id = t.object_id AND ep.minor_id = 0 AND ep.name = 'MS_Description'
ORDER BY s.name, t.name`

	columnsQuery = `
SELECT s.name, o.name, c.name, ty.name, c.max_length, c.precision, c.scale,
       c.is_nullable, c.is_identity, dc.definition, CAST(ep.value AS nvarchar(max))
FROM sys.columns AS c
JOIN sys.objects AS o ON o.object_id = c.object_id
JOIN sys.schemas AS s ON s.schema_id = o.schema_id
JOIN sys.types AS ty ON ty.user_type_id = c.user_type_id
LEFT JOIN sys.default_constraints AS dc ON dc.object_id = c.default_object_id
LEFT JOIN sys.extended_properties AS ep
  ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description'
WHERE o.type IN ('U', 'V')
ORDER BY s.name, o.name, c.column_id`

	// is_included_column is excluded because an included column is payload
	// carried in the leaf pages, not part of the key, and reporting it as a key
	// column would misstate which prefixes the index can serve.
	indexesQuery = `
SELECT s.name, t.name, i.name, i.is_unique, i.is_primary_key,
       i.is_unique_constraint, i.type_desc, c.name, ic.is_descending_key
FROM sys.indexes AS i
JOIN sys.tables AS t ON t.object_id = i.object_id
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
JOIN sys.index_columns AS ic
  ON ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0
JOIN sys.columns AS c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
WHERE i.name IS NOT NULL
ORDER BY s.name, t.name, i.name, ic.key_ordinal`

	foreignKeysQuery = `
SELECT s.name, t.name, fk.name, pc.name, ps.name, pt.name, rc.name,
       fk.delete_referential_action_desc, fk.update_referential_action_desc
FROM sys.foreign_keys AS fk
JOIN sys.tables AS t ON t.object_id = fk.parent_object_id
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
JOIN sys.tables AS pt ON pt.object_id = fk.referenced_object_id
JOIN sys.schemas AS ps ON ps.schema_id = pt.schema_id
JOIN sys.foreign_key_columns AS fkc ON fkc.constraint_object_id = fk.object_id
JOIN sys.columns AS pc
  ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id
JOIN sys.columns AS rc
  ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id
ORDER BY s.name, t.name, fk.name, fkc.constraint_column_id`

	checksQuery = `
SELECT s.name, t.name, cc.name, cc.definition, c.name
FROM sys.check_constraints AS cc
JOIN sys.tables AS t ON t.object_id = cc.parent_object_id
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
LEFT JOIN sys.columns AS c
  ON c.object_id = cc.parent_object_id AND c.column_id = cc.parent_column_id
ORDER BY s.name, t.name, cc.name`

	viewsQuery = `
SELECT s.name, v.name, OBJECT_DEFINITION(v.object_id)
FROM sys.views AS v
JOIN sys.schemas AS s ON s.schema_id = v.schema_id
ORDER BY s.name, v.name`

	// P is a stored procedure; FN, IF, and TF are the scalar, inline, and
	// table-valued function kinds.
	routinesQuery = `
SELECT s.name, o.name, o.type
FROM sys.objects AS o
JOIN sys.schemas AS s ON s.schema_id = o.schema_id
WHERE o.type IN ('P', 'FN', 'IF', 'TF')
ORDER BY s.name, o.name`

	triggersQuery = `
SELECT s.name, t.name, tr.name, te.type_desc, tr.is_instead_of_trigger
FROM sys.triggers AS tr
JOIN sys.tables AS t ON t.object_id = tr.parent_id
JOIN sys.schemas AS s ON s.schema_id = t.schema_id
JOIN sys.trigger_events AS te ON te.object_id = tr.object_id
WHERE tr.is_ms_shipped = 0
ORDER BY s.name, t.name, tr.name`

	userTypesQuery = `
SELECT s.name, ty.name, base.name
FROM sys.types AS ty
JOIN sys.schemas AS s ON s.schema_id = ty.schema_id
LEFT JOIN sys.types AS base ON base.user_type_id = ty.system_type_id
WHERE ty.is_user_defined = 1
ORDER BY s.name, ty.name`

	samplingColumnsQuery = `
SELECT c.name, c.is_identity, COALESCE(i.is_primary_key, 0)
FROM sys.columns AS c
JOIN sys.objects AS o ON o.object_id = c.object_id
JOIN sys.schemas AS s ON s.schema_id = o.schema_id
LEFT JOIN sys.index_columns AS ic
  ON ic.object_id = c.object_id AND ic.column_id = c.column_id
LEFT JOIN sys.indexes AS i
  ON i.object_id = ic.object_id AND i.index_id = ic.index_id AND i.is_primary_key = 1
WHERE s.name = @p1 AND o.name = @p2
ORDER BY c.column_id`
)

// tableKey identifies a table within a survey. SQL Server namespaces tables by
// schema, so the name alone is not a key.
type tableKey struct {
	schema string
	table  string
}

// CollectSchema reads the structure of the configured database.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mssql: invalid collection configuration: %w", err)
	}

	started := time.Now()

	info := dbschema.NewDatabaseInfo(a.database, dbschema.SQLServer)
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

// version reads the server version banner and confirms the database name.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var (
		banner   string
		database string
	)

	if err := a.db.QueryRowContext(queryCtx, versionQuery).Scan(&banner, &database); err != nil {
		return "", fmt.Errorf("mssql: read server version: %w", err)
	}

	// The banner is multi-line; the first line carries the edition and build.
	first, _, _ := strings.Cut(banner, "\n")

	return strings.TrimSpace(first), nil
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

	indexed, err := a.readIndexes(ctx)
	if err != nil {
		return err
	}

	foreignKeys, err := a.readForeignKeys(ctx)
	if err != nil {
		return err
	}

	checks, err := a.readChecks(ctx)
	if err != nil {
		return err
	}

	for i := range tables {
		table := &tables[i]
		key := tableKey{schema: derefSchema(table.Schema), table: table.Name}

		table.Columns = columns[key]
		table.PrimaryKey = indexed[key].primaryKey
		table.ForeignKeys = foreignKeys[key].keys

		if cfg.IncludeIndexes {
			table.Indexes = indexed[key].indexes
		}

		if cfg.IncludeConstraint {
			table.Constraints = append(append(append(
				[]dbschema.Constraint{}, indexed[key].constraints...),
				foreignKeys[key].constraints...),
				checks[key]...)
		}

		if len(table.Columns) == 0 {
			document.AddWarning(fmt.Sprintf(
				"table %s.%s reported no columns; the credential may not be able to read it",
				key.schema, key.table))
		}
	}

	document.Tables = tables

	return nil
}

// readTables reads the tables of the database.
func (a *Adapter) readTables(ctx context.Context) ([]dbschema.Table, error) {
	tables := []dbschema.Table{}

	err := a.eachRow(ctx, tablesQuery, nil, func(rows *sql.Rows) error {
		var (
			key      tableKey
			estimate sql.NullInt64
			comment  sql.NullString
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
		}

		// The partition sum is NULL for a table with no heap or clustered index
		// rows recorded yet, so it is scanned as nullable and reported as zero.
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
		return nil, fmt.Errorf("mssql: list tables of %q: %w", a.database, err)
	}

	return tables, nil
}

// readColumns reads every table and view column, grouped by object.
//
// The ordinal position is assigned by counting rather than taken from
// column_id, which a dropped column leaves a gap in.
func (a *Adapter) readColumns(ctx context.Context) (map[tableKey][]dbschema.Column, error) {
	grouped := map[tableKey][]dbschema.Column{}

	err := a.eachRow(ctx, columnsQuery, nil, func(rows *sql.Rows) error {
		var (
			key        tableKey
			name       string
			declared   columnType
			nullable   bool
			identity   bool
			definition sql.NullString
			comment    sql.NullString
		)

		err := rows.Scan(&key.schema, &key.table, &name, &declared.Name, &declared.MaxLength,
			&declared.Precision, &declared.Scale, &nullable, &identity, &definition, &comment)
		if err != nil {
			return err
		}

		column := dbschema.Column{
			Name:         name,
			DataType:     mapDataType(declared),
			Nullable:     nullable,
			AutoGenerate: identity,
			//nolint:gosec // G115: bounded by one object's column count.
			OrdinalPosition: uint32(len(grouped[key])) + 1,
		}

		if definition.Valid {
			column.Default = &definition.String
		}

		if comment.Valid && comment.String != "" {
			column.Comment = &comment.String
		}

		grouped[key] = append(grouped[key], column)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mssql: read columns of %q: %w", a.database, err)
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

// schemaPointer returns a pointer to a copy of the schema name, so that every
// object carrying one does not alias the same variable.
func schemaPointer(schema string) *string {
	name := schema

	return &name
}

// derefSchema reads an optional schema name, treating absence as the empty
// namespace.
func derefSchema(schema *string) string {
	if schema == nil {
		return ""
	}

	return *schema
}
