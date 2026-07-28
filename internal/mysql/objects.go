package mysql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// collectViews reads every view with its definition.
//
// VIEW_DEFINITION is empty rather than NULL for a credential that lacks SHOW
// VIEW on the view, which is a common outcome on managed servers and is not an
// error. The view is still recorded; only its body is missing.
func (a *Adapter) collectViews(ctx context.Context, schema *dbschema.Schema) error {
	err := a.eachRow(ctx, viewsQuery, []any{a.database}, func(rows *sql.Rows) error {
		var (
			name       string
			definition sql.NullString
		)

		if err := rows.Scan(&name, &definition); err != nil {
			return err
		}

		view := dbschema.View{
			Name:    name,
			Schema:  &a.database,
			Columns: []dbschema.Column{},
		}

		if definition.Valid && definition.String != "" {
			view.Definition = &definition.String
		}

		schema.Views = append(schema.Views, view)

		return nil
	})
	if err != nil {
		return fmt.Errorf("mysql: list views of %q: %w", a.database, err)
	}

	return a.attachViewColumns(ctx, schema)
}

// attachViewColumns fills in the columns of each collected view.
//
// INFORMATION_SCHEMA.COLUMNS carries view columns alongside table columns, so
// they come from the query already run for tables rather than from a second
// round trip per view.
func (a *Adapter) attachViewColumns(ctx context.Context, schema *dbschema.Schema) error {
	if len(schema.Views) == 0 {
		return nil
	}

	columns, err := a.readColumns(ctx)
	if err != nil {
		return err
	}

	for i := range schema.Views {
		schema.Views[i].Columns = columns[schema.Views[i].Name]
	}

	return nil
}

// collectRoutines reads stored procedures and functions.
//
// The two go into different lists on the document because MySQL distinguishes
// them: a procedure is called with CALL and returns nothing, a function returns
// a value and is usable in an expression.
func (a *Adapter) collectRoutines(ctx context.Context, schema *dbschema.Schema) error {
	err := a.eachRow(ctx, routinesQuery, []any{a.database}, func(rows *sql.Rows) error {
		var (
			name        string
			kind        string
			returnType  sql.NullString
			definition  sql.NullString
			bodyDialect sql.NullString
		)

		if err := rows.Scan(&name, &kind, &returnType, &definition, &bodyDialect); err != nil {
			return err
		}

		routine := dbschema.Routine{
			Name:       name,
			Schema:     &a.database,
			Parameters: []dbschema.Parameter{},
		}

		if definition.Valid && definition.String != "" {
			routine.Definition = &definition.String
		}

		if bodyDialect.Valid && bodyDialect.String != "" {
			language := strings.ToLower(bodyDialect.String)
			routine.Language = &language
		}

		if returnType.Valid && returnType.String != "" {
			mapped := mapDataType(columnType{DataType: returnType.String, FullType: returnType.String})
			routine.ReturnType = &mapped
		}

		if strings.EqualFold(kind, "FUNCTION") {
			schema.Functions = append(schema.Functions, routine)
		} else {
			schema.Procedures = append(schema.Procedures, routine)
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("mysql: list routines of %q: %w", a.database, err)
	}

	return nil
}

// collectTriggers reads every trigger.
func (a *Adapter) collectTriggers(ctx context.Context, schema *dbschema.Schema) error {
	err := a.eachRow(ctx, triggersQuery, []any{a.database}, func(rows *sql.Rows) error {
		var (
			name       string
			table      string
			event      string
			timing     string
			definition sql.NullString
		)

		if err := rows.Scan(&name, &table, &event, &timing, &definition); err != nil {
			return err
		}

		trigger := dbschema.Trigger{
			Name:      name,
			TableName: table,
			Schema:    &a.database,
			Event:     triggerEvent(event),
			Timing:    triggerTiming(timing),
		}

		if definition.Valid && definition.String != "" {
			trigger.Definition = &definition.String
		}

		schema.Triggers = append(schema.Triggers, trigger)

		return nil
	})
	if err != nil {
		return fmt.Errorf("mysql: list triggers of %q: %w", a.database, err)
	}

	return nil
}

// triggerEvent maps EVENT_MANIPULATION.
func triggerEvent(event string) dbschema.TriggerEvent {
	switch strings.ToUpper(event) {
	case "UPDATE":
		return dbschema.TriggerUpdate
	case "DELETE":
		return dbschema.TriggerDelete
	default:
		return dbschema.TriggerInsert
	}
}

// triggerTiming maps ACTION_TIMING. MySQL has no INSTEAD OF triggers, so only
// the two timings it does have are recognized.
func triggerTiming(timing string) dbschema.TriggerTiming {
	if strings.EqualFold(timing, "AFTER") {
		return dbschema.TimingAfter
	}

	return dbschema.TimingBefore
}

// systemDatabases are the catalogs the server provides. They are the same on
// every MySQL server, so including them in a multi-database survey drowns the
// databases an operator is actually looking at.
var systemDatabases = map[string]struct{}{
	"information_schema": {},
	"mysql":              {},
	"performance_schema": {},
	"sys":                {},
}

// isSystemDatabase reports whether a database is one the server provides.
func isSystemDatabase(name string) bool {
	_, system := systemDatabases[strings.ToLower(name)]

	return system
}
