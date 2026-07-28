package mssql

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// TestIdentifiersAreBracketedAndTheClosingBracketIsEscaped is the trap this
// engine sets. Only `]` can end the quoting, so a helper written by analogy with
// the double-quote engines escapes the wrong character.
func TestIdentifiersAreBracketedAndTheClosingBracketIsEscaped(t *testing.T) {
	assert.Equal(t, "[orders]", quoteIdentifier("orders"))
	assert.Equal(t, "[we]]ird]", quoteIdentifier("we]ird"))
	assert.Equal(t, "[a]]]]b]", quoteIdentifier("a]]b"))
	assert.Equal(t, "[dbo].[orders]", qualify("dbo", "orders"))
	assert.Equal(t, "[orders]", qualify("", "orders"))

	// The opening bracket cannot end the quoting, so it is left alone.
	assert.Equal(t, "[we[ird]", quoteIdentifier("we[ird"))
}

// TestTheDeclaredLengthIsRecoveredFromTheByteLength is the second trap:
// sys.columns reports bytes, so an NVARCHAR(50) arrives as 100.
func TestTheDeclaredLengthIsRecoveredFromTheByteLength(t *testing.T) {
	fifty := uint32(50)

	assert.Equal(t, &fifty, characterLength("nvarchar", 100), "two bytes per character")
	assert.Equal(t, &fifty, characterLength("varchar", 50), "one byte per character")
	assert.Equal(t, &fifty, characterLength("nchar", 100))

	assert.Nil(t, characterLength("varchar", -1), "a MAX type has no declared limit")
	assert.Nil(t, characterLength("nvarchar", -1))
	assert.Nil(t, characterLength("varchar", 0))
}

func TestTypesMapToTheirEngineIndependentForm(t *testing.T) {
	precision := uint8(10)
	length := uint32(64)

	for name, want := range map[string]struct {
		column columnType
		mapped dbschema.UnifiedDataType
	}{
		"int":      {columnType{Name: "int"}, dbschema.IntegerType(32, true)},
		"bigint":   {columnType{Name: "bigint"}, dbschema.IntegerType(64, true)},
		"smallint": {columnType{Name: "smallint"}, dbschema.IntegerType(16, true)},
		"tinyint": {
			columnType{Name: "tinyint"},
			dbschema.IntegerType(8, false),
		},
		"varchar":  {columnType{Name: "varchar", MaxLength: 64}, dbschema.StringType(&length)},
		"nvarchar": {columnType{Name: "nvarchar", MaxLength: 128}, dbschema.StringType(&length)},
		"bit":      {columnType{Name: "bit"}, dbschema.BooleanType()},
		"decimal": {
			columnType{Name: "decimal", Precision: precision},
			dbschema.FloatType(&precision),
		},
		"date":     {columnType{Name: "date"}, dbschema.DateType()},
		"datetime": {columnType{Name: "datetime2"}, dbschema.DateTimeType(false)},
		"datetimeoffset": {
			columnType{Name: "datetimeoffset"},
			dbschema.DateTimeType(true),
		},
		"uniqueidentifier": {columnType{Name: "uniqueidentifier"}, dbschema.UUIDType()},
		"varbinary":        {columnType{Name: "varbinary", MaxLength: -1}, dbschema.BinaryType(nil)},
		"geography":        {columnType{Name: "geography"}, dbschema.CustomType("geography")},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.mapped, mapDataType(want.column))
		})
	}
}

// TestTinyintIsTheOnlyUnsignedInteger records the asymmetry, which is the
// opposite way round from MySQL.
func TestTinyintIsTheOnlyUnsignedInteger(t *testing.T) {
	tiny := mapDataType(columnType{Name: "tinyint"})
	require.NotNil(t, tiny.Signed)
	assert.False(t, *tiny.Signed)

	wider := mapDataType(columnType{Name: "smallint"})
	require.NotNil(t, wider.Signed)
	assert.True(t, *wider.Signed)
}

func TestReferentialActionsMapFromTheirDescriptions(t *testing.T) {
	for description, want := range map[string]dbschema.ReferentialAction{
		"NO_ACTION":   dbschema.NoAction,
		"CASCADE":     dbschema.Cascade,
		"SET_NULL":    dbschema.SetNull,
		"SET_DEFAULT": dbschema.SetDefault,
	} {
		action := referentialAction(description)
		require.NotNil(t, action, "description %q", description)
		assert.Equal(t, want, *action)
	}

	assert.Nil(t, referentialAction("SOMETHING_NEW"))
}

// TestPrimaryAndUniqueConstraintsComeFromTheIndexes records where SQL Server
// keeps them: as flags on an index, not as separate catalog entries.
func TestPrimaryAndUniqueConstraintsComeFromTheIndexes(t *testing.T) {
	key := tableKey{schema: "dbo", table: "orders"}
	set := deriveKeyConstraints(key, indexSet{indexes: []dbschema.Index{
		{Name: "PK_orders", Unique: true, Primary: true, Columns: []dbschema.IndexColumn{{Name: "id"}}},
		{Name: "UQ_orders_ref", Unique: true, Columns: []dbschema.IndexColumn{{Name: "reference"}}},
		{Name: "IX_orders_user", Columns: []dbschema.IndexColumn{{Name: "user_id"}}},
	}})

	require.NotNil(t, set.primaryKey)
	assert.Equal(t, []string{"id"}, set.primaryKey.Columns)

	require.Len(t, set.constraints, 2, "the non-unique index is an index, not a constraint")
	assert.Equal(t, dbschema.ConstraintPrimaryKey, set.constraints[0].ConstraintType)
	assert.Equal(t, dbschema.ConstraintUnique, set.constraints[1].ConstraintType)
	assert.Equal(t, []string{"reference"}, set.constraints[1].Columns)
}

// TestTriggerTimingHasNoBeforeCase records that SQL Server cannot produce one.
func TestTriggerTimingHasNoBeforeCase(t *testing.T) {
	assert.Equal(t, dbschema.TimingAfter, triggerTiming(false))
	assert.Equal(t, dbschema.TimingInsteadOf, triggerTiming(true))
}

func TestTriggerEventsMap(t *testing.T) {
	assert.Equal(t, dbschema.TriggerInsert, triggerEvent("INSERT"))
	assert.Equal(t, dbschema.TriggerUpdate, triggerEvent("UPDATE"))
	assert.Equal(t, dbschema.TriggerDelete, triggerEvent("DELETE"))
}

func TestOrderingPrefersTheMostMeaningfulSignal(t *testing.T) {
	columns := []samplingColumn{
		{name: "id", autoIncrement: true, primaryKey: true},
		{name: "created_at"},
	}

	cfg := dbadapter.DefaultSamplingConfig()
	assert.Equal(t, dbschema.OrderAutoIncrement, orderingFor(columns, cfg).Kind)

	cfg.TimestampColumns = []string{"created_at"}
	assert.Equal(t, dbschema.OrderTimestamp, orderingFor(columns, cfg).Kind)

	keyOnly := []samplingColumn{{name: "a", primaryKey: true}}
	assert.Equal(t, dbschema.OrderPrimaryKey, orderingFor(keyOnly, dbadapter.DefaultSamplingConfig()).Kind)

	assert.Equal(t, dbschema.OrderUnordered,
		orderingFor([]samplingColumn{{name: "note"}}, dbadapter.DefaultSamplingConfig()).Kind)
}

func TestTheOrderClauseBracketsAndDescendsOrIsEmpty(t *testing.T) {
	column := "created_at"

	assert.Equal(t, " ORDER BY [created_at] DESC", orderClause(dbschema.OrderingStrategy{
		Kind:   dbschema.OrderTimestamp,
		Column: &column,
	}))

	assert.Empty(t, orderClause(dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}))
}

// TestTheDataSourceNameCarriesNoCredentialOutsideItsUserinfo is the leak check
// the Secret type cannot make on its own.
func TestTheDataSourceNameCarriesNoCredentialOutsideItsUserinfo(t *testing.T) {
	const password = "p@ss:word/with?delimiters"

	cfg := dbadapter.NewConnectionConfig("db.internal")
	cfg.Database = "shop"
	cfg.Username = "surveyor"
	cfg.Password = dbadapter.NewSecret([]byte(password))

	dsn := dataSourceName(cfg)

	// The delimiters have to be escaped, or the URL truncates at the first one
	// and the driver connects with a different password than it was given.
	assert.NotContains(t, dsn, password, "the password is percent-encoded, not embedded raw")
	assert.Contains(t, dsn, "sqlserver://")
	assert.Contains(t, dsn, "database=shop")
	assert.Contains(t, dsn, "ApplicationIntent=ReadOnly")

	cfg.ReadOnly = false
	assert.NotContains(t, dataSourceName(cfg), "ApplicationIntent")
}

func TestTheVersionBannerIsReducedToItsFirstLine(t *testing.T) {
	banner := "Microsoft SQL Server 2022 (RTM-CU12)\n\tJul  1 2024 12:00:00\n\tCopyright"

	first, _, _ := strings.Cut(banner, "\n")
	assert.Equal(t, "Microsoft SQL Server 2022 (RTM-CU12)", strings.TrimSpace(first))
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
		dbadapter.TableRef{Table: "orders"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrClosed)
}

func TestFeaturesAreReported(t *testing.T) {
	adapter := &Adapter{}

	assert.Equal(t, dbschema.SQLServer, adapter.DatabaseType())
	assert.True(t, adapter.Supports(dbadapter.FeatureSchemas))
	assert.True(t, adapter.Supports(dbadapter.FeatureRoutines))
	assert.True(t, adapter.Supports(dbadapter.FeatureCustomTypes))
	assert.False(t, adapter.Supports(dbadapter.FeatureMultiDatabase))
	assert.False(t, adapter.Supports(dbadapter.FeatureSchemaInference))
}
