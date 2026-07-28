package oracle

import (
	"database/sql"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

func TestIdentifiersAreDoubleQuotedAndEscaped(t *testing.T) {
	assert.Equal(t, `"ORDERS"`, quoteIdentifier("ORDERS"))
	assert.Equal(t, `"we""ird"`, quoteIdentifier(`we"ird`))
	assert.Equal(t, `"HR"."ORDERS"`, qualify("HR", "ORDERS"))
	assert.Equal(t, `"ORDERS"`, qualify("", "ORDERS"))
}

// TestUnquotedNamesAreFoldedUpwardsIsTheOracleDifference records the behavior
// that separates this engine from the other double-quote ones: PostgreSQL folds
// an unquoted identifier down, Oracle folds it up.
func TestUnquotedNamesAreFoldedUpwards(t *testing.T) {
	assert.Equal(t, "ORDERS", normalize("orders"), "an all-lowercase name was folded up on the way in")
	assert.Equal(t, "ORDERS", normalize("ORDERS"))

	// A name with any upper-case letter is either already the stored form or was
	// created quoted, and folding it would break the second case.
	assert.Equal(t, "MixedCase", normalize("MixedCase"))
	assert.Empty(t, normalize(""))
}

// TestNumberResolvesToThreeDifferentThings is the mapping that matters most on
// this engine. NUMBER is one type covering every numeric shape Oracle has, and
// collapsing all three to "float" loses what an operator most wants to know.
func TestNumberResolvesToThreeDifferentThings(t *testing.T) {
	zero := int16(0)
	two := int16(2)
	nine := uint8(9)
	ten := uint8(10)

	integer := mapDataType(columnType{Name: numberType, Precision: &nine, Scale: &zero})
	assert.Equal(t, dbschema.IntegerType(32, true), integer, "NUMBER(9,0) fits in 32 bits")

	wider := mapDataType(columnType{Name: numberType, Precision: &ten, Scale: &zero})
	assert.Equal(t, dbschema.IntegerType(64, true), wider, "NUMBER(10,0) does not")

	fixed := mapDataType(columnType{Name: numberType, Precision: &ten, Scale: &two})
	assert.Equal(t, dbschema.FloatType(&ten), fixed, "a scale makes it a decimal")

	bare := mapDataType(columnType{Name: numberType})
	assert.Equal(t, dbschema.FloatType(nil), bare, "a bare NUMBER has no declared shape at all")
}

func TestIntegerWidthsFollowTheDecimalPrecision(t *testing.T) {
	for precision, want := range map[uint8]uint8{1: 16, 4: 16, 5: 32, 9: 32, 10: 64, 38: 64} {
		digits := precision
		assert.Equal(t, want, integerBits(&digits), "precision %d", precision)
	}

	assert.Equal(t, uint8(64), integerBits(nil), "an unreported precision means up to 38 digits")
}

func TestTypesMapToTheirEngineIndependentForm(t *testing.T) {
	length := uint32(64)

	for name, want := range map[string]struct {
		column columnType
		mapped dbschema.UnifiedDataType
	}{
		"varchar2": {columnType{Name: "VARCHAR2", Length: &length}, dbschema.StringType(&length)},
		"clob":     {columnType{Name: "CLOB"}, dbschema.StringType(nil)},
		"blob":     {columnType{Name: "BLOB"}, dbschema.BinaryType(nil)},
		"date": {
			columnType{Name: "DATE"},
			// An Oracle DATE carries a time, unlike every other engine's DATE.
			dbschema.DateTimeType(false),
		},
		"timestamp": {columnType{Name: "TIMESTAMP(6)"}, dbschema.DateTimeType(false)},
		"timestamp with time zone": {
			columnType{Name: "TIMESTAMP(6) WITH TIME ZONE"},
			dbschema.DateTimeType(true),
		},
		"binary double": {columnType{Name: "BINARY_DOUBLE"}, dbschema.FloatType(nil)},
		"interval":      {columnType{Name: "INTERVAL DAY(2) TO SECOND(6)"}, dbschema.CustomType("INTERVAL DAY(2) TO SECOND(6)")},
		"xml":           {columnType{Name: "XMLTYPE"}, dbschema.CustomType("XMLTYPE")},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.mapped, mapDataType(want.column))
		})
	}
}

// TestNullPrecisionAndScaleStayAbsent is what keeps a bare NUMBER
// distinguishable from NUMBER(38,0).
func TestNullPrecisionAndScaleStayAbsent(t *testing.T) {
	bare := describeType(numberType, sql.NullInt64{}, sql.NullInt64{}, sql.NullInt64{})
	assert.Nil(t, bare.Precision)
	assert.Nil(t, bare.Scale)
	assert.Nil(t, bare.Length)

	declared := describeType(numberType,
		sql.NullInt64{},
		sql.NullInt64{Int64: 10, Valid: true},
		sql.NullInt64{Int64: 0, Valid: true})
	require.NotNil(t, declared.Precision)
	assert.Equal(t, uint8(10), *declared.Precision)
	require.NotNil(t, declared.Scale)
	assert.Equal(t, int16(0), *declared.Scale, "a zero scale is a value, not an absence")
}

// TestANotNullCheckIsNotRecordedAsAConstraint keeps the document readable.
// Oracle records every NOT NULL declaration as a check constraint, and repeating
// them would bury the checks an operator actually wrote.
func TestANotNullCheckIsNotRecordedAsAConstraint(t *testing.T) {
	assert.True(t, isNotNullCheck(sql.NullString{String: `"EMAIL" IS NOT NULL`, Valid: true}))
	assert.True(t, isNotNullCheck(sql.NullString{String: `  "ID" is not null  `, Valid: true}))
	assert.False(t, isNotNullCheck(sql.NullString{String: `LENGTH(EMAIL) > 3`, Valid: true}))
	assert.False(t, isNotNullCheck(sql.NullString{}))
}

func TestConstraintsFoldIntoTheirTable(t *testing.T) {
	referenced := map[string]referencedKey{
		"USERS_PK": {table: "USERS", columns: []string{"ID"}},
	}

	grouped := map[string]tableKeys{}

	applyConstraint(grouped, "HR", gathered{
		table:          "ORDERS",
		name:           "ORDERS_PK",
		constraintType: primaryKeyCode,
		columns:        []string{"ID"},
	}, referenced)

	applyConstraint(grouped, "HR", gathered{
		table:          "ORDERS",
		name:           "ORDERS_USER_FK",
		constraintType: foreignKeyCode,
		columns:        []string{"USER_ID"},
		referenced:     sql.NullString{String: "USERS_PK", Valid: true},
		deleteRule:     sql.NullString{String: "CASCADE", Valid: true},
	}, referenced)

	applyConstraint(grouped, "HR", gathered{
		table:          "ORDERS",
		name:           "ORDERS_NOT_NULL",
		constraintType: checkCode,
		columns:        []string{"ID"},
		condition:      sql.NullString{String: `"ID" IS NOT NULL`, Valid: true},
	}, referenced)

	orders := grouped["ORDERS"]

	require.NotNil(t, orders.primaryKey)
	assert.Equal(t, []string{"ID"}, orders.primaryKey.Columns)

	require.Len(t, orders.foreignKeys, 1)
	key := orders.foreignKeys[0]
	assert.Equal(t, "USERS", key.ReferencedTable, "resolved through the constraint it names")
	assert.Equal(t, []string{"ID"}, key.ReferencedColumns)
	require.NotNil(t, key.OnDelete)
	assert.Equal(t, dbschema.Cascade, *key.OnDelete)
	assert.Nil(t, key.OnUpdate, "Oracle has no ON UPDATE for foreign keys")

	assert.Len(t, orders.constraints, 2, "the NOT NULL check is not recorded again")
}

func TestReferentialActionsCoverOnlyWhatOracleHas(t *testing.T) {
	cascade := referentialAction(sql.NullString{String: "CASCADE", Valid: true})
	require.NotNil(t, cascade)
	assert.Equal(t, dbschema.Cascade, *cascade)

	setNull := referentialAction(sql.NullString{String: "SET NULL", Valid: true})
	require.NotNil(t, setNull)
	assert.Equal(t, dbschema.SetNull, *setNull)

	assert.Nil(t, referentialAction(sql.NullString{}))
	assert.Nil(t, referentialAction(sql.NullString{String: "RESTRICT", Valid: true}),
		"Oracle has no RESTRICT to report")
}

func TestTriggerEventsAndTimingAreReadFromTheirText(t *testing.T) {
	assert.Equal(t,
		[]dbschema.TriggerEvent{dbschema.TriggerInsert, dbschema.TriggerUpdate},
		triggerEvents("INSERT OR UPDATE"),
		"a multi-event trigger is recorded once per event")

	assert.Equal(t, []dbschema.TriggerEvent{dbschema.TriggerDelete}, triggerEvents("DELETE"))
	assert.Len(t, triggerEvents("TRUNCATE"), 1, "an unmodelled event is still recorded")

	assert.Equal(t, dbschema.TimingBefore, triggerTiming("BEFORE EACH ROW"))
	assert.Equal(t, dbschema.TimingAfter, triggerTiming("AFTER STATEMENT"))
	assert.Equal(t, dbschema.TimingInsteadOf, triggerTiming("INSTEAD OF"))
}

func TestOrderingPrefersTheMostMeaningfulSignal(t *testing.T) {
	columns := []samplingColumn{
		{name: "ID", autoIncrement: true, primaryKey: true},
		{name: "CREATED_AT"},
	}

	cfg := dbadapter.DefaultSamplingConfig()
	assert.Equal(t, dbschema.OrderAutoIncrement, orderingFor(columns, cfg).Kind)

	cfg.TimestampColumns = []string{"created_at"}
	ordering := orderingFor(columns, cfg)
	assert.Equal(t, dbschema.OrderTimestamp, ordering.Kind)
	require.NotNil(t, ordering.Column)
	assert.Equal(t, "CREATED_AT", *ordering.Column, "matched without regard to case")

	keyOnly := []samplingColumn{{name: "A", primaryKey: true}}
	assert.Equal(t, dbschema.OrderPrimaryKey, orderingFor(keyOnly, dbadapter.DefaultSamplingConfig()).Kind)

	// ROWID is deliberately not a fallback: it encodes physical placement, so a
	// "most recent" derived from it would mean nothing.
	assert.Equal(t, dbschema.OrderUnordered,
		orderingFor([]samplingColumn{{name: "NOTE"}}, dbadapter.DefaultSamplingConfig()).Kind)
}

func TestTheOrderClauseQuotesAndDescendsOrIsEmpty(t *testing.T) {
	column := "CREATED_AT"

	assert.Equal(t, ` ORDER BY "CREATED_AT" DESC`, orderClause(dbschema.OrderingStrategy{
		Kind:   dbschema.OrderTimestamp,
		Column: &column,
	}))

	assert.Empty(t, orderClause(dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}))
}

// TestTheConnectionURLCarriesNoCredentialInTheClear is the leak check the Secret
// type cannot make on its own.
func TestTheConnectionURLCarriesNoCredentialInTheClear(t *testing.T) {
	const password = "p@ss:word/with?delimiters"

	cfg := dbadapter.NewConnectionConfig("db.internal")
	cfg.Database = "ORCLPDB1"
	cfg.Username = "surveyor"
	cfg.Password = dbadapter.NewSecret([]byte(password))

	url := dataSourceName(cfg)

	assert.NotContains(t, url, password, "the password is escaped, not embedded raw")
	assert.Contains(t, url, "db.internal")
	assert.Contains(t, url, "ORCLPDB1")
}

func TestTheDefaultOwnerIsTheConnectingUserAndCanBeOverridden(t *testing.T) {
	adapter := &Adapter{owner: normalize("surveyor")}
	assert.Equal(t, "SURVEYOR", adapter.owner)

	adapter.SetOwner("hr")
	assert.Equal(t, "HR", adapter.owner, "an operator writing lower case reaches the folded schema")

	adapter.SetOwner("")
	assert.Equal(t, "HR", adapter.owner, "an empty name does not clear the selection")
}

func TestOpeningRejectsAnUnusableConfiguration(t *testing.T) {
	_, err := Open(t.Context(), dbadapter.ConnectionConfig{})
	require.ErrorIs(t, err, dbadapter.ErrMissingHost)

	_, err = Open(t.Context(), dbadapter.NewConnectionConfig("db.invalid"))
	require.ErrorIs(t, err, ErrNoService, "an Oracle listener routes on a service name")
}

func TestAClosedAdapterRefusesToWork(t *testing.T) {
	adapter := &Adapter{}

	require.NoError(t, adapter.Close(), "closing a never-opened adapter is not an error")
	require.ErrorIs(t, adapter.Ping(t.Context()), ErrClosed)

	_, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("x"))
	require.ErrorIs(t, err, ErrClosed)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "ORDERS"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrClosed)
}

func TestFeaturesAreReported(t *testing.T) {
	adapter := &Adapter{}

	assert.Equal(t, dbschema.Oracle, adapter.DatabaseType())
	assert.True(t, adapter.Supports(dbadapter.FeatureSchemas))
	assert.True(t, adapter.Supports(dbadapter.FeatureRoutines))
	assert.True(t, adapter.Supports(dbadapter.FeatureCustomTypes))
	assert.False(t, adapter.Supports(dbadapter.FeatureMultiDatabase))
	assert.False(t, adapter.Supports(dbadapter.FeatureSchemaInference))
}
