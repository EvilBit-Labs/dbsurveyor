package oracle

import (
	"context"
	"database/sql"
	"fmt"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// collectViews reads every view with its definition.
func (a *Adapter) collectViews(ctx context.Context, document *dbschema.Schema) error {
	var names []string

	err := a.eachRow(ctx, viewsQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name       string
			definition sql.NullString
		)

		if err := rows.Scan(&name, &definition); err != nil {
			return err
		}

		view := dbschema.View{
			Name:    name,
			Schema:  ownerPointer(a.owner),
			Columns: []dbschema.Column{},
		}

		if definition.Valid && strings.TrimSpace(definition.String) != "" {
			text := strings.TrimSpace(definition.String)
			view.Definition = &text
		}

		document.Views = append(document.Views, view)
		names = append(names, name)

		return nil
	})
	if err != nil {
		return fmt.Errorf("oracle: list views of %q: %w", a.owner, err)
	}

	if len(names) == 0 {
		return nil
	}

	// ALL_TAB_COLUMNS covers views as well as tables, so a view costs no extra
	// round trip.
	columns, err := a.readColumns(ctx)
	if err != nil {
		return err
	}

	for i, name := range names {
		document.Views[i].Columns = columns[name]
	}

	return nil
}

// collectRoutines reads stored procedures and functions.
//
// The body is deliberately not collected. A routine body is arbitrary text an
// operator wrote, and it is exactly the place a hard-coded connection string
// tends to live -- collecting it would put a credential into the document that
// the credential scan then rejects, turning a readable schema into an unwritable
// artifact.
func (a *Adapter) collectRoutines(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, routinesQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name string
			kind string
		)

		if err := rows.Scan(&name, &kind); err != nil {
			return err
		}

		language := "plsql"
		routine := dbschema.Routine{
			Name:       name,
			Schema:     ownerPointer(a.owner),
			Parameters: []dbschema.Parameter{},
			Language:   &language,
		}

		if strings.EqualFold(strings.TrimSpace(kind), "FUNCTION") {
			document.Functions = append(document.Functions, routine)
		} else {
			document.Procedures = append(document.Procedures, routine)
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("oracle: list routines of %q: %w", a.owner, err)
	}

	return nil
}

// collectTriggers reads every trigger on a table in the schema.
//
// TRIGGERING_EVENT is a list -- "INSERT OR UPDATE" -- and the document models one
// event per trigger, so a multi-event trigger is recorded once per event.
// Reporting only the first would hide the others.
func (a *Adapter) collectTriggers(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, triggersQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name        string
			table       string
			event       string
			triggerType string
		)

		if err := rows.Scan(&name, &table, &event, &triggerType); err != nil {
			return err
		}

		for _, mapped := range triggerEvents(event) {
			document.Triggers = append(document.Triggers, dbschema.Trigger{
				Name:      name,
				TableName: table,
				Schema:    ownerPointer(a.owner),
				Event:     mapped,
				Timing:    triggerTiming(triggerType),
			})
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("oracle: list triggers of %q: %w", a.owner, err)
	}

	return nil
}

// triggerEvents unpacks the event list, defaulting to INSERT when it names none
// the document models.
func triggerEvents(event string) []dbschema.TriggerEvent {
	upper := strings.ToUpper(event)

	var events []dbschema.TriggerEvent

	if strings.Contains(upper, "INSERT") {
		events = append(events, dbschema.TriggerInsert)
	}

	if strings.Contains(upper, "UPDATE") {
		events = append(events, dbschema.TriggerUpdate)
	}

	if strings.Contains(upper, "DELETE") {
		events = append(events, dbschema.TriggerDelete)
	}

	if len(events) == 0 {
		// A trigger on TRUNCATE or on a DDL event reaches here. The document has
		// no such event, and dropping the trigger entirely would hide it.
		events = append(events, dbschema.TriggerInsert)
	}

	return events
}

// triggerTiming reads the timing out of TRIGGER_TYPE, which spells it as
// "BEFORE EACH ROW", "AFTER STATEMENT", or "INSTEAD OF".
func triggerTiming(triggerType string) dbschema.TriggerTiming {
	upper := strings.ToUpper(triggerType)

	switch {
	case strings.Contains(upper, "INSTEAD OF"):
		return dbschema.TimingInsteadOf
	case strings.Contains(upper, "AFTER"):
		return dbschema.TimingAfter
	default:
		return dbschema.TimingBefore
	}
}

// collectUserTypes reads user-defined types.
func (a *Adapter) collectUserTypes(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, userTypesQuery, []any{a.owner}, func(rows *sql.Rows) error {
		var (
			name     string
			typecode sql.NullString
		)

		if err := rows.Scan(&name, &typecode); err != nil {
			return err
		}

		document.UserTypes = append(document.UserTypes, dbschema.UserType{
			Name:       name,
			Schema:     ownerPointer(a.owner),
			Definition: strings.TrimSpace(typecode.String),
			Category:   typeCategory(typecode.String),
		})

		return nil
	})
	if err != nil {
		return fmt.Errorf("oracle: list user types of %q: %w", a.owner, err)
	}

	return nil
}

// typeCategory maps the ALL_TYPES typecode.
//
// Oracle has no enum type and no range type, so an object type is the composite
// case and everything else -- a collection, an opaque type -- is reported as a
// domain, which is the closest thing the document models to "a named constraint
// over something else".
func typeCategory(typecode string) dbschema.TypeCategory {
	if strings.EqualFold(strings.TrimSpace(typecode), "OBJECT") {
		return dbschema.CategoryComposite
	}

	return dbschema.CategoryDomain
}
