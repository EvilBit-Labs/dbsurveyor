package postgres

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5"
	"golang.org/x/sync/errgroup"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// tableKey identifies a table within a survey. PostgreSQL namespaces tables by
// schema, so the name alone is not a key.
type tableKey struct {
	schema string
	table  string
}

// metadata is the per-table catalog data both collection paths produce.
//
// The two paths write the same structure through the same scan functions, which
// is what makes "the batch and the fallback produce identical schemas" a
// property of the code rather than a hope: there is only one place each row
// shape is turned into a document object.
type metadata struct {
	columns     map[tableKey][]dbschema.Column
	primaryKeys map[tableKey]*dbschema.PrimaryKey
	foreignKeys map[tableKey][]dbschema.ForeignKey
	indexes     map[tableKey][]dbschema.Index
	constraints map[tableKey][]dbschema.Constraint
}

// newMetadata returns an empty metadata set.
func newMetadata() *metadata {
	return &metadata{
		columns:     map[tableKey][]dbschema.Column{},
		primaryKeys: map[tableKey]*dbschema.PrimaryKey{},
		foreignKeys: map[tableKey][]dbschema.ForeignKey{},
		indexes:     map[tableKey][]dbschema.Index{},
		constraints: map[tableKey][]dbschema.Constraint{},
	}
}

// collector pairs a query template with the function that reads its rows.
//
// Naming each one matters for the fallback: when the batch fails, the operator
// is told which of the five queries failed, not merely that "collection fell
// back".
type collector struct {
	name     string
	template string
	scan     func(pgx.Rows, *metadata) error
}

// collectors are the five per-table metadata queries.
//
// Five queries covering every table replace five queries per table. On a
// database of a few hundred tables that is the difference between six round
// trips and well over a thousand, which is the whole runtime of a survey.
var collectors = []collector{
	{name: "columns", template: columnsTemplate, scan: scanColumns},
	{name: "keys", template: keysTemplate, scan: scanKeys},
	{name: "foreign keys", template: foreignKeysTemplate, scan: scanForeignKeys},
	{name: "indexes", template: indexesTemplate, scan: scanIndexes},
	{name: "check constraints", template: checksTemplate, scan: scanChecks},
}

// collectMetadata reads per-table metadata, preferring the batch path and
// falling back to per-table queries when it fails.
//
// The distinction the fallback rests on is between a query that failed and a
// query that returned nothing. An empty result is a legitimate answer -- a
// database with no foreign keys anywhere -- and treating it as a failure would
// make every simple schema take the slow path. Only an error triggers the
// fallback, and the error is reported rather than swallowed: a survey that
// quietly degraded would leave nobody able to explain why it took an hour.
func (a *Adapter) collectMetadata(
	ctx context.Context,
	schemas []string,
	tables []tableKey,
	onFallback func(string),
) (*metadata, error) {
	return chooseMetadata(
		func() (*metadata, error) { return a.batchMetadata(ctx, schemas) },
		func() (*metadata, error) { return a.perTableMetadata(ctx, tables) },
		onFallback,
	)
}

// chooseMetadata runs batch, and on failure reports why and runs fallback.
//
// It is separated from the queries so the choice can be tested without a
// server. The property it encodes is small and easy to get backwards: an error
// is not an empty result. A batch that returns no rows has answered the
// question, and taking that for a failure would send every simple schema down
// the slow path; a batch that errors has not answered it, and taking that for an
// empty result would produce a document reporting a database with no columns in
// it.
func chooseMetadata(
	batch, fallback func() (*metadata, error),
	onFallback func(string),
) (*metadata, error) {
	batched, err := batch()
	if err == nil {
		return batched, nil
	}

	onFallback(fmt.Sprintf(
		"batch metadata collection failed (%v); falling back to per-table queries", err))

	return fallback()
}

// batchMetadata runs the five metadata queries concurrently, each covering every
// schema in the survey.
//
// They run concurrently because they are independent reads that contend only for
// connections, and the pool already bounds how many can be in flight. The first
// failure cancels the rest through the group's context: once the batch is going
// to be discarded for the fallback, finishing the other four queries is work
// nobody will look at.
func (a *Adapter) batchMetadata(ctx context.Context, schemas []string) (*metadata, error) {
	collected := newMetadata()

	group, groupCtx := errgroup.WithContext(ctx)

	for _, each := range collectors {
		group.Go(func() error {
			if err := a.runCollector(groupCtx, each, schemaPredicate, []any{schemas}, collected); err != nil {
				return fmt.Errorf("%s: %w", each.name, err)
			}

			return nil
		})
	}

	if err := group.Wait(); err != nil {
		return nil, err
	}

	return collected, nil
}

// perTableMetadata runs the same five queries once per table.
//
// It is sequential where the batch is concurrent. The fallback exists because
// something already went wrong with the batch -- most often a permission or a
// resource limit -- and answering that by opening as many connections as
// possible is the wrong move.
func (a *Adapter) perTableMetadata(ctx context.Context, tables []tableKey) (*metadata, error) {
	collected := newMetadata()

	for _, table := range tables {
		for _, each := range collectors {
			args := []any{table.schema, table.table}

			if err := a.runCollector(ctx, each, tablePredicate, args, collected); err != nil {
				return nil, fmt.Errorf("postgres: %s of %s.%s: %w",
					each.name, table.schema, table.table, err)
			}
		}
	}

	return collected, nil
}

// runCollector executes one collector against one predicate and folds its rows
// into collected.
func (a *Adapter) runCollector(
	ctx context.Context,
	each collector,
	predicate string,
	args []any,
	collected *metadata,
) error {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	rows, err := a.pool.Query(queryCtx, withPredicate(each.template, predicate), args...)
	if err != nil {
		return err
	}

	defer rows.Close()

	if err := each.scan(rows, collected); err != nil {
		return err
	}

	return rows.Err()
}

// scanColumns reads the columns query.
//
// The ordinal position is assigned by counting rather than taken from attnum.
// A dropped column leaves its attribute number permanently unused, so a table
// that has been altered reports attnum 1, 2, 4 -- and the document requires
// consecutive one-based positions. The rows arrive ordered by attnum, so
// counting reproduces the declaration order without the gaps.
func scanColumns(rows pgx.Rows, collected *metadata) error {
	for rows.Next() {
		var (
			key      tableKey
			name     string
			typeName string
			fullType string
			nullable bool
			dflt     *string
			identity bool
			comment  *string
		)

		err := rows.Scan(&key.schema, &key.table, &name, &typeName, &fullType,
			&nullable, &dflt, &identity, &comment)
		if err != nil {
			return err
		}

		column := dbschema.Column{
			Name:     name,
			DataType: mapDataType(typeName, fullType),
			Nullable: nullable,
			// A serial column is an integer with a nextval default rather than a
			// distinct type, so both spellings of "the engine fills this in" have
			// to be recognized.
			AutoGenerate: identity || (dflt != nil && hasSequenceDefault(*dflt)),
			Default:      dflt,
			Comment:      comment,
			//nolint:gosec // G115: bounded by the column count of one table.
			OrdinalPosition: uint32(len(collected.columns[key])) + 1,
		}

		collected.columns[key] = append(collected.columns[key], column)
	}

	return nil
}

// scanKeys reads the primary-key and unique constraint query.
func scanKeys(rows pgx.Rows, collected *metadata) error {
	for rows.Next() {
		var (
			key            tableKey
			name           string
			constraintType string
			columns        []string
		)

		if err := rows.Scan(&key.schema, &key.table, &name, &constraintType, &columns); err != nil {
			return err
		}

		kind := dbschema.ConstraintUnique
		if constraintType == primaryKeyType {
			kind = dbschema.ConstraintPrimaryKey
			constraintName := name
			collected.primaryKeys[key] = &dbschema.PrimaryKey{Name: &constraintName, Columns: columns}
		}

		collected.constraints[key] = append(collected.constraints[key], dbschema.Constraint{
			Name:           name,
			TableName:      key.table,
			Schema:         schemaPointer(key.schema),
			ConstraintType: kind,
			Columns:        columns,
		})
	}

	return nil
}

// scanForeignKeys reads the foreign key query.
func scanForeignKeys(rows pgx.Rows, collected *metadata) error {
	for rows.Next() {
		var (
			key          tableKey
			name         string
			columns      []string
			parentSchema string
			parentTable  string
			parentCols   []string
			onUpdate     string
			onDelete     string
		)

		err := rows.Scan(&key.schema, &key.table, &name, &columns,
			&parentSchema, &parentTable, &parentCols, &onUpdate, &onDelete)
		if err != nil {
			return err
		}

		constraintName := name
		foreignKey := dbschema.ForeignKey{
			Name:              &constraintName,
			Columns:           columns,
			ReferencedTable:   parentTable,
			ReferencedColumns: parentCols,
			OnDelete:          referentialAction(onDelete),
			OnUpdate:          referentialAction(onUpdate),
		}

		// The parent schema is recorded only when it differs, so an ordinary
		// same-schema key does not carry a redundant qualifier into every
		// document.
		if parentSchema != key.schema {
			foreignKey.ReferencedSchema = &parentSchema
		}

		collected.foreignKeys[key] = append(collected.foreignKeys[key], foreignKey)
		collected.constraints[key] = append(collected.constraints[key], dbschema.Constraint{
			Name:           name,
			TableName:      key.table,
			Schema:         schemaPointer(key.schema),
			ConstraintType: dbschema.ConstraintForeignKey,
			Columns:        columns,
		})
	}

	return nil
}

// scanChecks reads the check constraint query.
func scanChecks(rows pgx.Rows, collected *metadata) error {
	for rows.Next() {
		var (
			key     tableKey
			name    string
			clause  string
			columns []string
		)

		if err := rows.Scan(&key.schema, &key.table, &name, &clause, &columns); err != nil {
			return err
		}

		definition := clause
		collected.constraints[key] = append(collected.constraints[key], dbschema.Constraint{
			Name:           name,
			TableName:      key.table,
			Schema:         schemaPointer(key.schema),
			ConstraintType: dbschema.ConstraintCheck,
			Columns:        columns,
			CheckClause:    &definition,
		})
	}

	return nil
}

// scanIndexes reads the index query.
func scanIndexes(rows pgx.Rows, collected *metadata) error {
	for rows.Next() {
		var (
			key        tableKey
			name       string
			unique     bool
			primary    bool
			method     string
			keyColumns []string
		)

		err := rows.Scan(&key.schema, &key.table, &name, &unique, &primary, &method, &keyColumns)
		if err != nil {
			return err
		}

		columns := make([]dbschema.IndexColumn, 0, len(keyColumns))
		for _, column := range keyColumns {
			columns = append(columns, indexColumn(column))
		}

		accessMethod := method
		collected.indexes[key] = append(collected.indexes[key], dbschema.Index{
			Name:      name,
			TableName: key.table,
			Schema:    schemaPointer(key.schema),
			Columns:   columns,
			Unique:    unique,
			Primary:   primary,
			IndexType: &accessMethod,
		})
	}

	return nil
}

// primaryKeyType is the contype value pg_constraint gives a primary key.
const primaryKeyType = "p"

// schemaPointer returns a pointer to a copy of the schema name, so that every
// object carrying one does not alias the same variable.
func schemaPointer(schema string) *string {
	name := schema

	return &name
}
