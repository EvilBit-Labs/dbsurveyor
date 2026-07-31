//go:build integration

package mssql

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	tcmssql "github.com/testcontainers/testcontainers-go/modules/mssql"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The fixture server's credentials. They are a container's throwaway
// credentials, created and destroyed inside one test run. SQL Server enforces a
// password complexity rule on the sa account, which is why this one looks the
// way it does.
const (
	fixtureImage    = "mcr.microsoft.com/mssql/server:2022-latest"
	fixtureDatabase = "surveyor"
	fixtureUser     = "sa"

	fixturePassword = "Surveyor-Fixture-1"
)

// fixtureDDL is the database every test in this file surveys.
//
// The oddities are deliberate. The bracketed table name embeds a closing
// bracket, which is the only character that can end SQL Server's quoting.
// compound declares its key in an order that differs from its column order, and
// orders has a dropped column so its column_id values have a gap.
const fixtureDDL = `
CREATE TABLE users (
    id           bigint IDENTITY(1,1) NOT NULL PRIMARY KEY,
    email        nvarchar(255) NOT NULL,
    display_name varchar(64) NULL,
    created_at   datetime2 NOT NULL DEFAULT SYSUTCDATETIME(),
    active       bit NOT NULL DEFAULT 1,
    score        tinyint NOT NULL DEFAULT 0,
    avatar       varbinary(max) NULL,
    CONSTRAINT users_email_unique UNIQUE (email),
    CONSTRAINT users_score_range CHECK (score >= 0)
);

CREATE TABLE orders (
    id      int IDENTITY(1,1) NOT NULL PRIMARY KEY,
    scratch int NULL,
    user_id bigint NOT NULL,
    total   decimal(10,2) NOT NULL,
    CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES users(id)
        ON DELETE CASCADE
);

ALTER TABLE orders DROP COLUMN scratch;

CREATE INDEX orders_by_user ON orders (user_id);
CREATE INDEX orders_by_total ON orders (total DESC);

CREATE TABLE compound (a int NOT NULL, b varchar(32) NOT NULL,
    CONSTRAINT compound_pk PRIMARY KEY (b, a));

CREATE TABLE never_written (id int NOT NULL PRIMARY KEY, note nvarchar(max));

CREATE TABLE [we]]ird] (id int IDENTITY(1,1) NOT NULL PRIMARY KEY, label nvarchar(64) NOT NULL);

INSERT INTO users (email, display_name) VALUES
    ('ada@example.test', 'Ada'), ('grace@example.test', 'Grace'), ('alan@example.test', 'Alan');

INSERT INTO orders (user_id, total) VALUES (1, 10.50), (2, 22.00), (1, 3.25);

INSERT INTO [we]]ird] (label) VALUES ('first'), ('second');
`

// fixtureViews and fixtureRoutines are applied separately because SQL Server
// requires CREATE VIEW, CREATE PROCEDURE, and CREATE TRIGGER to be the first
// statement of their batch.
const (
	fixtureView    = `CREATE VIEW active_users AS SELECT id, email FROM users WHERE active = 1`
	fixtureProc    = `CREATE PROCEDURE touch_user AS BEGIN SET NOCOUNT ON; END`
	fixtureTrigger = `CREATE TRIGGER users_touched ON users AFTER UPDATE AS BEGIN SET NOCOUNT ON; END`
)

// fixtureConnection points at the container TestMain started.
var fixtureConnection dbadapter.ConnectionConfig

// TestMain starts one SQL Server container for the whole package.
//
// It is per-package rather than per-test because this image takes the better
// part of a minute to become ready, and every test here reads the same fixture
// without changing it -- which is the property under test. A container per test
// would turn a fifteen-test suite into a fifteen-minute one.
func TestMain(m *testing.M) {
	ctx := context.Background()

	container, err := tcmssql.Run(ctx, fixtureImage,
		tcmssql.WithAcceptEULA(),
		tcmssql.WithPassword(fixturePassword),
	)
	if err != nil {
		panic("start the SQL Server fixture: " + err.Error())
	}

	code := runSuite(ctx, container, m)

	if err := testcontainers.TerminateContainer(container); err != nil {
		panic("terminate the SQL Server fixture: " + err.Error())
	}

	os.Exit(code)
}

// runSuite resolves the container's address, seeds it, and runs the tests. It is
// separate from TestMain so the container is terminated even when seeding fails.
func runSuite(ctx context.Context, container *tcmssql.MSSQLServerContainer, m *testing.M) int {
	host, err := container.Host(ctx)
	if err != nil {
		panic("resolve the fixture host: " + err.Error())
	}

	mapped, err := container.MappedPort(ctx, "1433/tcp")
	if err != nil {
		panic("resolve the fixture port: " + err.Error())
	}

	port := mapped.Num()

	fixtureConnection = dbadapter.NewConnectionConfig(host)
	fixtureConnection.Port = &port
	fixtureConnection.Database = fixtureDatabase
	fixtureConnection.Username = fixtureUser
	fixtureConnection.Password = dbadapter.NewSecret([]byte(fixturePassword))

	if err := seed(ctx, fixtureConnection); err != nil {
		panic("seed the SQL Server fixture: " + err.Error())
	}

	return m.Run()
}

// seed creates and populates the fixture database over a separate, writable
// connection. The adapter under test connects with a read-only intent, and in
// any case never issues a write.
func seed(ctx context.Context, cfg dbadapter.ConnectionConfig) error {
	// The database does not exist yet, so the bootstrap connection goes to
	// master and the survey connection to the database it creates.
	bootstrap := cfg
	bootstrap.Database = "master"
	bootstrap.ReadOnly = false

	master, err := sql.Open(driverName, dataSourceName(bootstrap))
	if err != nil {
		return err
	}

	defer func() { discardError(master.Close()) }()

	if _, err := master.ExecContext(ctx, `CREATE DATABASE `+quoteIdentifier(fixtureDatabase)); err != nil {
		return err
	}

	writable := cfg
	writable.ReadOnly = false

	db, err := sql.Open(driverName, dataSourceName(writable))
	if err != nil {
		return err
	}

	defer func() { discardError(db.Close()) }()

	for _, batch := range append(splitBatches(fixtureDDL), fixtureView, fixtureProc, fixtureTrigger) {
		if _, err := db.ExecContext(ctx, batch); err != nil {
			return fmt.Errorf("seeding %q: %w", batch, err)
		}
	}

	return nil
}

// splitBatches breaks the fixture DDL into statements. The driver sends one
// batch per Exec, and a CREATE that has to lead its batch cannot share one.
func splitBatches(ddl string) []string {
	var batches []string

	for _, statement := range strings.Split(ddl, ";\n") {
		if strings.TrimSpace(statement) != "" {
			batches = append(batches, statement)
		}
	}

	return batches
}

func openFixture(t *testing.T) *Adapter {
	t.Helper()

	adapter, err := Open(t.Context(), fixtureConnection)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, adapter.Close()) })

	return adapter
}

func collectFixture(t *testing.T) *dbschema.Schema {
	t.Helper()

	document, err := openFixture(t).CollectSchema(t.Context(), dbadapter.NewCollectionConfig("ignored"))
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
	assert.Equal(t, dbschema.SQLServer, document.DatabaseInfo.Type)
	assert.Equal(t, fixtureDatabase, document.DatabaseInfo.Name)
	require.NotNil(t, document.DatabaseInfo.Version)
	assert.Contains(t, *document.DatabaseInfo.Version, "SQL Server")
}

func TestEveryFixtureTableIsCollected(t *testing.T) {
	document := collectFixture(t)

	names := make([]string, 0, len(document.Tables))
	for _, table := range document.Tables {
		names = append(names, table.Name)
	}

	assert.ElementsMatch(t,
		[]string{"users", "orders", "compound", "never_written", "we]ird"}, names)
}

// TestOrdinalPositionsAreConsecutiveDespiteADroppedColumn covers the gap
// sys.columns.column_id keeps after an ALTER.
func TestOrdinalPositionsAreConsecutiveDespiteADroppedColumn(t *testing.T) {
	orders := tableNamed(t, collectFixture(t), "orders")

	require.Len(t, orders.Columns, 3, "the dropped column is gone")

	for i, column := range orders.Columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}
}

// TestTheDeclaredLengthIsRecoveredFromTheByteLength is the trap sys.columns
// sets: an NVARCHAR(255) reports max_length 510.
func TestTheDeclaredLengthIsRecoveredThroughTheEngine(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	email := uint32(255)
	assert.Equal(t, dbschema.StringType(&email), columnNamed(t, users, "email").DataType,
		"nvarchar(255), not nvarchar(510)")

	display := uint32(64)
	assert.Equal(t, dbschema.StringType(&display), columnNamed(t, users, "display_name").DataType)

	assert.Equal(t, dbschema.BinaryType(nil), columnNamed(t, users, "avatar").DataType,
		"a MAX type has no declared limit")
}

func TestColumnTypesAndFlagsComeFromTheCatalog(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	assert.Equal(t, dbschema.IntegerType(64, true), columnNamed(t, users, "id").DataType)
	assert.Equal(t, dbschema.IntegerType(8, false), columnNamed(t, users, "score").DataType,
		"tinyint is the one unsigned integer SQL Server has")
	assert.Equal(t, dbschema.BooleanType(), columnNamed(t, users, "active").DataType)
	assert.Equal(t, dbschema.DateTimeType(false), columnNamed(t, users, "created_at").DataType)

	assert.False(t, columnNamed(t, users, "email").Nullable)
	assert.True(t, columnNamed(t, users, "display_name").Nullable)
	assert.True(t, columnNamed(t, users, "id").AutoGenerate, "IDENTITY")
	assert.NotNil(t, columnNamed(t, users, "created_at").Default)
}

func TestKeysConstraintsAndIndexesAreCollected(t *testing.T) {
	document := collectFixture(t)

	orders := tableNamed(t, document, "orders")

	require.NotNil(t, orders.PrimaryKey)
	assert.Equal(t, []string{"id"}, orders.PrimaryKey.Columns)

	require.Len(t, orders.ForeignKeys, 1)
	key := orders.ForeignKeys[0]
	assert.Equal(t, "orders_user_fk", *key.Name)
	assert.Equal(t, []string{"user_id"}, key.Columns)
	assert.Equal(t, "users", key.ReferencedTable)
	assert.Equal(t, []string{"id"}, key.ReferencedColumns)
	require.NotNil(t, key.OnDelete)
	assert.Equal(t, dbschema.Cascade, *key.OnDelete)
	assert.Nil(t, key.ReferencedSchema, "a same-schema key carries no redundant qualifier")

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

	kinds := map[dbschema.ConstraintType]int{}
	for _, constraint := range users.Constraints {
		kinds[constraint.ConstraintType]++
	}

	assert.Equal(t, 1, kinds[dbschema.ConstraintPrimaryKey])
	assert.Equal(t, 1, kinds[dbschema.ConstraintUnique])
	assert.Equal(t, 1, kinds[dbschema.ConstraintCheck])
}

func TestADescendingIndexReportsItsDirection(t *testing.T) {
	for _, index := range tableNamed(t, collectFixture(t), "orders").Indexes {
		if index.Name != "orders_by_total" {
			continue
		}

		require.Len(t, index.Columns, 1)
		require.NotNil(t, index.Columns[0].SortOrder)
		assert.Equal(t, dbschema.Descending, *index.Columns[0].SortOrder)

		return
	}

	t.Fatal("orders_by_total was not collected")
}

func TestANeverWrittenTableReportsZeroRatherThanNoEstimate(t *testing.T) {
	document := collectFixture(t)

	empty := tableNamed(t, document, "never_written")
	require.NotNil(t, empty.RowCount)
	assert.Equal(t, uint64(0), *empty.RowCount)

	users := tableNamed(t, document, "users")
	require.NotNil(t, users.RowCount)
	assert.Equal(t, uint64(3), *users.RowCount)
}

func TestViewsRoutinesAndTriggersAreCollected(t *testing.T) {
	document := collectFixture(t)

	require.Len(t, document.Views, 1)
	assert.Equal(t, "active_users", document.Views[0].Name)
	require.NotNil(t, document.Views[0].Definition)
	assert.Contains(t, *document.Views[0].Definition, "SELECT")
	assert.Len(t, document.Views[0].Columns, 2)

	require.Len(t, document.Procedures, 1)
	assert.Equal(t, "touch_user", document.Procedures[0].Name)

	require.Len(t, document.Triggers, 1)
	assert.Equal(t, "users_touched", document.Triggers[0].Name)
	assert.Equal(t, dbschema.TriggerUpdate, document.Triggers[0].Event)
	assert.Equal(t, dbschema.TimingAfter, document.Triggers[0].Timing,
		"SQL Server has no BEFORE triggers")
}

// TestAnIdentifierContainingAClosingBracketSurvivesSampling exercises the
// escaping helper through the engine rather than against a string.
func TestAnIdentifierContainingAClosingBracketSurvivesSampling(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "we]ird"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	assert.Equal(t, uint32(2), sample.SampleSize)
}

func TestSamplingReturnsAtMostTheConfiguredRowCount(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
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

func TestSamplingAnUnknownTableFails(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "no_such_table"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrUnknownTable)
}

// TestASurveyLeavesTheDatabaseUnchanged is R16 asserted against the data.
//
// SQL Server has no session-level read-only switch outside an availability
// group, so unlike PostgreSQL or SQLite there is no server-side rejection to
// test here -- the guarantee rests on this package issuing only reads, and this
// is what checks that it does.
func TestASurveyLeavesTheDatabaseUnchanged(t *testing.T) {
	adapter := openFixture(t)

	before, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("ignored"))
	require.NoError(t, err)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	after, err := adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("ignored"))
	require.NoError(t, err)

	assert.Len(t, after.Tables, len(before.Tables), "no table was created or dropped")
	assert.Equal(t,
		*tableNamed(t, before, "users").RowCount,
		*tableNamed(t, after, "users").RowCount,
		"no row was written or removed")
}
