package dbschema

import (
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// testTime is a fixed instant so round-trip comparisons do not depend on the
// clock or on monotonic-clock readings, which json drops.
var testTime = time.Date(2026, time.July, 26, 12, 0, 0, 0, time.UTC)

func ptr[T any](v T) *T {
	return &v
}

// populatedSchema is a schema with every optional field set and every union
// carrying a payload, so a round-trip over it exercises the whole document.
func populatedSchema(t *testing.T) *Schema {
	t.Helper()

	schema := New(NewDatabaseInfo("inventory", PostgreSQL), "0.1.0", testTime)
	schema.DatabaseInfo.Version = ptr("16.2")
	schema.DatabaseInfo.SizeBytes = ptr(uint64(4096))
	schema.DatabaseInfo.Encoding = ptr("UTF8")
	schema.DatabaseInfo.Collation = ptr("en_US.UTF-8")
	schema.DatabaseInfo.Owner = ptr("app_owner")
	schema.DatabaseInfo.SystemDatabase = true
	schema.DatabaseInfo.AccessLevel = AccessLimited
	schema.DatabaseInfo.CollectionStatus = CollectionFailure("permission denied")
	schema.CollectionMetadata.DurationMS = 1200
	schema.AddWarning("one table skipped")

	schema.Tables = []Table{{
		Name:   "orders",
		Schema: ptr("public"),
		Columns: []Column{{
			Name:            "id",
			DataType:        IntegerType(64, true),
			PrimaryKey:      true,
			AutoGenerate:    true,
			Default:         ptr("nextval('orders_id_seq')"),
			Comment:         ptr("Surrogate key."),
			OrdinalPosition: 1,
		}, {
			Name:            "tags",
			DataType:        ArrayType(StringType(ptr(uint32(32)))),
			Nullable:        true,
			OrdinalPosition: 2,
		}},
		PrimaryKey: &PrimaryKey{Name: ptr("orders_pkey"), Columns: []string{"id"}},
		ForeignKeys: []ForeignKey{{
			Name:              ptr("orders_customer_fk"),
			Columns:           []string{"customer_id"},
			ReferencedTable:   "customers",
			ReferencedSchema:  ptr("public"),
			ReferencedColumns: []string{"id"},
			OnDelete:          ptr(Cascade),
			OnUpdate:          ptr(NoAction),
		}},
		Indexes: []Index{{
			Name:      "orders_placed_at_idx",
			TableName: "orders",
			Schema:    ptr("public"),
			Columns:   []IndexColumn{{Name: "placed_at", SortOrder: ptr(Descending)}},
			Unique:    true,
			IndexType: ptr("btree"),
		}},
		Constraints: []Constraint{{
			Name:           "orders_check",
			TableName:      "orders",
			Schema:         ptr("public"),
			ConstraintType: ConstraintCheck,
			Columns:        []string{"id"},
			CheckClause:    ptr("id > 0"),
		}},
		Comment:  ptr("Customer orders."),
		RowCount: ptr(uint64(10)),
	}}
	schema.Views = []View{{
		Name:       "recent_orders",
		Schema:     ptr("public"),
		Definition: ptr("SELECT 1"),
		Columns:    []Column{},
		Comment:    ptr("View comment."),
	}}
	schema.Procedures = []Routine{{
		Name:       "archive",
		Schema:     ptr("public"),
		Definition: ptr("BEGIN END"),
		Parameters: []Parameter{{
			Name:      "cutoff",
			DataType:  DateTimeType(true),
			Direction: DirectionIn,
			Default:   ptr("now()"),
		}},
		ReturnType: ptr(BooleanType()),
		Language:   ptr("plpgsql"),
		Comment:    ptr("Routine comment."),
	}}
	schema.Functions = []Routine{}
	schema.Triggers = []Trigger{{
		Name:       "orders_audit",
		TableName:  "orders",
		Schema:     ptr("public"),
		Event:      TriggerUpdate,
		Timing:     TimingInsteadOf,
		Definition: ptr("EXECUTE FUNCTION audit()"),
	}}
	schema.UserTypes = []UserType{{
		Name:       "order_state",
		Schema:     ptr("public"),
		Definition: "ENUM ('a', 'b')",
		Category:   CategoryEnum,
	}}
	schema.Samples = []TableSample{{
		TableName:        "orders",
		SchemaName:       ptr("public"),
		Rows:             []map[string]any{{"id": float64(1), "tags": []any{"x"}}},
		SampleSize:       1,
		TotalRows:        ptr(uint64(10)),
		SamplingStrategy: MostRecent(1),
		Ordering: &OrderingStrategy{
			Kind:      OrderTimestamp,
			Column:    ptr("placed_at"),
			Direction: ptr(Descending),
		},
		CollectedAt: testTime,
		Warnings:    []string{"partial"},
		Status:      ptr(PartialRetry(100)),
	}}

	schema.AggregateIndexesAndConstraints()

	return schema
}

func TestSchemaRoundTrips(t *testing.T) {
	original := populatedSchema(t)

	encoded, err := json.Marshal(original)
	require.NoError(t, err)

	var decoded Schema
	require.NoError(t, json.Unmarshal(encoded, &decoded))

	assert.Equal(t, *original, decoded)
}

func TestNewStampsFormatIdentifiers(t *testing.T) {
	schema := New(NewDatabaseInfo("db", SQLite), "0.1.0", testTime)

	assert.Equal(t, Format, schema.Format)
	assert.Equal(t, FormatVersion, schema.FormatVersion)
	assert.Equal(t, CollectionSuccess, schema.DatabaseInfo.CollectionStatus.State)
	assert.Equal(t, AccessFull, schema.DatabaseInfo.AccessLevel)
}

// TestNilOptionalsAreOmittedNotNull pins the difference between "the engine did
// not report this" and "the engine reported null", which a report renderer and
// the quality analyzers both depend on.
func TestNilOptionalsAreOmittedNotNull(t *testing.T) {
	schema := New(NewDatabaseInfo("db", SQLite), "0.1.0", testTime)

	encoded, err := json.Marshal(schema)
	require.NoError(t, err)

	var document map[string]json.RawMessage
	require.NoError(t, json.Unmarshal(encoded, &document))

	assert.NotContains(t, document, "samples", "samples must be absent when sampling did not run")

	var info map[string]json.RawMessage
	require.NoError(t, json.Unmarshal(document["database_info"], &info))

	for _, field := range []string{"version", "size_bytes", "encoding", "collation", "owner"} {
		assert.NotContains(t, info, field, "unset optional %q must be omitted, not emitted as null", field)
	}
}

func TestDecodeToleratesMissingOptionalFields(t *testing.T) {
	document := `{
		"format": "dbsurveyor/schema",
		"format_version": "1.0",
		"database_info": {
			"name": "db",
			"type": "sqlite",
			"system_database": false,
			"access_level": "full",
			"collection_status": {"state": "success"}
		},
		"tables": [], "views": [], "indexes": [], "constraints": [],
		"procedures": [], "functions": [], "triggers": [], "user_types": [],
		"collection_metadata": {
			"collected_at": "2026-07-26T12:00:00Z",
			"duration_ms": 0,
			"collector_version": "0.1.0",
			"warnings": []
		}
	}`

	var schema Schema
	require.NoError(t, json.Unmarshal([]byte(document), &schema))

	assert.Nil(t, schema.DatabaseInfo.Version)
	assert.Nil(t, schema.Samples)
	assert.Equal(t, CollectionSuccess, schema.DatabaseInfo.CollectionStatus.State)
	assert.Nil(t, schema.DatabaseInfo.CollectionStatus.Error)
}

func TestCollectionStatusEmitsOnlyItsOwnPayload(t *testing.T) {
	tests := map[string]struct {
		status  CollectionStatus
		present string
		absent  string
	}{
		"success": {CollectionSucceeded(), "", "error"},
		"failed":  {CollectionFailure("boom"), "error", "reason"},
		"skipped": {CollectionSkip("system database"), "reason", "error"},
	}

	for name, tc := range tests {
		t.Run(name, func(t *testing.T) {
			encoded, err := json.Marshal(tc.status)
			require.NoError(t, err)

			var fields map[string]any
			require.NoError(t, json.Unmarshal(encoded, &fields))

			assert.Contains(t, fields, "state")
			assert.NotContains(t, fields, tc.absent)

			if tc.present != "" {
				assert.Contains(t, fields, tc.present)
			}
		})
	}
}

func TestObjectCountSumsEveryCatalogList(t *testing.T) {
	schema := populatedSchema(t)

	// One table, one view, one index, one constraint, one procedure,
	// zero functions, one trigger, one user type.
	assert.Equal(t, 7, schema.ObjectCount())
}

func TestAggregateIndexesAndConstraintsRebuildsFromTables(t *testing.T) {
	schema := populatedSchema(t)
	schema.Indexes = nil
	schema.Constraints = nil

	schema.AggregateIndexesAndConstraints()

	require.Len(t, schema.Indexes, 1)
	require.Len(t, schema.Constraints, 1)
	assert.Equal(t, "orders_placed_at_idx", schema.Indexes[0].Name)
	assert.Equal(t, "orders_check", schema.Constraints[0].Name)
}

func TestServerSchemaRoundTrips(t *testing.T) {
	original := NewServerSchema(ServerInfo{
		ServerType:              PostgreSQL,
		Version:                 "16.2",
		Host:                    "db.internal",
		Port:                    ptr(uint16(5432)),
		TotalDatabases:          3,
		CollectedDatabases:      1,
		SystemDatabasesExcluded: 2,
		ConnectionUser:          "surveyor",
		Superuser:               true,
		CollectionMode:          MultiDatabase(3, 1, 0),
	}, "0.1.0", testTime)
	original.Databases = []Schema{*populatedSchema(t)}

	encoded, err := json.Marshal(original)
	require.NoError(t, err)

	var decoded ServerSchema
	require.NoError(t, json.Unmarshal(encoded, &decoded))

	assert.Equal(t, *original, decoded)
	assert.Equal(t, ServerFormat, decoded.Format)
}

// TestEnumsRejectUnknownValues covers every string-backed enum in the package.
//
// A new enum must be added here and to AllowedValues. Neither list is derived
// from the other, so the final assertion below checks that they cover the same
// set rather than trusting that they do.
func TestEnumsRejectUnknownValues(t *testing.T) {
	decoders := map[string]func([]byte) error{
		"DatabaseType":       func(b []byte) error { var v DatabaseType; return json.Unmarshal(b, &v) },
		"AccessLevel":        func(b []byte) error { var v AccessLevel; return json.Unmarshal(b, &v) },
		"ReferentialAction":  func(b []byte) error { var v ReferentialAction; return json.Unmarshal(b, &v) },
		"ConstraintType":     func(b []byte) error { var v ConstraintType; return json.Unmarshal(b, &v) },
		"TriggerEvent":       func(b []byte) error { var v TriggerEvent; return json.Unmarshal(b, &v) },
		"TriggerTiming":      func(b []byte) error { var v TriggerTiming; return json.Unmarshal(b, &v) },
		"TypeCategory":       func(b []byte) error { var v TypeCategory; return json.Unmarshal(b, &v) },
		"ParameterDirection": func(b []byte) error { var v ParameterDirection; return json.Unmarshal(b, &v) },
		"SortDirection":      func(b []byte) error { var v SortDirection; return json.Unmarshal(b, &v) },
		"DataTypeKind":       func(b []byte) error { var v DataTypeKind; return json.Unmarshal(b, &v) },
		"SampleState":        func(b []byte) error { var v SampleState; return json.Unmarshal(b, &v) },
		"SamplingKind":       func(b []byte) error { var v SamplingKind; return json.Unmarshal(b, &v) },
		"OrderingKind":       func(b []byte) error { var v OrderingKind; return json.Unmarshal(b, &v) },
		"CollectionState":    func(b []byte) error { var v CollectionState; return json.Unmarshal(b, &v) },
		"CollectionModeKind": func(b []byte) error { var v CollectionModeKind; return json.Unmarshal(b, &v) },
	}

	allowed := AllowedValues()

	for name, decode := range decoders {
		t.Run(name, func(t *testing.T) {
			require.Contains(t, allowed, name, "enum is not registered in AllowedValues")
			require.NotEmpty(t, allowed[name])

			for _, value := range allowed[name] {
				encoded, err := json.Marshal(value)
				require.NoError(t, err)
				assert.NoError(t, decode(encoded), "documented value %q must decode", value)
			}

			assert.Error(t, decode([]byte(`"definitely-not-a-real-value"`)),
				"unknown value must be rejected, not accepted as an opaque string")
		})
	}

	assert.Len(t, allowed, len(decoders),
		"AllowedValues and this table must cover the same enums")
}
