package main

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// exampleTime is the fixed timestamp every published example carries. Examples
// are generated artifacts committed to the repository, so they must not change
// when regenerated on a different day.
var exampleTime = time.Date(2026, time.July, 26, 12, 0, 0, 0, time.UTC)

const exampleVersion = "0.1.0"

// examples are the documents referenced from docs/formats/schema-document.md.
// Generating them from the Go constructors rather than hand-writing them means
// a documented example can never be one the implementation would reject.
func exampleArtifacts() []artifact {
	return []artifact{
		{filename: "examples/minimal-schema.json", root: minimalSchema()},
		{filename: "examples/full-schema.json", root: fullSchema()},
		{filename: "examples/server-document.json", root: serverDocument()},
	}
}

// encodeExample renders an example document as indented, newline-terminated JSON.
func encodeExample(v any) ([]byte, error) {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encode example: %w", err)
	}

	return append(data, '\n'), nil
}

// minimalSchema is an empty database: every required field present, every
// optional field absent. It is the smallest document a reader must accept.
func minimalSchema() *dbschema.Schema {
	return dbschema.New(
		dbschema.NewDatabaseInfo("inventory", dbschema.PostgreSQL),
		exampleVersion,
		exampleTime,
	)
}

// fullSchema exercises every optional field and every discriminated union so
// the published example doubles as a reader conformance fixture.
func fullSchema() *dbschema.Schema {
	schema := minimalSchema()
	schema.DatabaseInfo.Version = ptr("16.2")
	schema.DatabaseInfo.SizeBytes = ptr(uint64(52428800))
	schema.DatabaseInfo.Encoding = ptr("UTF8")
	schema.DatabaseInfo.Collation = ptr("en_US.UTF-8")
	schema.DatabaseInfo.Owner = ptr("app_owner")
	schema.CollectionMetadata.DurationMS = 1843
	schema.AddWarning("skipped table audit.events: permission denied")

	schema.Tables = []dbschema.Table{ordersTable()}
	schema.Views = []dbschema.View{{
		Name:       "recent_orders",
		Schema:     ptr("public"),
		Definition: ptr("SELECT * FROM orders WHERE placed_at > now() - interval '7 days'"),
		Columns:    ordersTable().Columns,
		Comment:    ptr("Rolling seven-day order window."),
	}}
	schema.Procedures = []dbschema.Routine{{
		Name:       "archive_orders",
		Schema:     ptr("public"),
		Definition: ptr("BEGIN DELETE FROM orders WHERE placed_at < cutoff; END"),
		Parameters: []dbschema.Parameter{{
			Name:      "cutoff",
			DataType:  dbschema.DateTimeType(true),
			Direction: dbschema.DirectionIn,
		}},
		Language: ptr("plpgsql"),
	}}
	schema.Functions = []dbschema.Routine{{
		Name:   "order_total",
		Schema: ptr("public"),
		Parameters: []dbschema.Parameter{{
			Name:      "order_id",
			DataType:  dbschema.IntegerType(64, true),
			Direction: dbschema.DirectionIn,
			Default:   ptr("0"),
		}},
		ReturnType: ptr(dbschema.FloatType(ptr(uint8(53)))),
		Language:   ptr("sql"),
	}}
	schema.Triggers = []dbschema.Trigger{{
		Name:       "orders_audit",
		TableName:  "orders",
		Schema:     ptr("public"),
		Event:      dbschema.TriggerUpdate,
		Timing:     dbschema.TimingAfter,
		Definition: ptr("EXECUTE FUNCTION write_audit()"),
	}}
	schema.UserTypes = []dbschema.UserType{{
		Name:       "order_state",
		Schema:     ptr("public"),
		Definition: "ENUM ('pending', 'shipped', 'cancelled')",
		Category:   dbschema.CategoryEnum,
	}}
	schema.Samples = []dbschema.TableSample{completeSample(), skippedSample()}

	schema.AggregateIndexesAndConstraints()

	return schema
}

// ordersTable is the single table carried by the full example.
func ordersTable() dbschema.Table {
	return dbschema.Table{
		Name:   "orders",
		Schema: ptr("public"),
		Columns: []dbschema.Column{
			{
				Name:            "id",
				DataType:        dbschema.IntegerType(64, true),
				PrimaryKey:      true,
				AutoGenerate:    true,
				Default:         ptr("nextval('orders_id_seq'::regclass)"),
				Comment:         ptr("Surrogate key."),
				OrdinalPosition: 1,
			},
			{
				Name:            "customer_id",
				DataType:        dbschema.IntegerType(64, true),
				OrdinalPosition: 2,
			},
			{
				Name:            "reference",
				DataType:        dbschema.StringType(ptr(uint32(64))),
				Nullable:        true,
				OrdinalPosition: 3,
			},
			{
				Name:            "tags",
				DataType:        dbschema.ArrayType(dbschema.StringType(nil)),
				Nullable:        true,
				OrdinalPosition: 4,
			},
			{
				Name:            "placed_at",
				DataType:        dbschema.DateTimeType(true),
				OrdinalPosition: 5,
			},
		},
		PrimaryKey: &dbschema.PrimaryKey{Name: ptr("orders_pkey"), Columns: []string{"id"}},
		ForeignKeys: []dbschema.ForeignKey{{
			Name:              ptr("orders_customer_fk"),
			Columns:           []string{"customer_id"},
			ReferencedTable:   "customers",
			ReferencedSchema:  ptr("public"),
			ReferencedColumns: []string{"id"},
			OnDelete:          ptr(dbschema.Cascade),
			OnUpdate:          ptr(dbschema.NoAction),
		}},
		Indexes: []dbschema.Index{{
			Name:      "orders_placed_at_idx",
			TableName: "orders",
			Schema:    ptr("public"),
			Columns: []dbschema.IndexColumn{
				{Name: "placed_at", SortOrder: ptr(dbschema.Descending)},
			},
			IndexType: ptr("btree"),
		}},
		Constraints: []dbschema.Constraint{{
			Name:           "orders_reference_check",
			TableName:      "orders",
			Schema:         ptr("public"),
			ConstraintType: dbschema.ConstraintCheck,
			Columns:        []string{"reference"},
			CheckClause:    ptr("length(reference) > 0"),
		}},
		Comment:  ptr("Customer orders."),
		RowCount: ptr(uint64(148231)),
	}
}

// completeSample is a sample that ran to completion.
func completeSample() dbschema.TableSample {
	return dbschema.TableSample{
		TableName:  "orders",
		SchemaName: ptr("public"),
		Rows: []map[string]any{
			{"id": 148231, "customer_id": 907, "reference": "AB-1201", "placed_at": "2026-07-26T09:14:02Z"},
		},
		SampleSize:       1,
		TotalRows:        ptr(uint64(148231)),
		SamplingStrategy: dbschema.MostRecent(1),
		Ordering: &dbschema.OrderingStrategy{
			Kind:      dbschema.OrderTimestamp,
			Column:    ptr("placed_at"),
			Direction: ptr(dbschema.Descending),
		},
		CollectedAt: exampleTime,
		Warnings:    []string{},
		Status:      ptr(dbschema.Complete()),
	}
}

// skippedSample is a sample that was never attempted, showing the skipped
// status shape alongside the complete one.
func skippedSample() dbschema.TableSample {
	return dbschema.TableSample{
		TableName:        "audit_events",
		SchemaName:       ptr("audit"),
		Rows:             []map[string]any{},
		SamplingStrategy: dbschema.NoSampling(),
		Ordering:         &dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered},
		CollectedAt:      exampleTime,
		Warnings:         []string{"permission denied"},
		Status:           ptr(dbschema.Skipped("permission denied")),
	}
}

// serverDocument is a two-database multi-database collection, one collected and
// one skipped.
func serverDocument() *dbschema.ServerSchema {
	doc := dbschema.NewServerSchema(dbschema.ServerInfo{
		ServerType:              dbschema.PostgreSQL,
		Version:                 "16.2",
		Host:                    "db.internal",
		Port:                    ptr(uint16(5432)),
		TotalDatabases:          5,
		CollectedDatabases:      1,
		SystemDatabasesExcluded: 3,
		ConnectionUser:          "surveyor",
		CollectionMode:          dbschema.MultiDatabase(5, 1, 1),
	}, exampleVersion, exampleTime)

	collected := minimalSchema()

	skipped := dbschema.New(
		dbschema.NewDatabaseInfo("reporting", dbschema.PostgreSQL),
		exampleVersion,
		exampleTime,
	)
	skipped.DatabaseInfo.AccessLevel = dbschema.AccessNone
	skipped.DatabaseInfo.CollectionStatus = dbschema.CollectionFailure("connect: permission denied")

	doc.Databases = []dbschema.Schema{*collected, *skipped}

	return doc
}

// ptr returns a pointer to v, for the optional fields of the schema types.
func ptr[T any](v T) *T {
	return &v
}
