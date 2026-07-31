package mysql

import (
	"context"
	"database/sql"
	"fmt"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// tableKeys is everything KEY_COLUMN_USAGE and its two companion catalogs say
// about one table: its primary key, its foreign keys, and the constraint records
// for both plus any unique constraints.
type tableKeys struct {
	primaryKey  *dbschema.PrimaryKey
	foreignKeys []dbschema.ForeignKey
	constraints []dbschema.Constraint
}

// keyRow is one row of keyColumnsQuery.
type keyRow struct {
	table          string
	constraint     string
	constraintType string
	column         string
	parentSchema   sql.NullString
	parentTable    sql.NullString
	parentColumn   sql.NullString
	onUpdate       sql.NullString
	onDelete       sql.NullString
}

// The constraint kinds TABLE_CONSTRAINTS reports for a key column.
const (
	primaryKeyConstraint = "PRIMARY KEY"
	foreignKeyConstraint = "FOREIGN KEY"
	uniqueConstraint     = "UNIQUE"
)

// readKeyConstraints reads every key constraint in the database, grouped by
// table.
//
// The rows arrive ordered by table, then constraint, then position within the
// constraint, so a multi-column key is a run of consecutive rows and can be
// assembled without holding the whole catalog in a map keyed by constraint.
func (a *Adapter) readKeyConstraints(ctx context.Context) (map[string]tableKeys, error) {
	grouped := map[string]tableKeys{}

	var (
		current  keyRow
		columns  []string
		parents  []string
		gathered bool
	)

	flush := func() {
		if gathered {
			applyConstraint(grouped, a.database, current, columns, parents)
		}

		gathered = false
		columns = nil
		parents = nil
	}

	err := a.eachRow(ctx, keyColumnsQuery, []any{a.database}, func(rows *sql.Rows) error {
		var row keyRow

		err := rows.Scan(&row.table, &row.constraint, &row.constraintType, &row.column,
			&row.parentSchema, &row.parentTable, &row.parentColumn, &row.onUpdate, &row.onDelete)
		if err != nil {
			return err
		}

		if gathered && (row.table != current.table || row.constraint != current.constraint) {
			flush()
		}

		current = row
		gathered = true

		columns = append(columns, row.column)
		if row.parentColumn.Valid {
			parents = append(parents, row.parentColumn.String)
		}

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mysql: read key constraints of %q: %w", a.database, err)
	}

	flush()

	return grouped, nil
}

// applyConstraint folds one completed constraint into the grouped result.
func applyConstraint(
	grouped map[string]tableKeys,
	database string,
	row keyRow,
	columns, parents []string,
) {
	keys := grouped[row.table]

	switch row.constraintType {
	case primaryKeyConstraint:
		name := row.constraint
		keys.primaryKey = &dbschema.PrimaryKey{Name: &name, Columns: columns}
		keys.constraints = append(keys.constraints, dbschema.Constraint{
			Name:           row.constraint,
			TableName:      row.table,
			ConstraintType: dbschema.ConstraintPrimaryKey,
			Columns:        columns,
		})
	case foreignKeyConstraint:
		keys.foreignKeys = append(keys.foreignKeys, foreignKey(database, row, columns, parents))
		keys.constraints = append(keys.constraints, dbschema.Constraint{
			Name:           row.constraint,
			TableName:      row.table,
			ConstraintType: dbschema.ConstraintForeignKey,
			Columns:        columns,
		})
	case uniqueConstraint:
		keys.constraints = append(keys.constraints, dbschema.Constraint{
			Name:           row.constraint,
			TableName:      row.table,
			ConstraintType: dbschema.ConstraintUnique,
			Columns:        columns,
		})
	}

	grouped[row.table] = keys
}

// foreignKey builds a foreign key from its accumulated rows.
func foreignKey(database string, row keyRow, columns, parents []string) dbschema.ForeignKey {
	name := row.constraint
	key := dbschema.ForeignKey{
		Name:              &name,
		Columns:           columns,
		ReferencedColumns: parents,
		OnDelete:          referentialAction(row.onDelete),
		OnUpdate:          referentialAction(row.onUpdate),
	}

	if row.parentTable.Valid {
		key.ReferencedTable = row.parentTable.String
	}

	// The parent database is recorded only when it differs from the child's, so
	// an ordinary same-database key does not carry a redundant qualifier into
	// every document.
	if row.parentSchema.Valid && row.parentSchema.String != "" && row.parentSchema.String != database {
		parent := row.parentSchema.String
		key.ReferencedSchema = &parent
	}

	return key
}

// referentialAction maps the rule text REFERENTIAL_CONSTRAINTS reports.
//
// The value is NULL for a key column that belongs to a primary or unique
// constraint rather than a foreign key, and MySQL reports "NO ACTION" where a
// foreign key declared no rule, so nil here means "not a foreign key" rather
// than "the engine default applies".
func referentialAction(rule sql.NullString) *dbschema.ReferentialAction {
	if !rule.Valid {
		return nil
	}

	mapped, ok := referentialActions[rule.String]
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
