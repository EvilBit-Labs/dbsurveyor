//go:build integration

package oracle

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/wait"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The fixture server. There is no testcontainers module for Oracle, so the
// generic container API is used with Oracle's own free image, which is the
// image Oracle publishes for exactly this purpose.
const (
	fixtureImage   = "gvenzl/oracle-free:23-slim-faststart"
	fixtureService = "FREEPDB1"
	fixtureUser    = "system"

	fixturePassword = "surveyor-fixture"
	// The image prints this once the database has finished opening. Waiting on a
	// port would return long before the listener accepts a login.
	readyLogLine = "DATABASE IS READY TO USE!"
)

// fixtureDDL is the schema every test in this file surveys.
//
// The NUMBER columns are the point: NUMBER(9,0), NUMBER(10,0), NUMBER(10,2), and
// a bare NUMBER are four different things that this engine spells with one type
// name, and an adapter that collapsed them would lose what an operator most
// wants from a numeric column.
const fixtureDDL = `
CREATE TABLE users (
    id           NUMBER(10,0) GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email        VARCHAR2(255) NOT NULL,
    display_name VARCHAR2(64),
    created_at   TIMESTAMP(6) WITH TIME ZONE DEFAULT SYSTIMESTAMP NOT NULL,
    small_count  NUMBER(9,0),
    amount       NUMBER(10,2),
    unbounded    NUMBER,
    payload      BLOB,
    CONSTRAINT users_email_unique UNIQUE (email),
    CONSTRAINT users_email_length CHECK (LENGTH(email) > 3)
)
~
COMMENT ON TABLE users IS 'people who can sign in'
~
CREATE TABLE orders (
    id      NUMBER(10,0) GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id NUMBER(10,0) NOT NULL,
    total   NUMBER(10,2) NOT NULL,
    CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
)
~
CREATE INDEX orders_by_user ON orders (user_id)
~
CREATE TABLE compound (a NUMBER(5,0) NOT NULL, b VARCHAR2(32) NOT NULL,
    CONSTRAINT compound_pk PRIMARY KEY (b, a))
~
CREATE TABLE never_written (id NUMBER(10,0) PRIMARY KEY, note VARCHAR2(100))
~
CREATE VIEW active_users AS SELECT id, email FROM users
~
CREATE TRIGGER users_touched BEFORE UPDATE ON users FOR EACH ROW
BEGIN
    NULL;
END;
~
INSERT INTO users (email, display_name) VALUES ('ada@example.test', 'Ada')
~
INSERT INTO users (email, display_name) VALUES ('grace@example.test', 'Grace')
~
INSERT INTO users (email, display_name) VALUES ('alan@example.test', 'Alan')
~
INSERT INTO orders (user_id, total) VALUES (1, 10.50)
~
INSERT INTO orders (user_id, total) VALUES (2, 22.00)
~
COMMIT
`

// fixtureConnection points at the container TestMain started.
var fixtureConnection dbadapter.ConnectionConfig

// TestMain starts one Oracle container for the whole package.
//
// It is per-package because this image is large and takes minutes to open the
// database, and every test here reads the same fixture without changing it --
// which is the property under test.
func TestMain(m *testing.M) {
	ctx := context.Background()

	container, err := testcontainers.Run(ctx, fixtureImage,
		testcontainers.WithExposedPorts("1521/tcp"),
		testcontainers.WithEnv(map[string]string{"ORACLE_PASSWORD": fixturePassword}),
		testcontainers.WithWaitStrategy(
			wait.ForLog(readyLogLine).WithStartupTimeout(10*time.Minute),
		),
	)
	if err != nil {
		panic("start the Oracle fixture: " + err.Error())
	}

	code := runSuite(ctx, container, m)

	if err := testcontainers.TerminateContainer(container); err != nil {
		panic("terminate the Oracle fixture: " + err.Error())
	}

	os.Exit(code)
}

// runSuite resolves the container's address, seeds it, and runs the tests. It is
// separate from TestMain so the container is terminated even when seeding fails.
func runSuite(ctx context.Context, container testcontainers.Container, m *testing.M) int {
	host, err := container.Host(ctx)
	if err != nil {
		panic("resolve the fixture host: " + err.Error())
	}

	mapped, err := container.MappedPort(ctx, "1521/tcp")
	if err != nil {
		panic("resolve the fixture port: " + err.Error())
	}

	port := mapped.Num()

	fixtureConnection = dbadapter.NewConnectionConfig(host)
	fixtureConnection.Port = &port
	fixtureConnection.Database = fixtureService
	fixtureConnection.Username = fixtureUser
	fixtureConnection.Password = dbadapter.NewSecret([]byte(fixturePassword))

	if err := seed(ctx, fixtureConnection); err != nil {
		panic("seed the Oracle fixture: " + err.Error())
	}

	return m.Run()
}

// seed applies the fixture DDL over a separate connection.
//
// The statements are separated by a tilde rather than by a semicolon, because a
// PL/SQL block contains semicolons of its own and splitting on those would cut
// the trigger body in half.
func seed(ctx context.Context, cfg dbadapter.ConnectionConfig) error {
	db, err := sql.Open(driverName, dataSourceName(cfg))
	if err != nil {
		return err
	}

	defer func() { discardError(db.Close()) }()

	for _, statement := range strings.Split(fixtureDDL, "\n~\n") {
		statement = strings.TrimSpace(statement)
		if statement == "" {
			continue
		}

		if _, err := db.ExecContext(ctx, statement); err != nil {
			return fmt.Errorf("seeding %q: %w", statement, err)
		}
	}

	return nil
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

// TestConnectingNeedsNoInstantClient is the property that makes this engine
// reachable at all, asserted where it can actually fail.
//
// A driver that bound to the Instant Client would fail to connect on a host that
// does not have it, and the container this suite runs against does not: the
// client libraries live inside the server image, not on the machine running the
// tests. A successful login is therefore direct evidence of the pure-Go wire
// protocol, which is what R13 exists for.
func TestConnectingNeedsNoInstantClient(t *testing.T) {
	assert.Empty(t, os.Getenv("ORACLE_HOME"),
		"no Instant Client installation is configured on this machine")
	assert.Empty(t, os.Getenv("LD_LIBRARY_PATH"),
		"nothing points the loader at an Oracle client library")

	adapter := openFixture(t)
	require.NoError(t, adapter.Ping(t.Context()))
}

// TestACollectedSchemaValidates is the end-to-end assertion: everything the
// adapter produces has to survive the same validation an artifact load applies,
// including the recursive credential scan.
func TestACollectedSchemaValidates(t *testing.T) {
	document := collectFixture(t)

	require.NoError(t, document.Validate())
	assert.Equal(t, dbschema.Oracle, document.DatabaseInfo.Type)
	assert.Equal(t, normalize(fixtureUser), document.DatabaseInfo.Name,
		"the surveyed schema is the connecting user's, folded as the engine stores it")
}

func TestEveryFixtureTableIsCollected(t *testing.T) {
	document := collectFixture(t)

	names := make([]string, 0, len(document.Tables))
	for _, table := range document.Tables {
		names = append(names, table.Name)
	}

	assert.Subset(t, names, []string{"USERS", "ORDERS", "COMPOUND", "NEVER_WRITTEN"},
		"an unquoted name was folded to upper case on the way in")
}

// TestNumberResolvesToThreeDifferentThingsThroughTheEngine is the mapping this
// adapter exists to get right, asserted against a real catalog.
func TestNumberResolvesToThreeDifferentThingsThroughTheEngine(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "USERS")

	assert.Equal(t, dbschema.IntegerType(32, true), columnNamed(t, users, "SMALL_COUNT").DataType,
		"NUMBER(9,0) fits in 32 bits")
	assert.Equal(t, dbschema.IntegerType(64, true), columnNamed(t, users, "ID").DataType,
		"NUMBER(10,0) does not")

	ten := uint8(10)
	assert.Equal(t, dbschema.FloatType(&ten), columnNamed(t, users, "AMOUNT").DataType,
		"a scale makes it a decimal")
	assert.Equal(t, dbschema.FloatType(nil), columnNamed(t, users, "UNBOUNDED").DataType,
		"a bare NUMBER has no declared shape at all")
}

func TestColumnTypesAndNullabilityComeFromTheCatalog(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "USERS")

	length := uint32(255)
	assert.Equal(t, dbschema.StringType(&length), columnNamed(t, users, "EMAIL").DataType)
	assert.Equal(t, dbschema.DateTimeType(true), columnNamed(t, users, "CREATED_AT").DataType)
	assert.Equal(t, dbschema.BinaryType(nil), columnNamed(t, users, "PAYLOAD").DataType)

	assert.False(t, columnNamed(t, users, "EMAIL").Nullable)
	assert.True(t, columnNamed(t, users, "DISPLAY_NAME").Nullable)

	require.NotNil(t, users.Comment)
	assert.Equal(t, "people who can sign in", *users.Comment)

	for i, column := range users.Columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}
}

func TestKeysConstraintsAndIndexesAreCollected(t *testing.T) {
	document := collectFixture(t)

	orders := tableNamed(t, document, "ORDERS")

	require.NotNil(t, orders.PrimaryKey)
	assert.Equal(t, []string{"ID"}, orders.PrimaryKey.Columns)

	require.Len(t, orders.ForeignKeys, 1)
	key := orders.ForeignKeys[0]
	assert.Equal(t, []string{"USER_ID"}, key.Columns)
	assert.Equal(t, "USERS", key.ReferencedTable, "resolved through the constraint it names")
	assert.Equal(t, []string{"ID"}, key.ReferencedColumns)
	require.NotNil(t, key.OnDelete)
	assert.Equal(t, dbschema.Cascade, *key.OnDelete)
	assert.Nil(t, key.OnUpdate, "Oracle has no ON UPDATE for foreign keys")

	indexNames := make([]string, 0, len(orders.Indexes))
	for _, index := range orders.Indexes {
		indexNames = append(indexNames, index.Name)
	}

	assert.Contains(t, indexNames, "ORDERS_BY_USER")

	compound := tableNamed(t, document, "COMPOUND")
	require.NotNil(t, compound.PrimaryKey)
	assert.Equal(t, []string{"B", "A"}, compound.PrimaryKey.Columns,
		"the key order, not the column order")
}

// TestNotNullChecksDoNotFloodTheConstraintList is the noise this adapter
// suppresses: Oracle records every NOT NULL declaration as a check constraint.
func TestNotNullChecksDoNotFloodTheConstraintList(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "USERS")

	checks := []dbschema.Constraint{}

	for _, constraint := range users.Constraints {
		if constraint.ConstraintType == dbschema.ConstraintCheck {
			checks = append(checks, constraint)
		}
	}

	require.Len(t, checks, 1, "only the check an operator actually wrote")
	assert.Equal(t, "USERS_EMAIL_LENGTH", checks[0].Name)
	require.NotNil(t, checks[0].CheckClause)
	assert.Contains(t, strings.ToUpper(*checks[0].CheckClause), "LENGTH")
}

// TestANeverAnalyzedTableReportsZeroRatherThanNoEstimate is the Oracle form of
// the nullable row-estimate trap: NUM_ROWS is NULL until statistics are
// gathered, which on a freshly loaded schema is every table.
func TestANeverAnalyzedTableReportsZeroRatherThanNoEstimate(t *testing.T) {
	empty := tableNamed(t, collectFixture(t), "NEVER_WRITTEN")

	require.NotNil(t, empty.RowCount)
	assert.Equal(t, uint64(0), *empty.RowCount)
}

func TestViewsAndTriggersAreCollected(t *testing.T) {
	document := collectFixture(t)

	viewNames := make([]string, 0, len(document.Views))
	for _, view := range document.Views {
		viewNames = append(viewNames, view.Name)
	}

	assert.Contains(t, viewNames, "ACTIVE_USERS")

	triggerNames := make([]string, 0, len(document.Triggers))
	for _, trigger := range document.Triggers {
		triggerNames = append(triggerNames, trigger.Name)

		if trigger.Name == "USERS_TOUCHED" {
			assert.Equal(t, dbschema.TimingBefore, trigger.Timing)
			assert.Equal(t, dbschema.TriggerUpdate, trigger.Event)
		}
	}

	assert.Contains(t, triggerNames, "USERS_TOUCHED")
}

// TestSamplingFoldsTheTableNameTheWayTheEngineWould is the case-handling claim
// asserted end to end: an operator writes a name in lower case and reaches the
// table the engine stored in upper case.
func TestSamplingFoldsTheTableNameTheWayTheEngineWould(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.NewSamplingConfig(2),
	)
	require.NoError(t, err)

	assert.Equal(t, "USERS", sample.TableName)
	assert.Equal(t, uint32(2), sample.SampleSize)
	require.NotNil(t, sample.Status)
	assert.Equal(t, dbschema.SampleComplete, sample.Status.State)
	require.NotNil(t, sample.Ordering)
	assert.Equal(t, dbschema.OrderAutoIncrement, sample.Ordering.Kind)
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

// TestASurveyLeavesTheSchemaUnchanged is R16 asserted against the data.
//
// Oracle has no session-level read-only switch either, so as with SQL Server the
// guarantee rests on this package issuing only reads, and this is what checks
// that it does.
func TestASurveyLeavesTheSchemaUnchanged(t *testing.T) {
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
	assert.Len(t,
		tableNamed(t, after, "USERS").Columns, len(tableNamed(t, before, "USERS").Columns),
		"no column was added or removed")
}
