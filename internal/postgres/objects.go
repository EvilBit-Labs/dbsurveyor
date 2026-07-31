package postgres

import (
	"context"
	"fmt"
	"strings"

	"github.com/jackc/pgx/v5"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// collectViews reads every view and materialized view with its definition.
//
// The columns come from the same metadata query the tables used -- the columns
// template covers relkind 'v' and 'm' as well as 'r' -- so a view costs no extra
// round trip.
func (a *Adapter) collectViews(ctx context.Context, document *dbschema.Schema, schemas []string) error {
	var keys []tableKey

	err := a.eachRow(ctx, viewsQuery, []any{schemas}, func(rows pgx.Rows) error {
		var (
			key        tableKey
			definition *string
			comment    *string
		)

		if err := rows.Scan(&key.schema, &key.table, &definition, &comment); err != nil {
			return err
		}

		document.Views = append(document.Views, dbschema.View{
			Name:       key.table,
			Schema:     schemaPointer(key.schema),
			Definition: definition,
			Columns:    []dbschema.Column{},
			Comment:    comment,
		})
		keys = append(keys, key)

		return nil
	})
	if err != nil {
		return fmt.Errorf("postgres: list views of %q: %w", a.database, err)
	}

	if len(keys) == 0 {
		return nil
	}

	collected := newMetadata()
	if err := a.runCollector(ctx, collectors[0], schemaPredicate, []any{schemas}, collected); err != nil {
		return fmt.Errorf("postgres: read view columns of %q: %w", a.database, err)
	}

	for i, key := range keys {
		document.Views[i].Columns = collected.columns[key]
	}

	return nil
}

// collectRoutines reads functions and procedures.
//
// Only the signature and the result type are recorded, not the body. A function
// body is arbitrary text an operator wrote, and it is exactly the place a
// hard-coded connection string tends to live -- collecting it would put a
// credential into the document that the credential scan then rejects, turning a
// readable database into an unwritable artifact.
func (a *Adapter) collectRoutines(ctx context.Context, document *dbschema.Schema, schemas []string) error {
	err := a.eachRow(ctx, routinesQuery, []any{schemas}, func(rows pgx.Rows) error {
		var (
			key       tableKey
			kind      string
			language  string
			result    *string
			arguments *string
			comment   *string
		)

		if err := rows.Scan(&key.schema, &key.table, &kind, &language, &result, &arguments, &comment); err != nil {
			return err
		}

		routine := dbschema.Routine{
			Name:       key.table,
			Schema:     schemaPointer(key.schema),
			Parameters: parseParameters(arguments),
			Language:   &language,
			Comment:    comment,
		}

		if result != nil && *result != "" && !strings.EqualFold(*result, "void") {
			mapped := dbschema.CustomType(*result)
			routine.ReturnType = &mapped
		}

		if kind == procedureKind {
			document.Procedures = append(document.Procedures, routine)
		} else {
			document.Functions = append(document.Functions, routine)
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("postgres: list routines of %q: %w", a.database, err)
	}

	return nil
}

// procedureKind is the prokind value pg_proc gives a procedure.
const procedureKind = "p"

// parseParameters reads the identity argument list pg_get_function_identity_arguments
// produced, which is a comma-separated list of "name type" or bare types.
func parseParameters(arguments *string) []dbschema.Parameter {
	parameters := []dbschema.Parameter{}

	if arguments == nil || strings.TrimSpace(*arguments) == "" {
		return parameters
	}

	for _, argument := range strings.Split(*arguments, ",") {
		argument = strings.TrimSpace(argument)
		if argument == "" {
			continue
		}

		direction := dbschema.DirectionIn

		for prefix, mapped := range parameterDirections {
			if trimmed, found := strings.CutPrefix(argument, prefix); found {
				direction = mapped
				argument = strings.TrimSpace(trimmed)

				break
			}
		}

		name, declared, named := strings.Cut(argument, " ")
		if !named {
			// An unnamed parameter is positional; the whole token is its type.
			declared = name
			name = ""
		}

		parameters = append(parameters, dbschema.Parameter{
			Name:      name,
			DataType:  dbschema.CustomType(strings.TrimSpace(declared)),
			Direction: direction,
		})
	}

	return parameters
}

// parameterDirections are the mode prefixes an identity argument list can carry.
// IN is the default and is not written, so it is absent here.
var parameterDirections = map[string]dbschema.ParameterDirection{
	"INOUT ":    dbschema.DirectionInOut,
	"OUT ":      dbschema.DirectionOut,
	"VARIADIC ": dbschema.DirectionIn,
}

// The tgtype bits pg_trigger packs a trigger's timing and events into. There is
// no catalog view that unpacks them, so the constants are named here rather than
// written as magic numbers at the point of use.
const (
	triggerBeforeBit    int16 = 1 << 1
	triggerInsertBit    int16 = 1 << 2
	triggerDeleteBit    int16 = 1 << 3
	triggerUpdateBit    int16 = 1 << 4
	triggerInsteadOfBit int16 = 1 << 6
)

// collectTriggers reads every trigger an operator declared.
//
// A trigger can fire on more than one event, and the document models one event
// per trigger, so a multi-event trigger is recorded once per event it fires on.
// Reporting only the first would hide the others.
func (a *Adapter) collectTriggers(ctx context.Context, document *dbschema.Schema, schemas []string) error {
	err := a.eachRow(ctx, triggersQuery, []any{schemas}, func(rows pgx.Rows) error {
		var (
			key        tableKey
			name       string
			flags      int16
			definition *string
		)

		if err := rows.Scan(&key.schema, &key.table, &name, &flags, &definition); err != nil {
			return err
		}

		for _, event := range triggerEvents(flags) {
			document.Triggers = append(document.Triggers, dbschema.Trigger{
				Name:       name,
				TableName:  key.table,
				Schema:     schemaPointer(key.schema),
				Event:      event,
				Timing:     triggerTiming(flags),
				Definition: definition,
			})
		}

		return nil
	})
	if err != nil {
		return fmt.Errorf("postgres: list triggers of %q: %w", a.database, err)
	}

	return nil
}

// triggerEvents unpacks the events a trigger fires on, defaulting to INSERT when
// the flags name none the document models.
func triggerEvents(flags int16) []dbschema.TriggerEvent {
	var events []dbschema.TriggerEvent

	if flags&triggerInsertBit != 0 {
		events = append(events, dbschema.TriggerInsert)
	}

	if flags&triggerUpdateBit != 0 {
		events = append(events, dbschema.TriggerUpdate)
	}

	if flags&triggerDeleteBit != 0 {
		events = append(events, dbschema.TriggerDelete)
	}

	if len(events) == 0 {
		// A TRUNCATE trigger reaches here. The document has no truncate event,
		// and dropping the trigger entirely would hide it, so it is recorded
		// under the event a reader will check the definition against.
		events = append(events, dbschema.TriggerInsert)
	}

	return events
}

// triggerTiming unpacks when a trigger fires relative to its event.
func triggerTiming(flags int16) dbschema.TriggerTiming {
	switch {
	case flags&triggerInsteadOfBit != 0:
		return dbschema.TimingInsteadOf
	case flags&triggerBeforeBit != 0:
		return dbschema.TimingBefore
	default:
		return dbschema.TimingAfter
	}
}

// collectUserTypes reads enums, composites, domains, and ranges.
func (a *Adapter) collectUserTypes(ctx context.Context, document *dbschema.Schema, schemas []string) error {
	err := a.eachRow(ctx, userTypesQuery, []any{schemas}, func(rows pgx.Rows) error {
		var (
			key      tableKey
			category string
			labels   string
		)

		if err := rows.Scan(&key.schema, &key.table, &category, &labels); err != nil {
			return err
		}

		document.UserTypes = append(document.UserTypes, dbschema.UserType{
			Name:       key.table,
			Schema:     schemaPointer(key.schema),
			Definition: labels,
			Category:   typeCategory(category),
		})

		return nil
	})
	if err != nil {
		return fmt.Errorf("postgres: list user types of %q: %w", a.database, err)
	}

	return nil
}

// typeCategory maps the typtype value pg_type reports.
func typeCategory(category string) dbschema.TypeCategory {
	switch category {
	case "c":
		return dbschema.CategoryComposite
	case "d":
		return dbschema.CategoryDomain
	case "r":
		return dbschema.CategoryRange
	default:
		return dbschema.CategoryEnum
	}
}
