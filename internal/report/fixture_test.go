package report

import (
	"testing"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// fixedTime is the timestamp every fixture uses.
//
// A golden file whose content changed every second would assert nothing, so the
// clock is a value rather than a reading. This is also why report.Options has a
// Now field at all.
var fixedTime = time.Date(2026, time.July, 27, 12, 0, 0, 0, time.UTC)

// pointer returns a pointer to a value, for the schema's many optional fields.
func pointer[T any](value T) *T {
	return &value
}

// newFixture builds a schema document exercising most of the renderer.
//
// It is deliberately varied rather than minimal: a table with every column flag,
// a table with none, a view with a definition, a routine, a trigger, a
// user-defined type, and a collection warning. A golden file over a minimal
// document would go on passing while half the renderer rotted.
func newFixture(t *testing.T) *dbschema.Schema {
	t.Helper()

	info := dbschema.NewDatabaseInfo("shop", dbschema.PostgreSQL)
	info.Version = pointer("17.2")
	info.Encoding = pointer("UTF8")
	info.Collation = pointer("en_US.utf8")
	info.Owner = pointer("surveyor")
	info.SizeBytes = pointer(uint64(15_728_640))

	document := dbschema.New(info, "test", fixedTime)
	document.Tables = []dbschema.Table{usersTable(), ordersTable()}
	document.Views = []dbschema.View{activeUsersView()}
	document.Functions = []dbschema.Routine{addFunction()}
	document.Triggers = []dbschema.Trigger{touchTrigger()}
	document.UserTypes = []dbschema.UserType{moodType()}
	document.AddWarning("table audit reported no columns")
	document.AggregateIndexesAndConstraints()

	return document
}

func usersTable() dbschema.Table {
	public := "public"

	return dbschema.Table{
		Name:    "users",
		Schema:  &public,
		Comment: pointer("people who can sign in"),
		Columns: []dbschema.Column{
			{
				Name:            "id",
				DataType:        dbschema.IntegerType(64, true),
				PrimaryKey:      true,
				AutoGenerate:    true,
				OrdinalPosition: 1,
			},
			{
				Name:            "email",
				DataType:        dbschema.StringType(nil),
				OrdinalPosition: 2,
				Comment:         pointer("unique, lowercased"),
			},
			{
				Name:            "display_name",
				DataType:        dbschema.StringType(pointer(uint32(64))),
				Nullable:        true,
				OrdinalPosition: 3,
			},
			{
				Name:            "created_at",
				DataType:        dbschema.DateTimeType(true),
				Default:         pointer("now()"),
				OrdinalPosition: 4,
			},
		},
		PrimaryKey:  &dbschema.PrimaryKey{Name: pointer("users_pkey"), Columns: []string{"id"}},
		ForeignKeys: []dbschema.ForeignKey{},
		Indexes: []dbschema.Index{
			{
				Name:      "users_pkey",
				TableName: "users",
				Schema:    &public,
				Columns:   []dbschema.IndexColumn{{Name: "id"}},
				Unique:    true,
				Primary:   true,
				IndexType: pointer("btree"),
			},
			{
				Name:      "users_by_created",
				TableName: "users",
				Schema:    &public,
				Columns: []dbschema.IndexColumn{
					{Name: "created_at", SortOrder: pointer(dbschema.Descending)},
				},
				IndexType: pointer("btree"),
			},
		},
		Constraints: []dbschema.Constraint{
			{
				Name:           "users_pkey",
				TableName:      "users",
				Schema:         &public,
				ConstraintType: dbschema.ConstraintPrimaryKey,
				Columns:        []string{"id"},
			},
			{
				Name:           "users_email_lowercase",
				TableName:      "users",
				Schema:         &public,
				ConstraintType: dbschema.ConstraintCheck,
				Columns:        []string{"email"},
				CheckClause:    pointer("email = lower(email)"),
			},
		},
		RowCount: pointer(uint64(1482)),
	}
}

func ordersTable() dbschema.Table {
	public := "public"

	return dbschema.Table{
		Name:   "orders",
		Schema: &public,
		Columns: []dbschema.Column{
			{Name: "id", DataType: dbschema.IntegerType(32, true), PrimaryKey: true, OrdinalPosition: 1},
			{Name: "user_id", DataType: dbschema.IntegerType(64, true), OrdinalPosition: 2},
			{Name: "total", DataType: dbschema.FloatType(pointer(uint8(10))), OrdinalPosition: 3},
		},
		PrimaryKey: &dbschema.PrimaryKey{Columns: []string{"id"}},
		ForeignKeys: []dbschema.ForeignKey{
			{
				Name:              pointer("orders_user_fk"),
				Columns:           []string{"user_id"},
				ReferencedTable:   "users",
				ReferencedColumns: []string{"id"},
				OnDelete:          pointer(dbschema.Cascade),
			},
		},
		Indexes:     []dbschema.Index{},
		Constraints: []dbschema.Constraint{},
		// A nil row count is the engine saying it maintains no such statistic,
		// which the renderer must not show as zero.
		RowCount: nil,
	}
}

func activeUsersView() dbschema.View {
	public := "public"

	return dbschema.View{
		Name:       "active_users",
		Schema:     &public,
		Definition: pointer("SELECT id, email FROM users WHERE active"),
		Columns: []dbschema.Column{
			{Name: "id", DataType: dbschema.IntegerType(64, true), OrdinalPosition: 1},
			{Name: "email", DataType: dbschema.StringType(nil), OrdinalPosition: 2},
		},
	}
}

func addFunction() dbschema.Routine {
	public := "public"

	return dbschema.Routine{
		Name:     "add",
		Schema:   &public,
		Language: pointer("sql"),
		Parameters: []dbschema.Parameter{
			{Name: "a", DataType: dbschema.IntegerType(32, true), Direction: dbschema.DirectionIn},
			{Name: "b", DataType: dbschema.IntegerType(32, true), Direction: dbschema.DirectionIn},
		},
		ReturnType: pointer(dbschema.IntegerType(32, true)),
	}
}

func touchTrigger() dbschema.Trigger {
	public := "public"

	return dbschema.Trigger{
		Name:      "users_touched",
		TableName: "users",
		Schema:    &public,
		Event:     dbschema.TriggerUpdate,
		Timing:    dbschema.TimingBefore,
	}
}

func moodType() dbschema.UserType {
	public := "public"

	return dbschema.UserType{
		Name:       "mood",
		Schema:     &public,
		Definition: "happy, sad",
		Category:   dbschema.CategoryEnum,
	}
}

// newSamples returns sampled rows carrying a value each redaction mode treats
// differently.
func newSamples() []dbschema.TableSample {
	status := dbschema.Complete()
	public := "public"

	return []dbschema.TableSample{{
		TableName:  "users",
		SchemaName: &public,
		Rows: []map[string]any{
			{"id": 1, "email": "ada@example.test", "api_key": "a-plain-looking-token"},
			{"id": 2, "email": "grace@example.test", "api_key": "another-token"},
		},
		SampleSize:       2,
		SamplingStrategy: dbschema.MostRecent(2),
		CollectedAt:      fixedTime,
		Warnings:         []string{},
		Status:           &status,
	}}
}
