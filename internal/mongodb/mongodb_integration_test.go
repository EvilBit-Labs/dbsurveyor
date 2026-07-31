//go:build integration

package mongodb

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/testcontainers/testcontainers-go"
	tcmongo "github.com/testcontainers/testcontainers-go/modules/mongodb"
	"go.mongodb.org/mongo-driver/v2/bson"
	"go.mongodb.org/mongo-driver/v2/mongo"
	"go.mongodb.org/mongo-driver/v2/mongo/options"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

const (
	fixtureImage    = "mongo:8"
	fixtureDatabase = "shop"
)

// fixtureServer starts one MongoDB container, seeds it, and returns a connection
// configuration pointing at it.
func fixtureServer(t *testing.T) dbadapter.ConnectionConfig {
	t.Helper()

	ctx := context.Background()

	container, err := tcmongo.Run(ctx, fixtureImage)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, testcontainers.TerminateContainer(container)) })

	uri, err := container.ConnectionString(ctx)
	require.NoError(t, err)

	seed(t, uri)

	host, err := container.Host(ctx)
	require.NoError(t, err)

	mapped, err := container.MappedPort(ctx, "27017/tcp")
	require.NoError(t, err)

	port := mapped.Num()

	cfg := dbadapter.NewConnectionConfig(host)
	cfg.Port = &port
	cfg.Database = fixtureDatabase

	return cfg
}

// seed writes the fixture documents over a separate connection. The adapter
// under test never writes, which is the point.
//
// The documents disagree on purpose: age is absent from one, postcode is a
// string in two and a number in one, and nickname is explicitly null. Each is a
// shape a schemaless collection produces in the field and a declaration-reading
// adapter would never see.
func seed(t *testing.T, uri string) {
	t.Helper()

	client, err := mongo.Connect(options.Client().ApplyURI(uri))
	require.NoError(t, err)

	defer func() { require.NoError(t, client.Disconnect(context.Background())) }()

	database := client.Database(fixtureDatabase)

	_, err = database.Collection("users").InsertMany(t.Context(), []any{
		bson.M{
			"email":    "ada@example.test",
			"age":      int32(36),
			"postcode": "SW1A 1AA",
			"address":  bson.M{"city": "London", "line1": "1 Test Street"},
		},
		bson.M{
			"email":    "grace@example.test",
			"nickname": nil,
			"postcode": "EC1A 1BB",
			"address":  bson.M{"city": "Cambridge"},
		},
		bson.M{
			"email":    "alan@example.test",
			"age":      int32(41),
			"postcode": int32(90210),
			"address":  bson.M{"city": "Manchester", "line1": "2 Test Road"},
		},
	})
	require.NoError(t, err)

	_, err = database.Collection("orders").InsertMany(t.Context(), []any{
		bson.M{"total": 10.5, "placed": bson.NewDateTimeFromTime(time.Now())},
		bson.M{"total": 22.0, "placed": bson.NewDateTimeFromTime(time.Now())},
	})
	require.NoError(t, err)

	// An empty collection is a case the inference has to survive.
	require.NoError(t, database.CreateCollection(t.Context(), "audit"))

	_, err = database.Collection("users").Indexes().CreateOne(t.Context(), mongo.IndexModel{
		Keys:    bson.D{{Key: "email", Value: 1}},
		Options: options.Index().SetUnique(true).SetName("users_email"),
	})
	require.NoError(t, err)
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

	t.Fatalf("no collection %q in the collected schema", name)

	return dbschema.Table{}
}

// TestACollectedSchemaValidates is the end-to-end assertion: everything the
// adapter produces has to survive the same validation an artifact load applies,
// including the recursive credential scan.
func TestACollectedSchemaValidates(t *testing.T) {
	document := collectFixture(t)

	require.NoError(t, document.Validate())
	assert.Equal(t, dbschema.MongoDB, document.DatabaseInfo.Type)
	assert.Equal(t, fixtureDatabase, document.DatabaseInfo.Name)
	assert.NotNil(t, document.DatabaseInfo.Version)
}

func TestEveryCollectionBecomesATable(t *testing.T) {
	document := collectFixture(t)

	names := make([]string, 0, len(document.Tables))
	for _, table := range document.Tables {
		names = append(names, table.Name)
	}

	assert.ElementsMatch(t, []string{"users", "orders", "audit"}, names)
}

// TestHeterogeneousDocumentsInferTheUnionOfTheirFields is the same claim the
// unit test makes, asserted against a real sample drawn by $sample.
func TestHeterogeneousDocumentsInferTheUnionThroughTheEngine(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	names := make([]string, 0, len(users.Columns))
	for _, column := range users.Columns {
		names = append(names, column.Name)
	}

	assert.Subset(t, names,
		[]string{"_id", "address", "address.city", "address.line1", "age", "email", "nickname", "postcode"})
}

func TestAFieldMissingFromSomeDocumentsIsNullableThroughTheEngine(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	byName := map[string]dbschema.Column{}
	for _, column := range users.Columns {
		byName[column.Name] = column
	}

	assert.False(t, byName["email"].Nullable, "present in every document")
	assert.True(t, byName["age"].Nullable, "absent from one")
	assert.True(t, byName["nickname"].Nullable, "explicitly null in one and absent from two")
	assert.True(t, byName["address.line1"].Nullable, "a nested field is judged the same way")
}

// TestADisagreementBetweenWritersIsReported is why the type frequency is
// collected at all.
func TestADisagreementBetweenWritersIsReported(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	for _, column := range users.Columns {
		if column.Name != "postcode" {
			continue
		}

		assert.Equal(t, dbschema.StringType(nil), column.DataType, "the type most documents carried")
		require.NotNil(t, column.Comment)
		assert.Contains(t, *column.Comment, "string in 2")
		assert.Contains(t, *column.Comment, "int in 1")

		return
	}

	t.Fatal("postcode was not inferred")
}

// TestAnEmptyCollectionIsATableWithNoColumns is the case the plan calls out: it
// is a fact about the collection, not a failure of the survey.
func TestAnEmptyCollectionIsATableWithNoColumns(t *testing.T) {
	audit := tableNamed(t, collectFixture(t), "audit")

	assert.Empty(t, audit.Columns)
	assert.Nil(t, audit.PrimaryKey)
	require.NotNil(t, audit.RowCount)
	assert.Equal(t, uint64(0), *audit.RowCount)
}

func TestTheIdentifierIsTheCollectionsPrimaryKey(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	require.NotNil(t, users.PrimaryKey)
	assert.Equal(t, []string{identifierField}, users.PrimaryKey.Columns)
}

func TestIndexesAreCollected(t *testing.T) {
	users := tableNamed(t, collectFixture(t), "users")

	byName := map[string]dbschema.Index{}
	for _, index := range users.Indexes {
		byName[index.Name] = index
	}

	require.Contains(t, byName, identifierIndexName)
	assert.True(t, byName[identifierIndexName].Primary)

	require.Contains(t, byName, "users_email")
	assert.True(t, byName["users_email"].Unique)
	assert.Equal(t, []dbschema.IndexColumn{
		{Name: "email", SortOrder: pointerTo(dbschema.Ascending)},
	}, byName["users_email"].Columns)
}

func pointerTo(direction dbschema.SortDirection) *dbschema.SortDirection {
	return &direction
}

func TestSamplingHonorsTheConfiguredSizeAndReportsComplete(t *testing.T) {
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
	assert.Equal(t, dbschema.SampleRandom, sample.SamplingStrategy.Kind,
		"$sample is a random subset, not the most recent documents")

	// An ObjectID marshals to an array of bytes unless it is rendered first.
	identifier, ok := sample.Rows[0][identifierField].(string)
	require.True(t, ok, "the identifier is rendered as its hex form")
	assert.Len(t, identifier, 24)
}

// TestSamplingAnUnknownCollectionFails keeps a misspelled name distinguishable
// from an empty collection, which the aggregation reports identically.
func TestSamplingAnUnknownCollectionFails(t *testing.T) {
	adapter := openFixture(t)

	_, err := adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "no_such_collection"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.ErrorIs(t, err, ErrUnknownCollection)
}

// TestASurveyLeavesTheDatabaseUnchanged is R16 asserted against the data rather
// than against the statements, which matters here because MongoDB has no
// read-only session mode to lean on.
func TestASurveyLeavesTheDatabaseUnchanged(t *testing.T) {
	connection := fixtureServer(t)

	adapter, err := Open(t.Context(), connection)
	require.NoError(t, err)

	defer func() { require.NoError(t, adapter.Close()) }()

	before, err := adapter.collectionNames(t.Context())
	require.NoError(t, err)

	beforeCount, err := adapter.estimatedCount(t.Context(), "users")
	require.NoError(t, err)

	_, err = adapter.CollectSchema(t.Context(), dbadapter.NewCollectionConfig("ignored"))
	require.NoError(t, err)

	_, err = adapter.SampleTable(
		t.Context(),
		dbadapter.TableRef{Table: "users"},
		dbadapter.DefaultSamplingConfig(),
	)
	require.NoError(t, err)

	after, err := adapter.collectionNames(t.Context())
	require.NoError(t, err)

	afterCount, err := adapter.estimatedCount(t.Context(), "users")
	require.NoError(t, err)

	assert.ElementsMatch(t, before, after, "no collection was created or dropped")
	assert.Equal(t, beforeCount, afterCount, "no document was written or removed")
}
