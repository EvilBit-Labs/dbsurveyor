package mssql

import (
	"context"
	"database/sql"
	"fmt"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// indexSet is what sys.indexes says about one table: its indexes, the primary
// key one of them backs, and the constraints the unique ones represent.
type indexSet struct {
	indexes     []dbschema.Index
	primaryKey  *dbschema.PrimaryKey
	constraints []dbschema.Constraint
}

// foreignKeySet is what sys.foreign_keys says about one table.
type foreignKeySet struct {
	keys        []dbschema.ForeignKey
	constraints []dbschema.Constraint
}

// readIndexes reads every index in the database, grouped by table, and derives
// the primary-key and unique constraints from them.
//
// SQL Server records a primary key and a unique constraint as indexes with a
// flag rather than as separate catalog entries, so this one query is the only
// place either is reported.
func (a *Adapter) readIndexes(ctx context.Context) (map[tableKey]indexSet, error) {
	grouped := map[tableKey]indexSet{}
	position := map[string]int{}

	err := a.eachRow(ctx, indexesQuery, nil, func(rows *sql.Rows) error {
		var (
			key        tableKey
			name       string
			unique     bool
			primary    bool
			constraint bool
			method     string
			column     string
			descending bool
		)

		err := rows.Scan(&key.schema, &key.table, &name, &unique, &primary,
			&constraint, &method, &column, &descending)
		if err != nil {
			return err
		}

		set := grouped[key]
		lookup := key.schema + "\x00" + key.table + "\x00" + name

		at, seen := position[lookup]
		if !seen {
			accessMethod := method
			set.indexes = append(set.indexes, dbschema.Index{
				Name:      name,
				TableName: key.table,
				Schema:    schemaPointer(key.schema),
				Columns:   []dbschema.IndexColumn{},
				Unique:    unique,
				Primary:   primary,
				IndexType: &accessMethod,
			})
			at = len(set.indexes) - 1
			position[lookup] = at
		}

		direction := dbschema.Ascending
		if descending {
			direction = dbschema.Descending
		}

		set.indexes[at].Columns = append(set.indexes[at].Columns, dbschema.IndexColumn{
			Name:      column,
			SortOrder: &direction,
		})

		grouped[key] = set

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mssql: read indexes of %q: %w", a.database, err)
	}

	// The constraints are derived after every index is complete, because a
	// constraint's column list is the index's column list and that is only
	// final once the last row of the index has been read.
	for key, set := range grouped {
		grouped[key] = deriveKeyConstraints(key, set)
	}

	return grouped, nil
}

// deriveKeyConstraints turns the primary-key and unique indexes of a table into
// the primary key and the constraints the document records.
func deriveKeyConstraints(key tableKey, set indexSet) indexSet {
	for _, index := range set.indexes {
		if !index.Unique {
			continue
		}

		columns := columnNames(index.Columns)

		if index.Primary {
			name := index.Name
			set.primaryKey = &dbschema.PrimaryKey{Name: &name, Columns: columns}
			set.constraints = append(set.constraints, dbschema.Constraint{
				Name:           index.Name,
				TableName:      key.table,
				Schema:         schemaPointer(key.schema),
				ConstraintType: dbschema.ConstraintPrimaryKey,
				Columns:        columns,
			})

			continue
		}

		set.constraints = append(set.constraints, dbschema.Constraint{
			Name:           index.Name,
			TableName:      key.table,
			Schema:         schemaPointer(key.schema),
			ConstraintType: dbschema.ConstraintUnique,
			Columns:        columns,
		})
	}

	return set
}

// readForeignKeys reads every foreign key in the database, grouping the
// per-column rows the catalog returns back into one constraint per key.
func (a *Adapter) readForeignKeys(ctx context.Context) (map[tableKey]foreignKeySet, error) {
	grouped := map[tableKey]foreignKeySet{}
	position := map[string]int{}

	err := a.eachRow(ctx, foreignKeysQuery, nil, func(rows *sql.Rows) error {
		var (
			key          tableKey
			name         string
			column       string
			parentSchema string
			parentTable  string
			parentColumn string
			onDelete     string
			onUpdate     string
		)

		err := rows.Scan(&key.schema, &key.table, &name, &column,
			&parentSchema, &parentTable, &parentColumn, &onDelete, &onUpdate)
		if err != nil {
			return err
		}

		set := grouped[key]
		lookup := key.schema + "\x00" + key.table + "\x00" + name

		at, seen := position[lookup]
		if !seen {
			constraintName := name
			foreignKey := dbschema.ForeignKey{
				Name:            &constraintName,
				ReferencedTable: parentTable,
				OnDelete:        referentialAction(onDelete),
				OnUpdate:        referentialAction(onUpdate),
			}

			// The parent schema is recorded only when it differs, so an ordinary
			// same-schema key does not carry a redundant qualifier into every
			// document.
			if parentSchema != key.schema {
				parent := parentSchema
				foreignKey.ReferencedSchema = &parent
			}

			set.keys = append(set.keys, foreignKey)
			at = len(set.keys) - 1
			position[lookup] = at
		}

		set.keys[at].Columns = append(set.keys[at].Columns, column)
		set.keys[at].ReferencedColumns = append(set.keys[at].ReferencedColumns, parentColumn)

		grouped[key] = set

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mssql: read foreign keys of %q: %w", a.database, err)
	}

	for key, set := range grouped {
		for _, foreignKey := range set.keys {
			name := ""
			if foreignKey.Name != nil {
				name = *foreignKey.Name
			}

			set.constraints = append(set.constraints, dbschema.Constraint{
				Name:           name,
				TableName:      key.table,
				Schema:         schemaPointer(key.schema),
				ConstraintType: dbschema.ConstraintForeignKey,
				Columns:        foreignKey.Columns,
			})
		}

		grouped[key] = set
	}

	return grouped, nil
}

// readChecks reads every check constraint in the database, grouped by table.
func (a *Adapter) readChecks(ctx context.Context) (map[tableKey][]dbschema.Constraint, error) {
	grouped := map[tableKey][]dbschema.Constraint{}

	err := a.eachRow(ctx, checksQuery, nil, func(rows *sql.Rows) error {
		var (
			key        tableKey
			name       string
			definition string
			column     sql.NullString
		)

		if err := rows.Scan(&key.schema, &key.table, &name, &definition, &column); err != nil {
			return err
		}

		clause := definition
		constraint := dbschema.Constraint{
			Name:           name,
			TableName:      key.table,
			Schema:         schemaPointer(key.schema),
			ConstraintType: dbschema.ConstraintCheck,
			Columns:        []string{},
			CheckClause:    &clause,
		}

		// A table-level check names no single column; sys.check_constraints
		// reports parent_column_id 0 for one, which the join turns into a NULL.
		if column.Valid {
			constraint.Columns = []string{column.String}
		}

		grouped[key] = append(grouped[key], constraint)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mssql: read check constraints of %q: %w", a.database, err)
	}

	return grouped, nil
}

// columnNames reduces index columns to their names.
func columnNames(columns []dbschema.IndexColumn) []string {
	names := make([]string, 0, len(columns))
	for _, column := range columns {
		names = append(names, column.Name)
	}

	return names
}

// referentialAction maps the action description the catalog reports.
//
// SQL Server spells the action out rather than encoding it, and reports
// NO_ACTION where a key declared nothing, so nil here means the catalog said
// something this code does not model rather than that the engine default
// applies.
func referentialAction(action string) *dbschema.ReferentialAction {
	mapped, ok := referentialActions[action]
	if !ok {
		return nil
	}

	return &mapped
}

var referentialActions = map[string]dbschema.ReferentialAction{
	"NO_ACTION":   dbschema.NoAction,
	"CASCADE":     dbschema.Cascade,
	"SET_NULL":    dbschema.SetNull,
	"SET_DEFAULT": dbschema.SetDefault,
}
