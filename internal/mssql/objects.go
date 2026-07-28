package mssql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// collectViews reads every view with its definition.
//
// OBJECT_DEFINITION returns NULL for a view created WITH ENCRYPTION, and for one
// the credential lacks VIEW DEFINITION on. Both are common on a managed server
// and neither is an error: the view is recorded, only its body is missing.
func (a *Adapter) collectViews(ctx context.Context, document *dbschema.Schema) error {
	var keys []tableKey

	err := a.eachRow(ctx, viewsQuery, nil, func(rows *sql.Rows) error {
		var (
			key        tableKey
			definition sql.NullString
		)

		if err := rows.Scan(&key.schema, &key.table, &definition); err != nil {
			return err
		}

		view := dbschema.View{
			Name:    key.table,
			Schema:  schemaPointer(key.schema),
			Columns: []dbschema.Column{},
		}

		if definition.Valid && definition.String != "" {
			view.Definition = &definition.String
		}

		document.Views = append(document.Views, view)
		keys = append(keys, key)

		return nil
	})
	if err != nil {
		return fmt.Errorf("mssql: list views of %q: %w", a.database, err)
	}

	if len(keys) == 0 {
		return nil
	}

	// The columns query covers views as well as tables, so a view costs no extra
	// round trip.
	columns, err := a.readColumns(ctx)
	if err != nil {
		return err
	}

	for i, key := range keys {
		document.Views[i].Columns = columns[key]
	}

	return nil
}

// collectRoutines reads stored procedures and functions.
//
// The body is deliberately not collected. A routine body is arbitrary text an
// operator wrote, and it is exactly the place a hard-coded connection string
// tends to live -- collecting it would put a credential into the document that
// the credential scan then rejects, turning a readable database into an
// unwritable artifact.
func (a *Adapter) collectRoutines(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, routinesQuery, nil, func(rows *sql.Rows) error {
		var (
			key  tableKey
			kind string
		)

		if err := rows.Scan(&key.schema, &key.table, &kind); err != nil {
			return err
		}

		language := "tsql"
		routine := dbschema.Routine{
			Name:       key.table,
			Schema:     schemaPointer(key.schema),
			Parameters: []dbschema.Parameter{},
			Language:   &language,
		}

		if strings.TrimSpace(kind) == procedureKind {
			document.Procedures = append(document.Procedures, routine)
		} else {
			document.Functions = append(document.Functions, routine)
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("mssql: list routines of %q: %w", a.database, err)
	}

	return nil
}

// procedureKind is the sys.objects type of a stored procedure. The type column
// is CHAR(2), so the value arrives padded.
const procedureKind = "P"

// collectTriggers reads every trigger an operator declared.
//
// sys.trigger_events carries one row per event, so a trigger declared on both
// INSERT and UPDATE arrives as two rows. The document models one event per
// trigger, so both are recorded: reporting only the first would hide the other.
func (a *Adapter) collectTriggers(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, triggersQuery, nil, func(rows *sql.Rows) error {
		var (
			key       tableKey
			name      string
			event     string
			insteadOf bool
		)

		if err := rows.Scan(&key.schema, &key.table, &name, &event, &insteadOf); err != nil {
			return err
		}

		document.Triggers = append(document.Triggers, dbschema.Trigger{
			Name:      name,
			TableName: key.table,
			Schema:    schemaPointer(key.schema),
			Event:     triggerEvent(event),
			Timing:    triggerTiming(insteadOf),
		})

		return nil
	})
	if err != nil {
		return fmt.Errorf("mssql: list triggers of %q: %w", a.database, err)
	}

	return nil
}

// triggerEvent maps a sys.trigger_events type description.
func triggerEvent(event string) dbschema.TriggerEvent {
	switch strings.ToUpper(strings.TrimSpace(event)) {
	case "UPDATE":
		return dbschema.TriggerUpdate
	case "DELETE":
		return dbschema.TriggerDelete
	default:
		return dbschema.TriggerInsert
	}
}

// triggerTiming maps the instead-of flag.
//
// SQL Server has no BEFORE triggers: a DML trigger is either AFTER or INSTEAD
// OF. Reporting BEFORE for anything here would describe a timing the engine
// cannot produce.
func triggerTiming(insteadOf bool) dbschema.TriggerTiming {
	if insteadOf {
		return dbschema.TimingInsteadOf
	}

	return dbschema.TimingAfter
}

// collectUserTypes reads user-defined types, which on SQL Server are aliases
// over a system type.
func (a *Adapter) collectUserTypes(ctx context.Context, document *dbschema.Schema) error {
	err := a.eachRow(ctx, userTypesQuery, nil, func(rows *sql.Rows) error {
		var (
			key  tableKey
			base sql.NullString
		)

		if err := rows.Scan(&key.schema, &key.table, &base); err != nil {
			return err
		}

		document.UserTypes = append(document.UserTypes, dbschema.UserType{
			Name:       key.table,
			Schema:     schemaPointer(key.schema),
			Definition: base.String,
			// An alias type constrains a system type, which is what a domain is
			// in the vocabulary the document uses.
			Category: dbschema.CategoryDomain,
		})

		return nil
	})
	if err != nil {
		return fmt.Errorf("mssql: list user types of %q: %w", a.database, err)
	}

	return nil
}
