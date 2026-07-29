//go:build integration

package mysql

import (
	"context"
	"database/sql"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	tcmysql "github.com/testcontainers/testcontainers-go/modules/mysql"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The fixture server's credentials. They are a container's throwaway
// credentials, created and destroyed inside one test run.
const (
	fixtureImage    = "mysql:8.4"
	fixtureDatabase = "shop"
	fixtureUser     = "root"

	fixturePassword = "surveyor-fixture"
)

// fixtureDDL is the database every test in this file surveys.
//
// The oddities are deliberate. The table whose name embeds a backtick exercises
// the identifier-quoting path through ordinary collection. compound declares
// its key in an order that differs from its column order. never_written is left
// empty so its TABLE_ROWS estimate is the NULL that GOTCHAS records.
const fixtureDDL = `
CREATE TABLE users (
    id           bigint unsigned NOT NULL AUTO_INCREMENT PRIMARY KEY,
    email        varchar(255) NOT NULL,
    display_name varchar(64) NULL,
    created_at   timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
    active       boolean NOT NULL DEFAULT TRUE,
    score        tinyint NOT NULL DEFAULT 0,
    payload      json NULL,
    avatar       blob NULL,
    UNIQUE KEY users_email (email)
) COMMENT 'people who can sign in';

CREATE TABLE orders (
    id      int NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id bigint unsigned NOT NULL,
    total   decimal(10,2) NOT NULL,
    CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES users(id)
        ON DELETE CASCADE ON UPDATE RESTRICT,
    KEY orders_by_user (user_id)
);

CREATE TABLE compound (a int NOT NULL, b varchar(32) NOT NULL, PRIMARY KEY (b, a));

CREATE TABLE never_written (id int NOT NULL PRIMARY KEY, note text);

CREATE TABLE ` + "`we``ird`" + ` (id int NOT NULL AUTO_INCREMENT PRIMARY KEY, label text NOT NULL);

CREATE VIEW active_users AS SELECT id, email FROM users WHERE active = TRUE;

CREATE TRIGGER users_touched BEFORE UPDATE ON users FOR EACH ROW SET NEW.score = NEW.score;

INSERT INTO users (email, display_name) VALUES
    ('ada@example.test', 'Ada'), ('grace@example.test', 'Grace'), ('alan@example.test', 'Alan');

INSERT INTO orders (user_id, total) VALUES (1, 10.50), (2, 22.00), (1, 3.25);

INSERT INTO ` + "`we``ird`" + ` (label) VALUES ('first'), ('second');

ANALYZE TABLE users;
`

// fixtureServer starts one MySQL container and returns a connection
// configuration pointing at it.
func fixtureServer(t *testing.T) dbadapter.ConnectionConfig {
	t.Helper()

	ctx := context.Background()

	container, err := tcmysql.Run(ctx, fixtureImage,
		tcmysql.WithDatabase(fixtureDatabase),
		tcmysql.WithUsername(fixtureUser),
		tcmysql.WithPassword(fixturePassword),
	)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, testcontainers.TerminateContainer(container)) })

	dsn, err := container.ConnectionString(ctx, "multiStatements=true", "parseTime=true")
	require.NoError(t, err)

	seed(t, dsn)

	host, err := container.Host(ctx)
	require.NoError(t, err)

	mapped, err := container.MappedPort(ctx, "3306/tcp")
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

	db, err := sql.Open(driverName, dsn)
	require.NoError(t, err)

	defer func() { require.NoError(t, db.Close()) }()

	for _, statement := range strings.Split(fixtureDDL, ";\n") {
		if strings.TrimSpace(statement) == "" {
			continue
		}

		_, err := db.ExecContext(t.Context(), statement)
		require.NoError(t, err, "seeding: %s", statement)
	}
}

func openFixture(t *testing.T) *Adapter {
	t.Helper()

	adapter, err := Open(t.Context(), fixtureServer(t))
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
	assert.Equal(t, dbschema.MySQL, document.DatabaseInfo.Type)
	assert.Equal(t, fixtureDatabase, document.DatabaseInfo.Name)
	assert.NotNil(t, document.DatabaseInfo.Version)
	assert.False(t, document.DatabaseInfo.SystemDatabase)
}

func TestEveryFixtureTableIsCollected(t *testing.T) {
	document := collectFixture(t)

	names := make([]string, 0, len(document.Tables))
	for _, table := range document.Tables {
		names = append(names, table.Name)
	}

	assert.ElementsMatch(t,
		[]string{"users", "orders", "compound", "never_written", "we`ird"}, names)
}

func TestColumnsCarryOneBasedOrdinalPositions(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	require.Len(t, users.Columns, 8)

	for i, column := range users.Columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}
}

// TestUnsignedAndSignedIntegersAreDistinguished is GOTCHAS 6.1 read through the
// document rather than through the scan: DATA_TYPE reports "bigint" for both,
// and only COLUMN_TYPE carries the difference.
func TestUnsignedAndSignedIntegersAreDistinguished(t *testing.T) {
	document := collectFixture(t)

	users := tableNamed(t, document, "users")
	assert.Equal(t, dbschema.IntegerType(64, false), columnNamed(t, users, "id").DataType,
		"bigint unsigned")
	assert.Equal(t, dbschema.IntegerType(8, true), columnNamed(t, users, "score").DataType,
		"tinyint, signed")

	orders := tableNamed(t, document, "orders")
	assert.Equal(t, dbschema.IntegerType(32, true), columnNamed(t, orders, "id").DataType)
	assert.Equal(t, dbschema.IntegerType(64, false), columnNamed(t, orders, "user_id").DataType)
}

func TestColumnTypesAndFlagsComeFromTheCatalog(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	length := uint32(255)
	assert.Equal(t, dbschema.StringType(&length), columnNamed(t, users, "email").DataType)
	assert.Equal(t, dbschema.BooleanType(), columnNamed(t, users, "active").DataType,
		"BOOLEAN is an alias for TINYINT(1) and is recovered from COLUMN_TYPE")
	assert.Equal(t, dbschema.DateTimeType(true), columnNamed(t, users, "created_at").DataType,
		"TIMESTAMP carries a zone even though the column does not name one")
	assert.Equal(t, dbschema.JSONType(), columnNamed(t, users, "payload").DataType)
	// A MySQL BLOB has a real 65,535-byte ceiling and INFORMATION_SCHEMA
	// reports it, unlike the MAX types of other engines which report no limit.
	blobLimit := uint32(65535)
	assert.Equal(t, dbschema.BinaryType(&blobLimit), columnNamed(t, users, "avatar").DataType)

	assert.False(t, columnNamed(t, users, "email").Nullable)
	assert.True(t, columnNamed(t, users, "display_name").Nullable)
	assert.True(t, columnNamed(t, users, "id").PrimaryKey)
	assert.True(t, columnNamed(t, users, "id").AutoGenerate)

	require.NotNil(t, users.Comment)
	assert.Equal(t, "people who can sign in", *users.Comment)
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
	assert.Nil(t, key.ReferencedSchema, "a same-database key carries no redundant qualifier")

	indexNames := make([]string, 0, len(orders.Indexes))
	for _, index := range orders.Indexes {
		indexNames = append(indexNames, index.Name)
	}

	assert.Contains(t, indexNames, "orders_by_user")
	assert.Contains(t, indexNames, primaryIndexName)

	compound := tableNamed(t, document, "compound")
	require.NotNil(t, compound.PrimaryKey)
	assert.Equal(t, []string{"b", "a"}, compound.PrimaryKey.Columns,
		"the key order, not the column order")

	users := tableNamed(t, document, "users")

	var unique []dbschema.Constraint

	for _, constraint := range users.Constraints {
		if constraint.ConstraintType == dbschema.ConstraintUnique {
			unique = append(unique, constraint)
		}
	}

	require.Len(t, unique, 1)
	assert.Equal(t, []string{"email"}, unique[0].Columns)
}

// TestANeverWrittenTableReportsZeroRatherThanNoEstimate is GOTCHAS 6.2:
// TABLE_ROWS is NULL for a table the storage engine has no statistics for, and
// scanning it into a plain integer would fail on every such table.
func TestANeverWrittenTableReportsZeroRatherThanNoEstimate(t *testing.T) {
	document := collectFixture(t)

	empty := tableNamed(t, document, "never_written")
	require.NotNil(t, empty.RowCount, "a NULL estimate is zero, not an absent estimate")
	assert.Equal(t, uint64(0), *empty.RowCount)
}

func TestViewsAndTriggersAreCollected(t *testing.T) {
	document := collectFixture(t)

	require.Len(t, document.Views, 1)
	assert.Equal(t, "active_users", document.Views[0].Name)
	assert.Len(t, document.Views[0].Columns, 2)

	require.Len(t, document.Triggers, 1)
	assert.Equal(t, "users_touched", document.Triggers[0].Name)
	assert.Equal(t, "users", document.Triggers[0].TableName)
	assert.Equal(t, dbschema.TimingBefore, document.Triggers[0].Timing)
	assert.Equal(t, dbschema.TriggerUpdate, document.Triggers[0].Event)
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
	assert.Equal(t, "alan@example.test", sample.Rows[0]["email"],
		"the most recent row, not the first")
	assert.IsType(t, "", sample.Rows[0]["email"], "text is reported as text, not base64")
}

// TestAnIdentifierContainingABacktickSurvivesSampling exercises the escaping
// helper through the engine rather than against a string.
func TestAnIdentifierContainingABacktickSurvivesSampling(t *testing.T) {
	adapter := openFixture(t)

	sample, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "we`ird"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	assert.Equal(t, uint32(2), sample.SampleSize)
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

func TestMultiDatabaseEnumeration(t *testing.T) {
	adapter := openFixture(t)

	cfg := dbadapter.NewCollectionConfig("ignored")

	names, err := adapter.ListDatabases(t.Context(), cfg)
	require.NoError(t, err)
	assert.Contains(t, names, fixtureDatabase)
	assert.NotContains(t, names, "mysql", "the server's own databases are excluded by default")
	assert.NotContains(t, names, "information_schema")

	cfg.IncludeSystemDatabases = true
	withSystem, err := adapter.ListDatabases(t.Context(), cfg)
	require.NoError(t, err)
	assert.Contains(t, withSystem, "information_schema", "and included when asked for")

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

	assert.Equal(t, dbschema.MySQL, server.ServerInfo.ServerType)
	assert.Equal(t, dbschema.ModeMultiDatabase, server.ServerInfo.CollectionMode.Kind)
	assert.Positive(t, server.ServerInfo.SystemDatabasesExcluded)
	require.Len(t, server.Databases, 1)
	assert.Equal(t, fixtureDatabase, server.Databases[0].DatabaseInfo.Name)
	assert.NotEmpty(t, server.Databases[0].Tables)
}

// TestTheSessionIsReadOnly is the enforcement half of R16. Every statement this
// package issues is a read, and the session variable is what catches the case
// where one is not.
func TestTheSessionIsReadOnly(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.db.ExecContext(t.Context(), `CREATE TABLE intruder (id int)`)
	require.Error(t, err, "DDL through the adapter's own pool is rejected")

	_, err = adapter.db.ExecContext(t.Context(), `DELETE FROM users`)
	require.Error(t, err, "DML through the adapter's own pool is rejected")

	document := collectFixture(t)
	assert.Len(t, tableNamed(t, document, "users").Columns, 8)
}
