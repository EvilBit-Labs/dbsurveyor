package sqlite

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// fixtureDDL is the database every test in this file surveys.
//
// The oddities are deliberate. "we""ird" carries a double quote so the
// identifier-quoting path is exercised by ordinary collection rather than only
// by a unit test of the helper. compound declares its primary key in an order
// that differs from its column order, which is the case a naive assembly gets
// wrong. empty_table is never inserted into, so MAX(rowid) is NULL for it.
const fixtureDDL = `
CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    email         TEXT NOT NULL,
    display_name  VARCHAR(64),
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    active        BOOLEAN NOT NULL DEFAULT 1,
    avatar        BLOB,
    UNIQUE (email)
);

CREATE TABLE orders (
    id      INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE ON UPDATE RESTRICT,
    total   REAL NOT NULL
);

CREATE INDEX orders_by_user ON orders(user_id);

CREATE TABLE empty_table (id INTEGER PRIMARY KEY, note TEXT);

CREATE TABLE compound (a INTEGER NOT NULL, b TEXT NOT NULL, PRIMARY KEY (b, a));

CREATE TABLE "we""ird" (id INTEGER PRIMARY KEY, label TEXT NOT NULL);

CREATE VIEW active_users AS SELECT id, email FROM users WHERE active = 1;

CREATE TRIGGER users_audit AFTER UPDATE ON users BEGIN SELECT 1; END;

INSERT INTO users (email, display_name) VALUES
    ('ada@example.test',   'Ada'),
    ('grace@example.test', 'Grace'),
    ('alan@example.test',  'Alan');

INSERT INTO orders (user_id, total) VALUES (1, 10.5), (2, 22.0), (1, 3.25);

INSERT INTO "we""ird" (label) VALUES ('first'), ('second');
`

// newFixture writes the fixture database into a temporary directory and returns
// its path. The write happens through the driver, on a separate read-write
// connection, so nothing in this file needs a file-creation primitive that
// internal/artifact reserves.
func newFixture(t *testing.T) string {
	t.Helper()

	path := filepath.Join(t.TempDir(), "fixture.db")

	db, err := sql.Open(driverName, path)
	require.NoError(t, err)

	defer func() { require.NoError(t, db.Close()) }()

	_, err = db.ExecContext(t.Context(), fixtureDDL)
	require.NoError(t, err)

	return path
}

// openFixture returns an adapter over a fresh fixture database.
func openFixture(t *testing.T) *Adapter {
	t.Helper()

	adapter, err := Open(t.Context(), dbadapter.NewConnectionConfig(newFixture(t)))
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, adapter.Close()) })

	return adapter
}

// collectFixture surveys a fresh fixture database.
func collectFixture(t *testing.T) *dbschema.Schema {
	t.Helper()

	adapter := openFixture(t)

	schema, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig(adapter.path))
	require.NoError(t, err)

	return schema
}

// tableNamed finds a table in a collected schema.
func tableNamed(t *testing.T, schema *dbschema.Schema, name string) dbschema.Table {
	t.Helper()

	for _, table := range schema.Tables {
		if table.Name == name {
			return table
		}
	}

	t.Fatalf("no table %q in the collected schema", name)

	return dbschema.Table{}
}

// columnNamed finds a column in a collected table.
func columnNamed(t *testing.T, table dbschema.Table, name string) dbschema.Column {
	t.Helper()

	for _, column := range table.Columns {
		if column.Name == name {
			return column
		}
	}

	t.Fatalf("no column %q in table %q", name, table.Name)

	return dbschema.Column{}
}

func TestTheAdapterReportsItsEngineAndFeatures(t *testing.T) {
	adapter := openFixture(t)

	assert.Equal(t, dbschema.SQLite, adapter.DatabaseType())
	assert.True(t, adapter.Supports(dbadapter.FeatureViews))
	assert.True(t, adapter.Supports(dbadapter.FeatureTriggers))
	assert.False(t, adapter.Supports(dbadapter.FeatureSchemas), "SQLite has no schema namespace")
	assert.False(t, adapter.Supports(dbadapter.FeatureRoutines))
	assert.False(t, adapter.Supports(dbadapter.FeatureMultiDatabase))
}

// TestACollectedSchemaValidates is the end-to-end assertion: everything the
// adapter produces has to survive the same validation an artifact load applies,
// including the recursive credential scan.
func TestACollectedSchemaValidates(t *testing.T) {
	schema := collectFixture(t)

	require.NoError(t, schema.Validate())
	assert.Equal(t, dbschema.SQLite, schema.DatabaseInfo.Type)
	assert.NotNil(t, schema.DatabaseInfo.Version)
	assert.Equal(t, "fixture", schema.DatabaseInfo.Name)
}

func TestEveryFixtureTableIsCollectedAndSystemTablesAreNot(t *testing.T) {
	schema := collectFixture(t)

	names := make([]string, 0, len(schema.Tables))
	for _, table := range schema.Tables {
		names = append(names, table.Name)
	}

	assert.ElementsMatch(t, []string{"compound", "empty_table", "orders", "users", `we"ird`}, names)
}

// TestColumnsCarryOneBasedOrdinalPositions is GOTCHAS 6.4: PRAGMA table_info
// numbers columns from zero and the document requires one.
func TestColumnsCarryOneBasedOrdinalPositions(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	require.Len(t, users.Columns, 6)

	for i, column := range users.Columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}

	assert.Equal(t, "id", users.Columns[0].Name)
	assert.Equal(t, uint32(1), users.Columns[0].OrdinalPosition, "cid 0 becomes ordinal position 1")
}

func TestColumnTypesAndNullabilityComeFromTheDefinition(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	assert.Equal(t, dbschema.IntegerType(64, true), columnNamed(t, users, "id").DataType)
	assert.Equal(t, dbschema.StringType(nil), columnNamed(t, users, "email").DataType)
	assert.Equal(t, dbschema.BooleanType(), columnNamed(t, users, "active").DataType)
	assert.Equal(t, dbschema.DateTimeType(false), columnNamed(t, users, "created_at").DataType)
	assert.Equal(t, dbschema.BinaryType(nil), columnNamed(t, users, "avatar").DataType)

	length := uint32(64)
	assert.Equal(t, dbschema.StringType(&length), columnNamed(t, users, "display_name").DataType)

	assert.False(t, columnNamed(t, users, "email").Nullable)
	assert.True(t, columnNamed(t, users, "display_name").Nullable)
	assert.True(t, columnNamed(t, users, "id").PrimaryKey)
	assert.True(t, columnNamed(t, users, "id").AutoGenerate, "INTEGER PRIMARY KEY aliases the rowid")
	assert.NotNil(t, columnNamed(t, users, "created_at").Default)
}

// TestACompoundKeyIsOrderedByTheKeyNotTheTable covers the case where a table
// declares its key columns in an order that differs from the column order.
func TestACompoundKeyIsOrderedByTheKeyNotTheTable(t *testing.T) {
	compound := tableNamed(t, collectFixture(t), "compound")

	require.NotNil(t, compound.PrimaryKey)
	assert.Equal(t, []string{"b", "a"}, compound.PrimaryKey.Columns)

	assert.False(t, columnNamed(t, compound, "a").AutoGenerate,
		"a multi-column key is not an alias for the rowid")
}

func TestForeignKeysCarryTheirReferentialActions(t *testing.T) {
	orders := tableNamed(t, collectFixture(t), "orders")

	require.Len(t, orders.ForeignKeys, 1)

	key := orders.ForeignKeys[0]
	assert.Equal(t, []string{"user_id"}, key.Columns)
	assert.Equal(t, "users", key.ReferencedTable)
	assert.Equal(t, []string{"id"}, key.ReferencedColumns)

	require.NotNil(t, key.OnDelete)
	assert.Equal(t, dbschema.Cascade, *key.OnDelete)

	require.NotNil(t, key.OnUpdate)
	assert.Equal(t, dbschema.Restrict, *key.OnUpdate)
}

func TestIndexesAndConstraintsAreCollectedAndRolledUp(t *testing.T) {
	schema := collectFixture(t)

	orders := tableNamed(t, schema, "orders")
	require.Len(t, orders.Indexes, 1)
	assert.Equal(t, "orders_by_user", orders.Indexes[0].Name)
	assert.Equal(t, []dbschema.IndexColumn{{Name: "user_id"}}, orders.Indexes[0].Columns)
	assert.False(t, orders.Indexes[0].Unique)

	users := tableNamed(t, schema, "users")

	var unique []dbschema.Constraint

	for _, constraint := range users.Constraints {
		if constraint.ConstraintType == dbschema.ConstraintUnique {
			unique = append(unique, constraint)
		}
	}

	require.Len(t, unique, 1)
	assert.Equal(t, []string{"email"}, unique[0].Columns)

	// The schema-wide roll-up has to agree with the per-table lists, which is
	// what Validate checks; assert the counts directly so a failure here says
	// which side is wrong.
	assert.Len(t, schema.Indexes, countIndexes(schema))
	assert.Len(t, schema.Constraints, countConstraints(schema))
}

func countIndexes(schema *dbschema.Schema) int {
	total := 0
	for _, table := range schema.Tables {
		total += len(table.Indexes)
	}

	return total
}

func countConstraints(schema *dbschema.Schema) int {
	total := 0
	for _, table := range schema.Tables {
		total += len(table.Constraints)
	}

	return total
}

// TestAnEmptyTableReportsZeroRatherThanNoEstimate is GOTCHAS 6.2. MAX(rowid) is
// NULL for a table nothing was ever inserted into, and scanning it into a plain
// integer fails on every empty table in the database.
func TestAnEmptyTableReportsZeroRatherThanNoEstimate(t *testing.T) {
	schema := collectFixture(t)

	empty := tableNamed(t, schema, "empty_table")
	require.NotNil(t, empty.RowCount, "a NULL maximum is an estimate of zero, not an absent estimate")
	assert.Equal(t, uint64(0), *empty.RowCount)

	users := tableNamed(t, schema, "users")
	require.NotNil(t, users.RowCount)
	assert.Equal(t, uint64(3), *users.RowCount)
}

func TestViewsAndTriggersAreCollected(t *testing.T) {
	schema := collectFixture(t)

	require.Len(t, schema.Views, 1)
	assert.Equal(t, "active_users", schema.Views[0].Name)
	require.NotNil(t, schema.Views[0].Definition)
	assert.Contains(t, *schema.Views[0].Definition, "CREATE VIEW")
	assert.Len(t, schema.Views[0].Columns, 2)

	require.Len(t, schema.Triggers, 1)
	assert.Equal(t, "users_audit", schema.Triggers[0].Name)
	assert.Equal(t, "users", schema.Triggers[0].TableName)
	assert.Equal(t, dbschema.TimingAfter, schema.Triggers[0].Timing)
	assert.Equal(t, dbschema.TriggerUpdate, schema.Triggers[0].Event)
}

func TestViewsAndTriggersAreSkippedWhenNotRequested(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewCollectionConfig(adapter.path)
	cfg.IncludeViews = false
	cfg.IncludeTriggers = false

	schema, err := adapter.CollectSchema(t.Context(), cfg)
	require.NoError(t, err)

	assert.Empty(t, schema.Views)
	assert.Empty(t, schema.Triggers)
}

// TestAnIdentifierContainingAQuoteIsEscapedForDML asserts the escaping rule
// directly, so a regression is reported here rather than as a syntax error in
// whichever query happened to use it first.
func TestAnIdentifierContainingAQuoteIsEscapedForDML(t *testing.T) {
	assert.Equal(t, `"we""ird"`, quoteIdentifier(`we"ird`))
	assert.Equal(t, `"plain"`, quoteIdentifier("plain"))
	assert.Equal(t, `"a""""b"`, quoteIdentifier(`a""b`))
	assert.Equal(t, `"main"."t"`, qualify("main", "t"))
}

// TestAnIdentifierContainingAQuoteSurvivesCollectionAndSampling is the same rule
// asserted through the engine rather than against a string.
//
// The two paths reach the table name differently: collection binds it as a
// parameter to a pragma_* table-valued function, and sampling interpolates it as
// a quoted identifier. They are asserted separately because only one of them can
// be got wrong by the escaping helper, and a test that only collected would pass
// with the helper deleted.
func TestAnIdentifierContainingAQuoteSurvivesCollectionAndSampling(t *testing.T) {
	adapter := openFixture(t)

	schema, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig(adapter.path))
	require.NoError(t, err)

	weird := tableNamed(t, schema, `we"ird`)
	assert.Len(t, weird.Columns, 2, "the bound pragma parameter reached the right table")

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: `we"ird`},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)
	assert.Equal(t, uint32(2), sample.SampleSize, "the quoted identifier reached the right table")
}

func TestSamplingReturnsAtMostTheConfiguredRowCount(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewSamplingConfig(2)

	sample, err := adapter.SampleTable(t.Context(), dbadapter.TableRef{Table: "users"}, cfg)
	require.NoError(t, err)

	assert.Equal(t, uint32(2), sample.SampleSize)
	assert.Len(t, sample.Rows, 2)
	require.NotNil(t, sample.Status)
	assert.Equal(t, dbschema.SampleComplete, sample.Status.State)
	assert.Equal(t, "grace@example.test", sample.Rows[1]["email"], "text is reported as text, not base64")
}

// TestSamplingTakesTheNewestRowsUnderTheDetectedOrdering checks that the
// ordering strategy is applied rather than merely reported.
func TestSamplingTakesTheNewestRowsUnderTheDetectedOrdering(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.NewSamplingConfig(1),
	)
	require.NoError(t, err)

	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderAutoIncrement, sample.Ordering.Kind)
	require.Len(t, sample.Rows, 1)
	assert.Equal(t, "alan@example.test", sample.Rows[0]["email"], "the most recent row, not the first")
}

func TestAConfiguredTimestampColumnWinsOverTheKey(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewSamplingConfig(2)
	cfg.TimestampColumns = []string{"CREATED_AT"}

	sample, err := adapter.SampleTable(t.Context(), dbadapter.TableRef{Table: "users"}, cfg)
	require.NoError(t, err)

	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderTimestamp, sample.Ordering.Kind)
	require.NotNil(t, sample.Ordering.Column)
	assert.Equal(t, "created_at", *sample.Ordering.Column, "matched without regard to case")
}

// TestATableWithNoUsableOrderingStillReturnsRows covers the fallback. A view has
// neither a primary key nor a rowid, so there is nothing to order by, and the
// sample says so rather than failing.
func TestATableWithNoUsableOrderingStillReturnsRows(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "active_users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderUnordered, sample.Ordering.Kind)
	assert.Equal(t, dbschema.SampleNone, sample.SamplingStrategy.Kind)
	assert.NotEmpty(t, sample.Rows)
	assert.NotEmpty(t, sample.Warnings, "an unordered sample says that it is one")
}

// TestSamplingAnUnknownTableFails is the adapter form of the empty-result trap:
// PRAGMA table_info reports a table that does not exist as zero rows, and a
// reader that took that for success would report a table with no columns.
func TestSamplingAnUnknownTableFails(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "no_such_table"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrUnknownTable)
}

func TestSamplingRejectsAnEmptyTableName(t *testing.T) {
	_, err := openFixture(t).SampleTable(
		t.Context(),
		dbadapter.TableRef{},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, dbadapter.ErrEmptyTableName)
}

// TestSensitiveValuesAreReportedByColumnAndRuleOnly checks that a warning names
// where and which rule, never what. A warning quoting the value it matched would
// put the thing it detected into the document.
func TestSensitiveValuesAreReportedByColumnAndRuleOnly(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewSamplingConfig(10)
	cfg.WarnSensitive = true
	cfg.SensitivePatterns = []dbadapter.SensitivePattern{
		{Pattern: `@example\.test$`, Description: "a reserved-domain address"},
	}

	sample, err := adapter.SampleTable(t.Context(), dbadapter.TableRef{Table: "users"}, cfg)
	require.NoError(t, err)

	require.Len(t, sample.Warnings, 1)
	assert.Contains(t, sample.Warnings[0], "email")
	assert.Contains(t, sample.Warnings[0], "a reserved-domain address")
	assert.NotContains(t, sample.Warnings[0], "ada@example.test")
}

// TestAReadOnlyConnectionRefusesToWrite is the enforcement half of R16. Every
// statement this package issues is a read, and the pragma is what catches the
// case where one is not.
func TestAReadOnlyConnectionRefusesToWrite(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.db.ExecContext(t.Context(), `CREATE TABLE intruder (id INTEGER)`)
	require.Error(t, err, "DDL through the adapter's own connection is rejected")

	_, err = adapter.db.ExecContext(t.Context(), `DELETE FROM users`)
	require.Error(t, err, "DML through the adapter's own connection is rejected")
}

// TestAWritableConnectionIsAvailableWhenAsked keeps the previous test honest: it
// fails if query_only were somehow rejecting everything for an unrelated reason.
func TestAWritableConnectionIsAvailableWhenAsked(t *testing.T) {
	cfg := dbadapter.NewConnectionConfig(newFixture(t))
	cfg.ReadOnly = false

	db, err := sql.Open(driverName, dataSourceName(cfg))
	require.NoError(t, err)

	defer func() { require.NoError(t, db.Close()) }()

	_, err = db.ExecContext(t.Context(), `CREATE TABLE intruder (id INTEGER)`)
	require.NoError(t, err, "the same statement succeeds without the read-only pragma")
}

// TestCollectionLeavesTheDatabaseFileUnchanged is R16 stated as a property of
// the bytes rather than of the statements: a full survey plus sampling must not
// modify the file it read.
func TestCollectionLeavesTheDatabaseFileUnchanged(t *testing.T) {
	path := newFixture(t)

	before := digest(t, path)

	adapter, err := Open(t.Context(), dbadapter.NewConnectionConfig(path))
	require.NoError(t, err)

	_, err = adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig(path))
	require.NoError(t, err)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	require.NoError(t, adapter.Close())

	assert.Equal(t, before, digest(t, path), "the database file is byte-identical after a survey")
}

func digest(t *testing.T, path string) [sha256.Size]byte {
	t.Helper()

	contents, err := os.ReadFile(path)
	require.NoError(t, err)

	return sha256.Sum256(contents)
}

func TestAClosedAdapterRefusesToWork(t *testing.T) {
	adapter, err := Open(t.Context(), dbadapter.NewConnectionConfig(newFixture(t)))
	require.NoError(t, err)

	require.NoError(t, adapter.Close())
	require.NoError(t, adapter.Close(), "closing twice is not an error")

	require.ErrorIs(t, adapter.Ping(t.Context()), ErrClosed)

	_, err = adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("x"))
	require.ErrorIs(t, err, ErrClosed)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrClosed)
}

func TestOpeningAMissingFileFails(t *testing.T) {
	cfg := dbadapter.NewConnectionConfig(filepath.Join(t.TempDir(), "absent.db"))

	_, err := Open(t.Context(), cfg)
	require.Error(t, err, "read-only mode does not create the file it was pointed at")
}

func TestOpeningRejectsAnInvalidConfiguration(t *testing.T) {
	_, err := Open(context.Background(), dbadapter.ConnectionConfig{})
	require.ErrorIs(t, err, dbadapter.ErrMissingHost)
}

func TestCollectionRejectsAnInvalidConfiguration(t *testing.T) {
	_, err := openFixture(t).CollectSchema(t.Context(), dbadapter.CollectionConfig{})
	require.ErrorIs(t, err, dbadapter.ErrMissingHost)
}

// TestADanglingViewDoesNotAbortTheSurvey pins the demotion in collectViews.
//
// SQLite resolves a view's columns by planning its SELECT, so a view over a
// dropped table fails there even though sqlite_master still lists the view.
// Before this was demoted to a warning, one such view denied the operator the
// entire schema.
func TestADanglingViewDoesNotAbortTheSurvey(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "dangling.db")

	db, err := sql.Open(driverName, path)
	require.NoError(t, err)

	for _, statement := range []string{
		`CREATE TABLE kept (id INTEGER PRIMARY KEY, label TEXT)`,
		`CREATE TABLE doomed (id INTEGER)`,
		`CREATE VIEW dangling AS SELECT id FROM doomed`,
		`DROP TABLE doomed`,
	} {
		_, execErr := db.ExecContext(t.Context(), statement)
		require.NoError(t, execErr, statement)
	}

	require.NoError(t, db.Close())

	adapter, err := Open(t.Context(), dbadapter.NewConnectionConfig(path))
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, adapter.Close()) })

	schema, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig(path))
	require.NoError(t, err, "one unresolvable view must not fail the whole survey")

	// The table beside it is still reported in full.
	kept := tableNamed(t, schema, "kept")
	assert.Len(t, kept.Columns, 2)

	// The view is kept, without columns, and the reason is recorded.
	require.Len(t, schema.Views, 1)
	assert.Equal(t, "dangling", schema.Views[0].Name)
	assert.Empty(t, schema.Views[0].Columns)

	assert.Contains(t, strings.Join(schema.CollectionMetadata.Warnings, "\n"), `no columns for view "dangling"`)
}

// TestAWithoutRowidTableIsSurveyedWithoutARowEstimate covers GOTCHAS 6.2 at the
// unit level.
//
// A WITHOUT ROWID table has no rowid to take a maximum of, so the row-count
// query errors rather than returning NULL. Both the collector and the sampler
// carry code written for that case; before this test neither had ever run
// against one.
func TestAWithoutRowidTableIsSurveyedWithoutARowEstimate(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "norowid.db")

	db, err := sql.Open(driverName, path)
	require.NoError(t, err)

	_, err = db.ExecContext(t.Context(),
		`CREATE TABLE keyed (code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID`)
	require.NoError(t, err)

	_, err = db.ExecContext(t.Context(), `INSERT INTO keyed VALUES ('a', 'first'), ('b', 'second')`)
	require.NoError(t, err)
	require.NoError(t, db.Close())

	adapter, err := Open(t.Context(), dbadapter.NewConnectionConfig(path))
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, adapter.Close()) })

	schema, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig(path))
	require.NoError(t, err, "a WITHOUT ROWID table must not fail the survey")

	keyed := tableNamed(t, schema, "keyed")
	assert.Len(t, keyed.Columns, 2)
	assert.Nil(t, keyed.RowCount, "there is no rowid to estimate from")
	assert.Contains(t, strings.Join(schema.CollectionMetadata.Warnings, "\n"), `no row estimate for table "keyed"`)

	// Sampling still works: it must not depend on the rowid either.
	sample, err := adapter.SampleTable(t.Context(),
		dbadapter.TableRef{Table: "keyed"}, dbadapter.NewSamplingConfig(10))
	require.NoError(t, err)
	assert.Len(t, sample.Rows, 2)
}
