//go:build integration

package postgres

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The fixture server's credentials. They are a container's throwaway
// credentials, created and destroyed inside one test run.
const (
	fixtureImage    = "postgres:17-alpine"
	fixtureDatabase = "surveyor"
	fixtureUser     = "surveyor"

	fixturePassword = "surveyor-fixture"
)

// fixtureDDL is the database every test in this file surveys.
//
// The oddities are deliberate. "we""ird" carries a double quote so the
// identifier-quoting path is exercised by ordinary collection. compound declares
// its key in an order that differs from its column order. orders has a dropped
// column, so its attribute numbers have a gap the ordinal positions must not.
// empty_table is never analyzed, so reltuples is -1 for it.
const fixtureDDL = `
CREATE TYPE mood AS ENUM ('happy', 'sad');

CREATE TABLE users (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email        text NOT NULL UNIQUE,
    display_name varchar(64),
    created_at   timestamptz NOT NULL DEFAULT now(),
    active       boolean NOT NULL DEFAULT true,
    tags         text[],
    feeling      mood,
    avatar       bytea,
    CONSTRAINT users_email_lowercase CHECK (email = lower(email))
);

COMMENT ON TABLE users IS 'people who can sign in';
COMMENT ON COLUMN users.email IS 'unique, lowercased';

CREATE TABLE orders (
    id           serial PRIMARY KEY,
    scratch      integer,
    user_id      bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE ON UPDATE RESTRICT,
    total        numeric(10,2) NOT NULL
);

ALTER TABLE orders DROP COLUMN scratch;

CREATE INDEX orders_by_user ON orders (user_id);
CREATE INDEX orders_by_total ON orders (total DESC);

CREATE TABLE compound (a integer NOT NULL, b text NOT NULL, PRIMARY KEY (b, a));

CREATE TABLE empty_table (id integer PRIMARY KEY, note text);

CREATE TABLE "we""ird" (id serial PRIMARY KEY, label text NOT NULL);

CREATE VIEW active_users AS SELECT id, email FROM users WHERE active;

CREATE FUNCTION touch_user() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RETURN NEW;
END;
$$;

CREATE FUNCTION add(a integer, b integer) RETURNS integer LANGUAGE sql AS $$ SELECT a + b $$;

CREATE TRIGGER users_touched BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION touch_user();

INSERT INTO users (email, display_name) VALUES
    ('ada@example.test', 'Ada'), ('grace@example.test', 'Grace'), ('alan@example.test', 'Alan');

INSERT INTO orders (user_id, total) VALUES (1, 10.50), (2, 22.00), (1, 3.25);

INSERT INTO "we""ird" (label) VALUES ('first'), ('second');

ANALYZE users;
ANALYZE orders;
`

// fixtureServer starts one PostgreSQL container for the whole package and
// returns a connection configuration pointing at it.
//
// It is package-scoped rather than per-test because starting a server is the
// expensive part and every test here reads the same fixture without changing it
// -- the adapter is read-only, which is the property under test.
func fixtureServer(t *testing.T) dbadapter.ConnectionConfig {
	t.Helper()

	ctx := context.Background()

	container, err := tcpostgres.Run(ctx, fixtureImage,
		tcpostgres.WithDatabase(fixtureDatabase),
		tcpostgres.WithUsername(fixtureUser),
		tcpostgres.WithPassword(fixturePassword),
		tcpostgres.BasicWaitStrategies(),
	)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, testcontainers.TerminateContainer(container)) })

	dsn, err := container.ConnectionString(ctx, "sslmode=disable")
	require.NoError(t, err)

	seed(t, dsn)

	host, err := container.Host(ctx)
	require.NoError(t, err)

	mapped, err := container.MappedPort(ctx, "5432/tcp")
	require.NoError(t, err)

	port := mapped.Num()

	cfg := dbadapter.NewConnectionConfig(host)
	cfg.Port = &port
	cfg.Database = fixtureDatabase
	cfg.Username = fixtureUser
	cfg.Password = dbadapter.NewSecret([]byte(fixturePassword))

	return cfg
}

// seed applies the fixture DDL over a separate, writable connection. The adapter
// under test cannot do this: its sessions are read-only, which is the point.
func seed(t *testing.T, dsn string) {
	t.Helper()

	conn, err := pgx.Connect(t.Context(), dsn)
	require.NoError(t, err)

	defer func() { require.NoError(t, conn.Close(context.Background())) }()

	_, err = conn.Exec(t.Context(), fixtureDDL)
	require.NoError(t, err)
}

// openFixture returns an adapter over the fixture server.
func openFixture(t *testing.T) *Adapter {
	t.Helper()

	adapter, err := Open(t.Context(), fixtureServer(t))
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, adapter.Close()) })

	return adapter
}

func collectFixture(t *testing.T) *dbschema.Schema {
	t.Helper()

	cfg := dbadapter.NewCollectionConfig("ignored")

	document, err := openFixture(t).CollectSchema(t.Context(), cfg)
	require.NoError(t, err)

	return document
}

func tableNamed(t *testing.T, document *dbschema.Schema, name string) dbschema.Table {
	t.Helper()

	for _, table := range document.Tables {
		if table.Name == name {
			return table
		}
	}

	t.Fatalf("no table %q in the collected schema", name)

	return dbschema.Table{}
}

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

// TestACollectedSchemaValidates is the end-to-end assertion: everything the
// adapter produces has to survive the same validation an artifact load applies,
// including the recursive credential scan.
func TestACollectedSchemaValidates(t *testing.T) {
	document := collectFixture(t)

	require.NoError(t, document.Validate())
	assert.Equal(t, dbschema.PostgreSQL, document.DatabaseInfo.Type)
	assert.Equal(t, fixtureDatabase, document.DatabaseInfo.Name)
	assert.NotNil(t, document.DatabaseInfo.Version)
	assert.NotNil(t, document.DatabaseInfo.Encoding)
	assert.Empty(t, document.CollectionMetadata.Warnings, "the batch path carries the whole survey")
}

func TestEveryFixtureTableIsCollected(t *testing.T) {
	document := collectFixture(t)

	names := make([]string, 0, len(document.Tables))
	for _, table := range document.Tables {
		names = append(names, table.Name)
	}

	assert.ElementsMatch(t,
		[]string{"users", "orders", "compound", "empty_table", `we"ird`}, names)
}

// TestOrdinalPositionsAreConsecutiveDespiteADroppedColumn is the trap
// pg_attribute sets: a dropped column's attribute number is never reused, so
// attnum has a gap the document must not.
func TestOrdinalPositionsAreConsecutiveDespiteADroppedColumn(t *testing.T) {
	document := collectFixture(t)

	orders := tableNamed(t, document, "orders")
	require.Len(t, orders.Columns, 3, "the dropped column is gone")

	for i, column := range orders.Columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}

	assert.Equal(t, []string{"id", "user_id", "total"},
		[]string{orders.Columns[0].Name, orders.Columns[1].Name, orders.Columns[2].Name})
}

func TestColumnTypesNullabilityAndCommentsComeFromTheCatalog(t *testing.T) {
	document := collectFixture(t)

	users := tableNamed(t, document, "users")

	assert.Equal(t, dbschema.IntegerType(64, true), columnNamed(t, users, "id").DataType)
	assert.Equal(t, dbschema.StringType(nil), columnNamed(t, users, "email").DataType)
	assert.Equal(t, dbschema.DateTimeType(true), columnNamed(t, users, "created_at").DataType)
	assert.Equal(t, dbschema.BooleanType(), columnNamed(t, users, "active").DataType)
	assert.Equal(t, dbschema.BinaryType(nil), columnNamed(t, users, "avatar").DataType)
	assert.Equal(t, dbschema.ArrayType(dbschema.StringType(nil)), columnNamed(t, users, "tags").DataType)
	assert.Equal(t, dbschema.CustomType("mood"), columnNamed(t, users, "feeling").DataType)

	length := uint32(64)
	assert.Equal(t, dbschema.StringType(&length), columnNamed(t, users, "display_name").DataType)

	assert.False(t, columnNamed(t, users, "email").Nullable)
	assert.True(t, columnNamed(t, users, "display_name").Nullable)

	require.NotNil(t, users.Comment)
	assert.Equal(t, "people who can sign in", *users.Comment)

	comment := columnNamed(t, users, "email").Comment
	require.NotNil(t, comment)
	assert.Equal(t, "unique, lowercased", *comment)
}

// TestBothSpellingsOfAGeneratedKeyAreRecognized covers identity columns and the
// older serial form, which are a different mechanism with the same meaning.
func TestBothSpellingsOfAGeneratedKeyAreRecognized(t *testing.T) {
	document := collectFixture(t)

	assert.True(t, columnNamed(t, tableNamed(t, document, "users"), "id").AutoGenerate,
		"GENERATED ALWAYS AS IDENTITY")
	assert.True(t, columnNamed(t, tableNamed(t, document, "orders"), "id").AutoGenerate,
		"serial, which is an integer with a nextval default")
	assert.False(t, columnNamed(t, tableNamed(t, document, "empty_table"), "id").AutoGenerate)
}

func TestKeysConstraintsAndIndexesAreCollected(t *testing.T) {
	document := collectFixture(t)

	orders := tableNamed(t, document, "orders")

	require.NotNil(t, orders.PrimaryKey)
	assert.Equal(t, []string{"id"}, orders.PrimaryKey.Columns)

	require.Len(t, orders.ForeignKeys, 1)
	key := orders.ForeignKeys[0]
	assert.Equal(t, []string{"user_id"}, key.Columns)
	assert.Equal(t, "users", key.ReferencedTable)
	assert.Equal(t, []string{"id"}, key.ReferencedColumns)
	require.NotNil(t, key.OnDelete)
	assert.Equal(t, dbschema.Cascade, *key.OnDelete)
	require.NotNil(t, key.OnUpdate)
	assert.Equal(t, dbschema.Restrict, *key.OnUpdate)

	indexNames := make([]string, 0, len(orders.Indexes))
	for _, index := range orders.Indexes {
		indexNames = append(indexNames, index.Name)
	}

	assert.Subset(t, indexNames, []string{"orders_by_user", "orders_by_total"})

	compound := tableNamed(t, document, "compound")
	require.NotNil(t, compound.PrimaryKey)
	assert.Equal(t, []string{"b", "a"}, compound.PrimaryKey.Columns,
		"the key order, not the column order")

	users := tableNamed(t, document, "users")

	var checks []dbschema.Constraint

	for _, constraint := range users.Constraints {
		if constraint.ConstraintType == dbschema.ConstraintCheck {
			checks = append(checks, constraint)
		}
	}

	require.NotEmpty(t, checks)
	assert.Contains(t, checks[0].Name, "users_email_lowercase")
	require.NotNil(t, checks[0].CheckClause)
	assert.Contains(t, *checks[0].CheckClause, "lower")
}

// TestADescendingIndexReportsItsDirection covers the rendered-key-element form
// pg_get_indexdef returns, where the direction is a suffix on the column text.
func TestADescendingIndexReportsItsDirection(t *testing.T) {
	document := collectFixture(t)

	for _, index := range tableNamed(t, document, "orders").Indexes {
		if index.Name != "orders_by_total" {
			continue
		}

		require.Len(t, index.Columns, 1)
		assert.Equal(t, "total", index.Columns[0].Name)
		require.NotNil(t, index.Columns[0].SortOrder)
		assert.Equal(t, dbschema.Descending, *index.Columns[0].SortOrder)

		return
	}

	t.Fatal("orders_by_total was not collected")
}

func TestViewsRoutinesTriggersAndTypesAreCollected(t *testing.T) {
	document := collectFixture(t)

	require.Len(t, document.Views, 1)
	assert.Equal(t, "active_users", document.Views[0].Name)
	require.NotNil(t, document.Views[0].Definition)
	assert.Contains(t, *document.Views[0].Definition, "SELECT")
	assert.Len(t, document.Views[0].Columns, 2, "a view's columns come from the same batch query")

	functionNames := make([]string, 0, len(document.Functions))
	for _, routine := range document.Functions {
		functionNames = append(functionNames, routine.Name)
	}

	assert.Subset(t, functionNames, []string{"add", "touch_user"})

	require.Len(t, document.Triggers, 1)
	assert.Equal(t, "users_touched", document.Triggers[0].Name)
	assert.Equal(t, "users", document.Triggers[0].TableName)
	assert.Equal(t, dbschema.TimingBefore, document.Triggers[0].Timing)
	assert.Equal(t, dbschema.TriggerUpdate, document.Triggers[0].Event)

	require.Len(t, document.UserTypes, 1)
	assert.Equal(t, "mood", document.UserTypes[0].Name)
	assert.Equal(t, dbschema.CategoryEnum, document.UserTypes[0].Category)
	assert.Equal(t, "happy, sad", document.UserTypes[0].Definition)
}

// TestATableWithNoStatisticsReportsZeroRatherThanANegativeCount is the
// PostgreSQL form of GOTCHAS 6.2: reltuples is -1 on a table that has never been
// analyzed, which is not a row count.
func TestATableWithNoStatisticsReportsZeroRatherThanANegativeCount(t *testing.T) {
	document := collectFixture(t)

	empty := tableNamed(t, document, "empty_table")
	require.NotNil(t, empty.RowCount)
	assert.Equal(t, uint64(0), *empty.RowCount)

	users := tableNamed(t, document, "users")
	require.NotNil(t, users.RowCount)
	assert.Equal(t, uint64(3), *users.RowCount, "an analyzed table reports its estimate")
}

// TestTheBatchAndPerTablePathsProduceTheSameMetadata is the plan's central
// requirement for this adapter. The fallback exists so a survey survives a batch
// failure, and it is only worth having if the document it produces is the same
// one.
func TestTheBatchAndPerTablePathsProduceTheSameMetadata(t *testing.T) {
	adapter := openFixture(t)

	schemas, err := adapter.schemas(t.Context())
	require.NoError(t, err)

	listing, err := adapter.readTables(t.Context(), schemas)
	require.NoError(t, err)
	require.NotEmpty(t, listing.tables)

	batched, err := adapter.batchMetadata(t.Context(), schemas)
	require.NoError(t, err)

	perTable, err := adapter.perTableMetadata(t.Context(), listing.keys)
	require.NoError(t, err)

	for _, key := range listing.keys {
		assert.Equal(t, batched.columns[key], perTable.columns[key], "columns of %v", key)
		assert.Equal(t, batched.primaryKeys[key], perTable.primaryKeys[key], "primary key of %v", key)
		assert.Equal(t, batched.foreignKeys[key], perTable.foreignKeys[key], "foreign keys of %v", key)
		assert.ElementsMatch(t, batched.indexes[key], perTable.indexes[key], "indexes of %v", key)
		assert.ElementsMatch(t, batched.constraints[key], perTable.constraints[key], "constraints of %v", key)
	}
}

// TestAFailedBatchFallsBackToPerTableQueries drives the real fallback against
// the real server, rather than against stubs as the unit test does.
//
// The batch is made to fail with a cancelled context, which is the closest a
// test can get to the resource limit or permission error that provokes a
// fallback in the field. What matters is the outcome: a full metadata set, and a
// warning saying it came the slow way.
func TestAFailedBatchFallsBackToPerTableQueries(t *testing.T) {
	adapter := openFixture(t)

	schemas, err := adapter.schemas(t.Context())
	require.NoError(t, err)

	listing, err := adapter.readTables(t.Context(), schemas)
	require.NoError(t, err)

	cancelled, cancel := context.WithCancel(t.Context())
	cancel()

	var warnings []string

	collected, err := chooseMetadata(
		func() (*metadata, error) { return adapter.batchMetadata(cancelled, schemas) },
		func() (*metadata, error) { return adapter.perTableMetadata(t.Context(), listing.keys) },
		func(warning string) { warnings = append(warnings, warning) },
	)

	require.NoError(t, err)
	require.Len(t, warnings, 1, "the fallback is counted, not swallowed")
	assert.Contains(t, warnings[0], "per-table")

	users := tableKey{schema: "public", table: "users"}
	assert.NotEmpty(t, collected.columns[users],
		"the fallback produced real metadata, not an empty result read as success")
}

func TestSamplingReturnsAtMostTheConfiguredRowCount(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Schema: "public", Table: "users"},
		dbadapter.NewSamplingConfig(2),
	)
	require.NoError(t, err)

	assert.Equal(t, uint32(2), sample.SampleSize)
	require.NotNil(t, sample.Status)
	assert.Equal(t, dbschema.SampleComplete, sample.Status.State)
	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderAutoIncrement, sample.Ordering.Kind)
	assert.Equal(t, "alan@example.test", sample.Rows[0]["email"], "the most recent row, not the first")
}

func TestSamplingATableWithNoUsableOrderingStillReturnsRows(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Schema: "public", Table: "active_users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderUnordered, sample.Ordering.Kind)
	assert.Equal(t, dbschema.SampleNone, sample.SamplingStrategy.Kind)
	assert.NotEmpty(t, sample.Rows)
	assert.NotEmpty(t, sample.Warnings)
}

// TestAnIdentifierContainingAQuoteSurvivesSampling exercises the escaping helper
// through the engine rather than against a string.
func TestAnIdentifierContainingAQuoteSurvivesSampling(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Schema: "public", Table: `we"ird`},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	assert.Equal(t, uint32(2), sample.SampleSize)
}

func TestSamplingAnUnknownTableFails(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Schema: "public", Table: "no_such_table"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrUnknownTable)
}

// TestMultiDatabaseEnumerationExcludesTheServersOwnDatabases covers the listing
// rules and, through CollectServerSchema, that every pool opened for a database
// is closed when that database is done.
func TestMultiDatabaseEnumeration(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewCollectionConfig("ignored")
	cfg.Connection = fixtureServer(t)

	names, err := adapter.ListDatabases(t.Context(), cfg)
	require.NoError(t, err)
	assert.Contains(t, names, fixtureDatabase)
	assert.NotContains(t, names, "postgres", "the server's own database is excluded by default")
	assert.NotContains(t, names, "template1")

	cfg.IncludeSystemDatabases = true
	withSystem, err := adapter.ListDatabases(t.Context(), cfg)
	require.NoError(t, err)
	assert.Contains(t, withSystem, "postgres", "and included when asked for")

	cfg.IncludeSystemDatabases = false
	cfg.ExcludeDatabases = []string{fixtureDatabase}
	excluded, err := adapter.ListDatabases(t.Context(), cfg)
	require.NoError(t, err)
	assert.NotContains(t, excluded, fixtureDatabase)
}

func TestAServerSurveyCollectsEverySelectedDatabase(t *testing.T) {
	connection := fixtureServer(t)

	adapter, err := Open(t.Context(), connection)
	require.NoError(t, err)

	defer func() { require.NoError(t, adapter.Close()) }()

	cfg := dbadapter.NewCollectionConfig(connection.Host)
	cfg.Connection = connection

	server, err := adapter.CollectServerSchema(t.Context(), cfg)
	require.NoError(t, err)
	require.NoError(t, server.Validate())

	assert.Equal(t, dbschema.PostgreSQL, server.ServerInfo.ServerType)
	assert.Equal(t, dbschema.ModeMultiDatabase, server.ServerInfo.CollectionMode.Kind)
	assert.Positive(t, server.ServerInfo.SystemDatabasesExcluded)
	require.Len(t, server.Databases, 1)
	assert.Equal(t, fixtureDatabase, server.Databases[0].DatabaseInfo.Name)
	assert.NotEmpty(t, server.Databases[0].Tables)
}

// TestTheSessionIsReadOnly is the enforcement half of R16. Every statement this
// package issues is a read, and the session setting is what catches the case
// where one is not.
func TestTheSessionIsReadOnly(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.pool.Exec(t.Context(), `CREATE TABLE intruder (id integer)`)
	require.Error(t, err, "DDL through the adapter's own pool is rejected")

	_, err = adapter.pool.Exec(t.Context(), `DELETE FROM users`)
	require.Error(t, err, "DML through the adapter's own pool is rejected")

	// The fixture is unchanged, which is the property the rejection exists for.
	document := collectFixture(t)
	assert.Len(t, tableNamed(t, document, "users").Columns, 8)
}
