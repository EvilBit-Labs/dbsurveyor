package mysql

import (
	"database/sql"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

func TestIdentifiersAreQuotedWithBackticksAndEscaped(t *testing.T) {
	assert.Equal(t, "`orders`", quoteIdentifier("orders"))
	assert.Equal(t, "`we``ird`", quoteIdentifier("we`ird"))
	assert.Equal(t, "`shop`.`orders`", qualify("shop", "orders"))
	assert.Equal(t, "`orders`", qualify("", "orders"))

	// A double quote is not the escape character here. Passing a MySQL name
	// through another engine's helper would leave it unescaped, which is why
	// each adapter carries its own.
	assert.Equal(t, "`we\"ird`", quoteIdentifier(`we"ird`))
}

// TestSignednessComesFromTheFullTypeNotTheDataType is the trap
// INFORMATION_SCHEMA sets: DATA_TYPE reports "int" for a signed and an unsigned
// column alike, and only COLUMN_TYPE carries the difference.
func TestSignednessComesFromTheFullTypeNotTheDataType(t *testing.T) {
	signed := mapDataType(columnType{DataType: "int", FullType: "int(11)"})
	assert.Equal(t, dbschema.IntegerType(32, true), signed)

	unsigned := mapDataType(columnType{DataType: "int", FullType: "int(10) unsigned"})
	assert.Equal(t, dbschema.IntegerType(32, false), unsigned)

	assert.Equal(t, dbschema.IntegerType(64, false),
		mapDataType(columnType{DataType: "bigint", FullType: "bigint unsigned"}))
}

// TestBooleanIsRecoveredFromItsAlias records that MySQL has no boolean type. A
// column of ones and zeroes reported as an 8-bit integer tells a reader nothing
// about what the values mean.
func TestBooleanIsRecoveredFromItsAlias(t *testing.T) {
	assert.Equal(t, dbschema.BooleanType(),
		mapDataType(columnType{DataType: "tinyint", FullType: "tinyint(1)"}))

	assert.Equal(t, dbschema.IntegerType(8, true),
		mapDataType(columnType{DataType: "tinyint", FullType: "tinyint(4)"}),
		"a wider tinyint is an integer, not a flag")
}

func TestTypesMapToTheirEngineIndependentForm(t *testing.T) {
	length := uint32(64)
	precision := uint8(10)

	for name, want := range map[string]struct {
		column columnType
		mapped dbschema.UnifiedDataType
	}{
		"varchar": {
			columnType{DataType: "varchar", FullType: "varchar(64)", MaxLength: &length},
			dbschema.StringType(&length),
		},
		"text":     {columnType{DataType: "text", FullType: "text"}, dbschema.StringType(nil)},
		"blob":     {columnType{DataType: "blob", FullType: "blob"}, dbschema.BinaryType(nil)},
		"json":     {columnType{DataType: "json", FullType: "json"}, dbschema.JSONType()},
		"date":     {columnType{DataType: "date", FullType: "date"}, dbschema.DateType()},
		"datetime": {columnType{DataType: "datetime", FullType: "datetime"}, dbschema.DateTimeType(false)},
		"timestamp": {
			columnType{DataType: "timestamp", FullType: "timestamp"},
			dbschema.DateTimeType(true),
		},
		"decimal": {
			columnType{DataType: "decimal", FullType: "decimal(10,2)", Precision: &precision},
			dbschema.FloatType(&precision),
		},
		"year":     {columnType{DataType: "year", FullType: "year(4)"}, dbschema.IntegerType(16, true)},
		"geometry": {columnType{DataType: "geometry", FullType: "geometry"}, dbschema.CustomType("geometry")},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.mapped, mapDataType(want.column))
		})
	}
}

// TestNullableCatalogValuesAreDescribedRatherThanAssumed covers the nullable
// length and precision columns, which are NULL for every type that has neither.
func TestNullableCatalogValuesAreDescribedRatherThanAssumed(t *testing.T) {
	described := describeType("int", "int(11)", sql.NullInt64{}, sql.NullInt64{Int64: 10, Valid: true})
	assert.Nil(t, described.MaxLength)
	require.NotNil(t, described.Precision)
	assert.Equal(t, uint8(10), *described.Precision)

	// A length wider than the field it maps to is dropped rather than truncated
	// into a smaller, wrong number.
	huge := describeType("text", "longtext", sql.NullInt64{Int64: 1 << 40, Valid: true}, sql.NullInt64{})
	assert.Nil(t, huge.MaxLength)
}

func TestIndexSortDirectionComesFromTheCollationColumn(t *testing.T) {
	ascending := sortDirection(sql.NullString{String: "A", Valid: true})
	require.NotNil(t, ascending)
	assert.Equal(t, dbschema.Ascending, *ascending)

	descending := sortDirection(sql.NullString{String: "D", Valid: true})
	require.NotNil(t, descending)
	assert.Equal(t, dbschema.Descending, *descending)

	assert.Nil(t, sortDirection(sql.NullString{}), "a hash index has no direction to report")
}

// TestReferentialActionIsAbsentForANonForeignKey records why the nullable form
// matters: the rule columns are NULL for the primary-key and unique rows the
// same query returns, and a nil action there means "not a foreign key".
func TestReferentialActionIsAbsentForANonForeignKey(t *testing.T) {
	assert.Nil(t, referentialAction(sql.NullString{}))
	assert.Nil(t, referentialAction(sql.NullString{String: "SOMETHING NEW", Valid: true}))

	cascade := referentialAction(sql.NullString{String: "CASCADE", Valid: true})
	require.NotNil(t, cascade)
	assert.Equal(t, dbschema.Cascade, *cascade)
}

// TestConstraintsFoldIntoTheirTable covers the assembly the ordered catalog rows
// are turned into, including a compound key whose column order is its own.
func TestConstraintsFoldIntoTheirTable(t *testing.T) {
	grouped := map[string]tableKeys{}

	applyConstraint(grouped, "shop", keyRow{
		table:          "orders",
		constraint:     "PRIMARY",
		constraintType: primaryKeyConstraint,
	}, []string{"b", "a"}, nil)

	applyConstraint(grouped, "shop", keyRow{
		table:          "orders",
		constraint:     "orders_user_fk",
		constraintType: foreignKeyConstraint,
		parentSchema:   sql.NullString{String: "shop", Valid: true},
		parentTable:    sql.NullString{String: "users", Valid: true},
		onDelete:       sql.NullString{String: "CASCADE", Valid: true},
	}, []string{"user_id"}, []string{"id"})

	orders := grouped["orders"]

	require.NotNil(t, orders.primaryKey)
	assert.Equal(t, []string{"b", "a"}, orders.primaryKey.Columns, "the key order, not the column order")

	require.Len(t, orders.foreignKeys, 1)
	assert.Equal(t, "users", orders.foreignKeys[0].ReferencedTable)
	assert.Equal(t, []string{"id"}, orders.foreignKeys[0].ReferencedColumns)
	assert.Nil(t, orders.foreignKeys[0].ReferencedSchema,
		"a same-database key carries no redundant qualifier")

	assert.Len(t, orders.constraints, 2)
}

func TestACrossDatabaseForeignKeyRecordsItsParentDatabase(t *testing.T) {
	key := foreignKey("shop", keyRow{
		table:        "orders",
		constraint:   "orders_catalog_fk",
		parentSchema: sql.NullString{String: "catalog", Valid: true},
		parentTable:  sql.NullString{String: "products", Valid: true},
	}, []string{"product_id"}, []string{"id"})

	require.NotNil(t, key.ReferencedSchema)
	assert.Equal(t, "catalog", *key.ReferencedSchema)
}

func TestTriggerEventAndTimingMap(t *testing.T) {
	assert.Equal(t, dbschema.TriggerUpdate, triggerEvent("UPDATE"))
	assert.Equal(t, dbschema.TriggerDelete, triggerEvent("delete"))
	assert.Equal(t, dbschema.TriggerInsert, triggerEvent("INSERT"))
	assert.Equal(t, dbschema.TimingAfter, triggerTiming("AFTER"))
	assert.Equal(t, dbschema.TimingBefore, triggerTiming("BEFORE"))
}

func TestSystemDatabasesAreRecognized(t *testing.T) {
	assert.True(t, isSystemDatabase("mysql"))
	assert.True(t, isSystemDatabase("INFORMATION_SCHEMA"), "matched without regard to case")
	assert.True(t, isSystemDatabase("performance_schema"))
	assert.True(t, isSystemDatabase("sys"))
	assert.False(t, isSystemDatabase("shop"))
}

func TestOrderingPrefersTheMostMeaningfulSignal(t *testing.T) {
	columns := []samplingColumn{
		{name: "id", autoIncrement: true, primaryKey: true},
		{name: "created_at"},
	}

	cfg := dbadapter.DefaultSamplingConfig()
	assert.Equal(t, dbschema.OrderAutoIncrement, orderingFor(columns, cfg).Kind)

	cfg.TimestampColumns = []string{"CREATED_AT"}
	ordering := orderingFor(columns, cfg)
	assert.Equal(t, dbschema.OrderTimestamp, ordering.Kind)
	require.NotNil(t, ordering.Column)
	assert.Equal(t, "created_at", *ordering.Column, "matched without regard to case")

	keyOnly := []samplingColumn{{name: "a", primaryKey: true}, {name: "b", primaryKey: true}}
	assert.Equal(t, dbschema.OrderPrimaryKey, orderingFor(keyOnly, dbadapter.DefaultSamplingConfig()).Kind)

	assert.Equal(t, dbschema.OrderUnordered,
		orderingFor([]samplingColumn{{name: "note"}}, dbadapter.DefaultSamplingConfig()).Kind)
}

func TestTheOrderClauseQuotesAndDescendsOrIsEmpty(t *testing.T) {
	column := "created_at"

	assert.Equal(t, " ORDER BY `created_at` DESC", orderClause(dbschema.OrderingStrategy{
		Kind:   dbschema.OrderTimestamp,
		Column: &column,
	}))

	assert.Empty(t, orderClause(dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}))
}

// TestTheReadOnlySessionVariableIsSetOnConnect checks the second line of defense
// is actually requested, since nothing else in a unit test can observe it.
func TestTheReadOnlySessionVariableIsSetOnConnect(t *testing.T) {
	cfg := dbadapter.NewConnectionConfig("db.internal")
	cfg.Database = "shop"

	assert.Equal(t, "1", dataSourceConfig(cfg).Params["transaction_read_only"])

	cfg.ReadOnly = false
	assert.NotContains(t, dataSourceConfig(cfg).Params, "transaction_read_only")
}

// TestTheDriverConfigurationCarriesNoCredentialInItsAddress is the leak check
// the Secret type cannot make on its own: the password reaches the driver's own
// field and nothing else.
func TestTheDriverConfigurationCarriesNoCredentialInItsAddress(t *testing.T) {
	const password = "hunter2-the-actual-password"

	cfg := dbadapter.NewConnectionConfig("db.internal")
	cfg.Database = "shop"
	cfg.Username = "surveyor"
	cfg.Password = dbadapter.NewSecret([]byte(password))

	config := dataSourceConfig(cfg)

	assert.Equal(t, password, config.Passwd, "the driver still receives it")
	assert.NotContains(t, config.Addr, password)
	assert.NotContains(t, config.DBName, password)
	assert.NotContains(t, config.User, password)
}

func TestOpeningRejectsAnUnusableConfiguration(t *testing.T) {
	_, err := Open(t.Context(), dbadapter.ConnectionConfig{})
	require.ErrorIs(t, err, dbadapter.ErrMissingHost)

	_, err = Open(t.Context(), dbadapter.NewConnectionConfig("db.invalid"))
	require.ErrorIs(t, err, ErrNoDatabase, "a survey needs a database to survey")
}

func TestAClosedAdapterRefusesToWork(t *testing.T) {
	adapter := &Adapter{}

	require.NoError(t, adapter.Close(), "closing a never-opened adapter is not an error")
	require.ErrorIs(t, adapter.Ping(t.Context()), ErrClosed)

	_, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("x"))
	require.ErrorIs(t, err, ErrClosed)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrClosed)

	_, err = adapter.ListDatabases(t.Context(), dbadapter.NewCollectionConfig("x"))
	require.ErrorIs(t, err, ErrClosed)
}

func TestFeaturesAreReported(t *testing.T) {
	adapter := &Adapter{}

	assert.Equal(t, dbschema.MySQL, adapter.DatabaseType())
	assert.True(t, adapter.Supports(dbadapter.FeatureViews))
	assert.True(t, adapter.Supports(dbadapter.FeatureRoutines))
	assert.True(t, adapter.Supports(dbadapter.FeatureMultiDatabase))
	assert.False(t, adapter.Supports(dbadapter.FeatureCustomTypes))
	assert.False(t, adapter.Supports(dbadapter.FeatureSchemaInference))
}
