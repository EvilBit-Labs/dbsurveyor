package oracle

import (
	"context"
	"database/sql"
	"fmt"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// tableKeys is what ALL_CONSTRAINTS says about one table.
type tableKeys struct {
	primaryKey  *dbschema.PrimaryKey
	foreignKeys []dbschema.ForeignKey
	constraints []dbschema.Constraint
}

// The constraint_type codes ALL_CONSTRAINTS uses.
const (
	primaryKeyCode = "P"
	foreignKeyCode = "R"
	uniqueCode     = "U"
	checkCode      = "C"
)

// gathered accumulates the rows of one constraint before it is complete.
type gathered struct {
	table          string
	name           string
	constraintType string
	columns        []string
	condition      sql.NullString
	referenced     sql.NullString
	deleteRule     sql.NullString
}

// referencedKey is the primary or unique constraint a foreign key points at.
type referencedKey struct {
	table   string
	columns []string
}

// readConstraints reads every key and check constraint in the schema.
//
// Oracle names the far side of a foreign key by the *constraint* it references
// rather than by the table and columns, so resolving one takes a second pass
// over the same view collecting the primary and unique constraints by name.
// Doing it in two queries rather than a self-join keeps each query readable and
// costs one extra round trip for the whole schema.
func (a *Adapter) readConstraints(ctx context.Context) (map[string]tableKeys, error) {
	referenced, err := a.readReferencedKeys(ctx)
	if err != nil {
		return nil, err
	}

	grouped := map[string]tableKeys{}

	var current gathered

	flush := func() {
		if current.name != "" {
			applyConstraint(grouped, a.owner, current, referenced)
		}

		current = gathered{}
	}

	err = a.eachRow(ctx, constraintsQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var row gathered

		var column sql.NullString

		err := rows.Scan(&row.table, &row.name, &row.constraintType, &column,
			&row.condition, &row.referenced, &row.deleteRule)
		if err != nil {
			return err
		}

		if current.name != "" && (row.table != current.table || row.name != current.name) {
			flush()
		}

		columns := current.columns
		if column.Valid {
			columns = append(columns, column.String)
		}

		current = row
		current.columns = columns

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: read constraints of %q: %w", a.owner, err)
	}

	flush()

	return grouped, nil
}

// readReferencedKeys reads the primary and unique constraints by name, so a
// foreign key can be resolved to the table and columns it points at.
func (a *Adapter) readReferencedKeys(ctx context.Context) (map[string]referencedKey, error) {
	keys := map[string]referencedKey{}

	err := a.eachRow(ctx, referencedKeysQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name   string
			table  string
			column string
		)

		if err := rows.Scan(&name, &table, &column); err != nil {
			return err
		}

		key := keys[name]
		key.table = table
		key.columns = append(key.columns, column)
		keys[name] = key

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("oracle: read referenced keys of %q: %w", a.owner, err)
	}

	return keys, nil
}

// applyConstraint folds one completed constraint into the grouped result.
func applyConstraint(
	grouped map[string]tableKeys,
	owner string,
	row gathered,
	referenced map[string]referencedKey,
) {
	keys := grouped[row.table]

	switch row.constraintType {
	case primaryKeyCode:
		name := row.name
		keys.primaryKey = &dbschema.PrimaryKey{Name: &name, Columns: row.columns}
		keys.constraints = append(keys.constraints,
			constraint(owner, row, dbschema.ConstraintPrimaryKey))
	case foreignKeyCode:
		keys.foreignKeys = append(keys.foreignKeys, foreignKey(row, referenced))
		keys.constraints = append(keys.constraints,
			constraint(owner, row, dbschema.ConstraintForeignKey))
	case uniqueCode:
		keys.constraints = append(keys.constraints,
			constraint(owner, row, dbschema.ConstraintUnique))
	case checkCode:
		// Oracle records a NOT NULL declaration as a check constraint whose
		// condition is `"COL" IS NOT NULL`. The column's own nullability already
		// carries that, so recording it again would fill the document with one
		// constraint per non-nullable column and bury the checks an operator
		// actually wrote.
		if isNotNullCheck(row.condition) {
			return
		}

		check := constraint(owner, row, dbschema.ConstraintCheck)
		if row.condition.Valid {
			clause := strings.TrimSpace(row.condition.String)
			check.CheckClause = &clause
		}

		keys.constraints = append(keys.constraints, check)
	}

	grouped[row.table] = keys
}

// constraint builds the document record common to every constraint kind.
func constraint(owner string, row gathered, kind dbschema.ConstraintType) dbschema.Constraint {
	return dbschema.Constraint{
		Name:           row.name,
		TableName:      row.table,
		Schema:         ownerPointer(owner),
		ConstraintType: kind,
		Columns:        row.columns,
	}
}

// foreignKey resolves a referential constraint to the table and columns it
// points at.
func foreignKey(row gathered, referenced map[string]referencedKey) dbschema.ForeignKey {
	name := row.name
	key := dbschema.ForeignKey{
		Name:              &name,
		Columns:           row.columns,
		ReferencedColumns: []string{},
		OnDelete:          referentialAction(row.deleteRule),
	}

	// ALL_CONSTRAINTS has no update rule at all: Oracle does not implement ON
	// UPDATE for foreign keys, so leaving OnUpdate nil is accurate rather than
	// incomplete.
	if row.referenced.Valid {
		if parent, ok := referenced[row.referenced.String]; ok {
			key.ReferencedTable = parent.table
			key.ReferencedColumns = parent.columns
		}
	}

	return key
}

// isNotNullCheck reports whether a check condition is the one Oracle generates
// for a NOT NULL declaration.
func isNotNullCheck(condition sql.NullString) bool {
	if !condition.Valid {
		return false
	}

	return strings.HasSuffix(strings.ToUpper(strings.TrimSpace(condition.String)), "IS NOT NULL")
}

// referentialAction maps the delete rule the catalog reports.
//
// Oracle reports NO ACTION where a key declared nothing, and supports only three
// of the five actions the document models -- there is no RESTRICT and no SET
// DEFAULT.
func referentialAction(rule sql.NullString) *dbschema.ReferentialAction {
	if !rule.Valid {
		return nil
	}

	mapped, ok := referentialActions[strings.ToUpper(strings.TrimSpace(rule.String))]
	if !ok {
		return nil
	}

	return &mapped
}

var referentialActions = map[string]dbschema.ReferentialAction{
	"CASCADE":   dbschema.Cascade,
	"SET NULL":  dbschema.SetNull,
	"NO ACTION": dbschema.NoAction,
}
