package postgres

import (
	"errors"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

func TestIdentifiersAreQuotedAndEscaped(t *testing.T) {
	assert.Equal(t, `"orders"`, quoteIdentifier("orders"))
	assert.Equal(t, `"we""ird"`, quoteIdentifier(`we"ird`))
	assert.Equal(t, `"a""""b"`, quoteIdentifier(`a""b`))
	assert.Equal(t, `"public"."orders"`, qualify("public", "orders"))
	assert.Equal(t, `"orders"`, qualify("", "orders"))
}

func TestTypesMapToTheirEngineIndependentForm(t *testing.T) {
	length := uint32(64)
	precision := uint8(10)

	for name, want := range map[string]struct {
		typeName string
		fullType string
		mapped   dbschema.UnifiedDataType
	}{
		"integer":      {"int4", "integer", dbschema.IntegerType(32, true)},
		"bigint":       {"int8", "bigint", dbschema.IntegerType(64, true)},
		"varchar":      {"varchar", "character varying(64)", dbschema.StringType(&length)},
		"text":         {"text", "text", dbschema.StringType(nil)},
		"numeric":      {"numeric", "numeric(10,2)", dbschema.FloatType(&precision)},
		"boolean":      {"bool", "boolean", dbschema.BooleanType()},
		"timestamp":    {"timestamp", "timestamp without time zone", dbschema.DateTimeType(false)},
		"timestamptz":  {"timestamptz", "timestamp with time zone", dbschema.DateTimeType(true)},
		"uuid":         {"uuid", "uuid", dbschema.UUIDType()},
		"jsonb":        {"jsonb", "jsonb", dbschema.JSONType()},
		"bytea":        {"bytea", "bytea", dbschema.BinaryType(nil)},
		"enum":         {"mood", "mood", dbschema.CustomType("mood")},
		"text array":   {"_text", "text[]", dbschema.ArrayType(dbschema.StringType(nil))},
		"int array":    {"_int4", "integer[]", dbschema.ArrayType(dbschema.IntegerType(32, true))},
		"varchar list": {"_varchar", "character varying(64)[]", dbschema.ArrayType(dbschema.StringType(&length))},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.mapped, mapDataType(want.typeName, want.fullType))
		})
	}
}

// TestIndexColumnsAreUnwrappedFromTheirRenderedForm covers what pg_get_indexdef
// actually returns, which is a rendered key element rather than a column name.
//
// The direction is a parameter rather than something parsed out of the
// rendering, because pg_index.indoption is where PostgreSQL records it. The
// pretty form of pg_get_indexdef omits the ordering options entirely -- which is
// how a DESC index came back reported as ascending, in a way no unit test over
// strings could have caught.
func TestIndexColumnsAreUnwrappedFromTheirRenderedForm(t *testing.T) {
	ascending := dbschema.Ascending
	descending := dbschema.Descending

	for name, want := range map[string]struct {
		rendered   string
		descending bool
		column     dbschema.IndexColumn
	}{
		"bare column": {
			"user_id", false,
			dbschema.IndexColumn{Name: "user_id", SortOrder: &ascending},
		},
		"quoted column": {
			`"user id"`, false,
			dbschema.IndexColumn{Name: "user id", SortOrder: &ascending},
		},
		"embedded quote": {
			`"we""ird"`, false,
			dbschema.IndexColumn{Name: `we"ird`, SortOrder: &ascending},
		},
		"descending by indoption": {
			"created_at", true,
			dbschema.IndexColumn{Name: "created_at", SortOrder: &descending},
		},
		"descending with the modifier rendered too": {
			"created_at DESC", true,
			dbschema.IndexColumn{Name: "created_at", SortOrder: &descending},
		},
		"nulls modifier trimmed": {
			"created_at NULLS LAST", false,
			dbschema.IndexColumn{Name: "created_at", SortOrder: &ascending},
		},
		"expression": {
			"lower(email)", false,
			dbschema.IndexColumn{Name: "lower(email)", SortOrder: &ascending},
		},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.column, indexColumn(want.rendered, want.descending))
		})
	}
}

func TestReferentialActionsMapFromTheirCatalogCodes(t *testing.T) {
	for code, want := range map[string]dbschema.ReferentialAction{
		"a": dbschema.NoAction,
		"r": dbschema.Restrict,
		"c": dbschema.Cascade,
		"n": dbschema.SetNull,
		"d": dbschema.SetDefault,
	} {
		action := referentialAction(code)
		require.NotNil(t, action, "code %q", code)
		assert.Equal(t, want, *action)
	}

	assert.Nil(t, referentialAction("?"), "an unrecognized code is absent, not guessed")
}

// TestTriggerFlagsUnpackIntoEveryEventTheTriggerFiresOn covers the packed tgtype
// column, which has no catalog view that unpacks it.
func TestTriggerFlagsUnpackIntoEveryEventTheTriggerFiresOn(t *testing.T) {
	insertAfter := triggerInsertBit
	assert.Equal(t, []dbschema.TriggerEvent{dbschema.TriggerInsert}, triggerEvents(insertAfter))
	assert.Equal(t, dbschema.TimingAfter, triggerTiming(insertAfter))

	updateBefore := triggerBeforeBit | triggerUpdateBit
	assert.Equal(t, []dbschema.TriggerEvent{dbschema.TriggerUpdate}, triggerEvents(updateBefore))
	assert.Equal(t, dbschema.TimingBefore, triggerTiming(updateBefore))

	all := triggerInsertBit | triggerUpdateBit | triggerDeleteBit
	assert.Equal(t, []dbschema.TriggerEvent{
		dbschema.TriggerInsert, dbschema.TriggerUpdate, dbschema.TriggerDelete,
	}, triggerEvents(all), "a trigger on three events is recorded three times, not once")

	assert.Equal(t, dbschema.TimingInsteadOf, triggerTiming(triggerInsteadOfBit|triggerUpdateBit))

	// A TRUNCATE trigger names no event the document models, and is still
	// recorded rather than silently dropped.
	assert.Len(t, triggerEvents(0), 1)
}

func TestRoutineParametersAreReadFromTheIdentityArgumentList(t *testing.T) {
	arguments := "a integer, INOUT b text, OUT c boolean, numeric"

	parameters := parseParameters(&arguments)

	require.Len(t, parameters, 4)
	assert.Equal(t, "a", parameters[0].Name)
	assert.Equal(t, dbschema.DirectionIn, parameters[0].Direction)
	assert.Equal(t, "b", parameters[1].Name)
	assert.Equal(t, dbschema.DirectionInOut, parameters[1].Direction)
	assert.Equal(t, "c", parameters[2].Name)
	assert.Equal(t, dbschema.DirectionOut, parameters[2].Direction)
	assert.Empty(t, parameters[3].Name, "a positional parameter has no name")
	assert.Equal(t, dbschema.CustomType("numeric"), parameters[3].DataType)

	assert.Empty(t, parseParameters(nil))
}

// TestTheBatchAndFallbackStatementsDifferOnlyInTheirPredicate is the property
// the equivalence of the two collection paths rests on.
//
// The plan requires that batch collection and the per-table fallback produce
// identical schemas for the same database. That holds because both paths run the
// same query text through the same scan function, differing only in how the rows
// are filtered -- so this asserts exactly that, without needing a server to
// compare two documents against.
func TestTheBatchAndFallbackStatementsDifferOnlyInTheirPredicate(t *testing.T) {
	require.Len(t, collectors, 5, "the plan's optimization is five queries, not four or six")

	for _, each := range collectors {
		t.Run(each.name, func(t *testing.T) {
			batched := withPredicate(each.template, schemaPredicate)
			perTable := withPredicate(each.template, tablePredicate)

			require.NotEqual(t, batched, perTable)
			assert.NotContains(t, batched, predicatePlaceholder, "the placeholder was substituted")
			assert.NotContains(t, perTable, predicatePlaceholder)

			assert.Equal(t,
				strings.Replace(batched, schemaPredicate, tablePredicate, 1),
				perTable,
				"the two statements differ in the predicate and nowhere else")
		})
	}
}

// TestEveryCollectorBindsItsFilterRatherThanInterpolatingIt states the rule the
// predicates exist to keep: an identifier never reaches a statement as text.
func TestEveryCollectorBindsItsFilterRatherThanInterpolatingIt(t *testing.T) {
	for _, each := range collectors {
		t.Run(each.name, func(t *testing.T) {
			assert.Contains(t, withPredicate(each.template, schemaPredicate), "$1")
			assert.Contains(t, withPredicate(each.template, tablePredicate), "$2")
		})
	}
}

// TestAFailedBatchFallsBackAndIsReported is the distinction GOTCHAS records: an
// error is not an empty result.
func TestAFailedBatchFallsBackAndIsReported(t *testing.T) {
	fallbackRan := false
	reported := []string{}

	collected, err := chooseMetadata(
		func() (*metadata, error) { return nil, errors.New("connection reset by peer") },
		func() (*metadata, error) {
			fallbackRan = true

			return newMetadata(), nil
		},
		func(warning string) { reported = append(reported, warning) },
	)

	require.NoError(t, err)
	require.NotNil(t, collected)
	assert.True(t, fallbackRan, "a failed batch is retried per table")
	require.Len(t, reported, 1, "the fallback is counted, not swallowed")
	assert.Contains(t, reported[0], "connection reset by peer")
	assert.Contains(t, reported[0], "per-table")
}

// TestAnEmptyBatchIsNotAFailure is the other half of the same distinction. A
// database with no foreign keys anywhere returns no rows, and sending that down
// the slow path would make the optimization useless on exactly the schemas it
// works best on.
func TestAnEmptyBatchIsNotAFailure(t *testing.T) {
	fallbackRan := false
	reported := 0

	collected, err := chooseMetadata(
		func() (*metadata, error) { return newMetadata(), nil },
		func() (*metadata, error) {
			fallbackRan = true

			return newMetadata(), nil
		},
		func(string) { reported++ },
	)

	require.NoError(t, err)
	assert.Empty(t, collected.columns)
	assert.False(t, fallbackRan, "an empty result is an answer, not a failure")
	assert.Zero(t, reported)
}

// TestAFailedFallbackIsAnError keeps the fallback from being a way to turn a
// broken survey into an empty one.
func TestAFailedFallbackIsAnError(t *testing.T) {
	_, err := chooseMetadata(
		func() (*metadata, error) { return nil, errors.New("batch failed") },
		func() (*metadata, error) { return nil, errors.New("fallback failed too") },
		func(string) {},
	)

	require.ErrorContains(t, err, "fallback failed too")
}

func TestOrderingPrefersTheMostMeaningfulSignal(t *testing.T) {
	columns := []samplingColumn{
		{name: "id", autoIncrement: true, primaryKey: true},
		{name: "created_at"},
		{name: "email"},
	}

	cfg := dbadapter.DefaultSamplingConfig()
	assert.Equal(t, dbschema.OrderAutoIncrement, orderingFor(columns, cfg).Kind)

	cfg.TimestampColumns = []string{"created_at"}
	assert.Equal(t, dbschema.OrderTimestamp, orderingFor(columns, cfg).Kind,
		"a named timestamp column outranks the generated key")

	keyOnly := []samplingColumn{{name: "a", primaryKey: true}, {name: "b", primaryKey: true}}
	ordering := orderingFor(keyOnly, dbadapter.DefaultSamplingConfig())
	assert.Equal(t, dbschema.OrderPrimaryKey, ordering.Kind)
	assert.Equal(t, []string{"a", "b"}, ordering.Columns)

	plain := []samplingColumn{{name: "note"}}
	assert.Equal(t, dbschema.OrderUnordered, orderingFor(plain, dbadapter.DefaultSamplingConfig()).Kind)
}

func TestTheOrderClauseQuotesAndDescendsOrIsEmpty(t *testing.T) {
	column := `we"ird`

	assert.Equal(t, ` ORDER BY "we""ird" DESC`, orderClause(dbschema.OrderingStrategy{
		Kind:   dbschema.OrderAutoIncrement,
		Column: &column,
	}))

	assert.Equal(t, ` ORDER BY "a" DESC, "b" DESC`, orderClause(dbschema.OrderingStrategy{
		Kind:    dbschema.OrderPrimaryKey,
		Columns: []string{"a", "b"},
	}))

	assert.Empty(t, orderClause(dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered}))
}

func TestSystemDatabasesAreRecognized(t *testing.T) {
	assert.True(t, isSystemDatabase("template0"))
	assert.True(t, isSystemDatabase("Template1"), "matched without regard to case")
	assert.True(t, isSystemDatabase("postgres"))
	assert.False(t, isSystemDatabase("inventory"))
}

func TestASerialDefaultIsRecognizedAsGenerated(t *testing.T) {
	assert.True(t, hasSequenceDefault("nextval('users_id_seq'::regclass)"))
	assert.True(t, hasSequenceDefault("  NEXTVAL('x')"))
	assert.False(t, hasSequenceDefault("now()"))
	assert.False(t, hasSequenceDefault("'unknown'::text"))
}

func TestOpeningRejectsAnUnusableConfiguration(t *testing.T) {
	_, err := Open(t.Context(), dbadapter.ConnectionConfig{})
	require.ErrorIs(t, err, dbadapter.ErrMissingHost)

	cfg := dbadapter.NewConnectionConfig("db.invalid")
	_, err = Open(t.Context(), cfg)
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

	_, err = adapter.CollectServerSchema(t.Context(), dbadapter.NewCollectionConfig("x"))
	require.ErrorIs(t, err, ErrClosed)
}
