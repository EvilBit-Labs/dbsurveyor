package mongodb

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.mongodb.org/mongo-driver/v2/bson"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// infer folds a set of documents into an inference, which is the whole of what
// this adapter has to get right.
func infer(documents ...bson.M) *inference {
	inferred := newInference()
	for _, document := range documents {
		inferred.observe(document)
	}

	return inferred
}

// columnNamed finds an inferred column by its path.
func columnNamed(t *testing.T, columns []dbschema.Column, name string) dbschema.Column {
	t.Helper()

	for _, column := range columns {
		if column.Name == name {
			return column
		}
	}

	t.Fatalf("no inferred column %q", name)

	return dbschema.Column{}
}

// TestHeterogeneousDocumentsInferTheUnionOfTheirFields is the core claim. A
// collection has no declaration, so the shape is whatever the documents agree
// and disagree about, and every field any of them carries has to appear.
func TestHeterogeneousDocumentsInferTheUnionOfTheirFields(t *testing.T) {
	inferred := infer(
		bson.M{"_id": bson.NewObjectID(), "email": "ada@example.test", "age": int32(36)},
		bson.M{"_id": bson.NewObjectID(), "email": "grace@example.test", "nickname": "Amazing"},
		bson.M{"_id": bson.NewObjectID(), "email": "alan@example.test", "age": int32(41)},
	)

	columns := inferred.columns()

	names := make([]string, 0, len(columns))
	for _, column := range columns {
		names = append(names, column.Name)
	}

	assert.Equal(t, []string{"_id", "age", "email", "nickname"}, names,
		"every field any document carried, sorted so two runs agree")
}

// TestAFieldMissingFromSomeDocumentsIsNullable is the only evidence a
// schemaless collection offers about nullability.
func TestAFieldMissingFromSomeDocumentsIsNullable(t *testing.T) {
	columns := infer(
		bson.M{"email": "ada@example.test", "age": int32(36)},
		bson.M{"email": "grace@example.test"},
	).columns()

	assert.False(t, columnNamed(t, columns, "email").Nullable, "present in every sampled document")
	assert.True(t, columnNamed(t, columns, "age").Nullable, "absent from one")
}

// TestAnExplicitNullAlsoMakesAFieldNullable covers the other way a field can be
// absent: present as a key, holding nothing.
func TestAnExplicitNullAlsoMakesAFieldNullable(t *testing.T) {
	columns := infer(
		bson.M{"nickname": "Amazing"},
		bson.M{"nickname": nil},
	).columns()

	nickname := columnNamed(t, columns, "nickname")
	assert.True(t, nickname.Nullable)
	assert.Equal(t, dbschema.StringType(nil), nickname.DataType,
		"a null says the field exists and nothing about its type")
}

// TestAFieldSeenOnlyAsNullHasNoInferredType records the honest answer rather
// than a guess.
func TestAFieldSeenOnlyAsNullHasNoInferredType(t *testing.T) {
	columns := infer(bson.M{"unset": nil}, bson.M{"unset": nil}).columns()

	assert.Equal(t, dbschema.CustomType(typeNull), columnNamed(t, columns, "unset").DataType)
}

// TestTheDominantTypeWinsAndTheDisagreementIsReported is why the frequency
// matters. A field that is a string in most documents and a number in the rest
// is a data-quality problem, and reporting only "string" would hide it.
func TestTheDominantTypeWinsAndTheDisagreementIsReported(t *testing.T) {
	columns := infer(
		bson.M{"postcode": "SW1A 1AA"},
		bson.M{"postcode": "EC1A 1BB"},
		bson.M{"postcode": int32(90210)},
	).columns()

	postcode := columnNamed(t, columns, "postcode")

	assert.Equal(t, dbschema.StringType(nil), postcode.DataType, "the type most documents carried")

	require.NotNil(t, postcode.Comment, "a field two writers disagreed about says so")
	assert.Contains(t, *postcode.Comment, "string in 2")
	assert.Contains(t, *postcode.Comment, "int in 1")
}

// TestAnAgreedFieldCarriesNoComment keeps the comment meaningful: it appears
// when there is something to say, not on every column.
func TestAnAgreedFieldCarriesNoComment(t *testing.T) {
	columns := infer(
		bson.M{"email": "ada@example.test"},
		bson.M{"email": "grace@example.test"},
	).columns()

	assert.Nil(t, columnNamed(t, columns, "email").Comment)
}

// TestNestedDocumentsAreInferredAsDottedPaths covers the shape an application
// actually queries by.
func TestNestedDocumentsAreInferredAsDottedPaths(t *testing.T) {
	columns := infer(
		bson.M{"address": bson.M{"city": "London", "postcode": "SW1A 1AA"}},
		bson.M{"address": bson.M{"city": "Cambridge"}},
	).columns()

	assert.Equal(t, dbschema.JSONType(), columnNamed(t, columns, "address").DataType,
		"the parent is reported as well as its fields")
	assert.Equal(t, dbschema.StringType(nil), columnNamed(t, columns, "address.city").DataType)
	assert.True(t, columnNamed(t, columns, "address.postcode").Nullable)
}

// TestNestingIsBoundedSoAPathologicalDocumentCannotRunAway checks the depth cap.
func TestNestingIsBoundedSoAPathologicalDocumentCannotRunAway(t *testing.T) {
	document := bson.M{"a": bson.M{"b": bson.M{"c": bson.M{"d": bson.M{"e": "too deep"}}}}}

	columns := infer(document).columns()

	names := make([]string, 0, len(columns))
	for _, column := range columns {
		names = append(names, column.Name)
	}

	assert.Contains(t, names, "a.b.c.d")
	assert.NotContains(t, names, "a.b.c.d.e", "inference stops at maxInferenceDepth")
}

// TestAnEmptyCollectionInfersNoColumnsAndDoesNotFail is the case a schemaless
// engine makes ordinary: a collection with no documents has no shape, and that
// is a fact rather than a failure.
func TestAnEmptyCollectionInfersNoColumnsAndDoesNotFail(t *testing.T) {
	inferred := newInference()

	assert.Empty(t, inferred.columns())
	assert.Zero(t, inferred.documents)
}

// TestTheIdentifierIsTheOnlyKeyMongoDBGuarantees records the one field every
// document has.
func TestTheIdentifierIsTheOnlyKeyMongoDBGuarantees(t *testing.T) {
	columns := infer(bson.M{"_id": bson.NewObjectID(), "email": "ada@example.test"}).columns()

	identifier := columnNamed(t, columns, identifierField)
	assert.True(t, identifier.PrimaryKey)
	assert.True(t, identifier.AutoGenerate)
	assert.Equal(t, dbschema.CustomType(typeObjectID), identifier.DataType,
		"an ObjectID is 12 bytes, not a UUID")

	assert.False(t, columnNamed(t, columns, "email").PrimaryKey)
}

func TestOrdinalPositionsAreConsecutiveAndSorted(t *testing.T) {
	columns := infer(bson.M{"zebra": 1, "alpha": 2, "mike": 3}).columns()

	require.Len(t, columns, 3)

	for i, column := range columns {
		assert.Equal(t, uint32(i+1), column.OrdinalPosition, "column %q", column.Name)
	}

	assert.Equal(t, "alpha", columns[0].Name)
}

func TestBSONTypesMapToTheirEngineIndependentForm(t *testing.T) {
	for name, want := range map[string]struct {
		value  any
		mapped dbschema.UnifiedDataType
	}{
		"string":    {"text", dbschema.StringType(nil)},
		"int":       {int32(1), dbschema.IntegerType(32, true)},
		"long":      {int64(1), dbschema.IntegerType(64, true)},
		"double":    {1.5, dbschema.FloatType(nil)},
		"bool":      {true, dbschema.BooleanType()},
		"date":      {bson.DateTime(0), dbschema.DateTimeType(true)},
		"binary":    {bson.Binary{Data: []byte{1}}, dbschema.BinaryType(nil)},
		"object":    {bson.M{}, dbschema.JSONType()},
		"array":     {bson.A{1, "two"}, dbschema.ArrayType(dbschema.CustomType(typeUnknown))},
		"object id": {bson.NewObjectID(), dbschema.CustomType(typeObjectID)},
		"time":      {time.Now(), dbschema.DateTimeType(true)},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, want.mapped, mapDataType(bsonTypeOf(want.value)))
		})
	}
}

// TestDecodedValuesAreRenderedAsThemselves covers the JSON round trip. An
// ObjectID marshals to an array of bytes and a DateTime to a bare integer unless
// each is rendered first.
func TestDecodedValuesAreRenderedAsThemselves(t *testing.T) {
	identifier := bson.NewObjectID()
	moment := bson.NewDateTimeFromTime(time.Date(2026, time.July, 27, 12, 0, 0, 0, time.UTC))

	rendered := normalizeDocument(bson.M{
		"_id":     identifier,
		"created": moment,
		"nested":  bson.M{"id": identifier},
		"list":    bson.A{identifier, "plain"},
	})

	assert.Equal(t, identifier.Hex(), rendered["_id"])
	assert.Equal(t, "2026-07-27T12:00:00Z", rendered["created"])

	nested, ok := rendered["nested"].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, identifier.Hex(), nested["id"], "nested documents are rendered too")

	list, ok := rendered["list"].([]any)
	require.True(t, ok)
	assert.Equal(t, identifier.Hex(), list[0])
	assert.Equal(t, "plain", list[1])
}

func TestIndexKeysKeepTheirOrderAndDirection(t *testing.T) {
	columns := indexColumns(bson.D{
		{Key: "user_id", Value: int32(1)},
		{Key: "created", Value: int32(-1)},
		{Key: "body", Value: "text"},
	})

	require.Len(t, columns, 3)

	assert.Equal(t, "user_id", columns[0].Name)
	require.NotNil(t, columns[0].SortOrder)
	assert.Equal(t, dbschema.Ascending, *columns[0].SortOrder)

	assert.Equal(t, "created", columns[1].Name)
	require.NotNil(t, columns[1].SortOrder)
	assert.Equal(t, dbschema.Descending, *columns[1].SortOrder)

	assert.Equal(t, "body", columns[2].Name)
	assert.Nil(t, columns[2].SortOrder, "a text index has no sort order to report")
}

func TestFeaturesAreReported(t *testing.T) {
	adapter := &Adapter{}

	assert.Equal(t, dbschema.MongoDB, adapter.DatabaseType())
	assert.True(t, adapter.Supports(dbadapter.FeatureSchemaInference),
		"the reason this adapter has the shape it does")
	assert.True(t, adapter.Supports(dbadapter.FeatureRowCountEstimate))
	assert.False(t, adapter.Supports(dbadapter.FeatureSchemas))
	assert.False(t, adapter.Supports(dbadapter.FeatureRoutines))
	assert.False(t, adapter.Supports(dbadapter.FeatureTriggers))
}

// TestTheClientURICarriesNoCredentialInItsHost is the leak check the Secret type
// cannot make on its own.
func TestTheClientURICarriesNoCredentialInItsHost(t *testing.T) {
	const password = "hunter2-the-actual-password"

	cfg := dbadapter.NewConnectionConfig("db.internal")
	cfg.Database = "shop"
	cfg.Username = "surveyor"
	cfg.Password = dbadapter.NewSecret([]byte(password))

	client := clientOptions(cfg)

	require.NotNil(t, client.Hosts)
	for _, host := range client.Hosts {
		assert.NotContains(t, host, password)
	}

	require.NotNil(t, client.Auth)
	assert.Equal(t, password, client.Auth.Password, "the driver still receives it")
	assert.Equal(t, "surveyor", client.Auth.Username)
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
}

// TestASubdocumentDecodedAsBsonDIsStillDescended guards the defect the first
// container run found.
//
// Which Go type a subdocument arrives as is the driver's choice, not the
// caller's: the same document can decode to bson.D rather than bson.M. When
// inference recognized only bson.M it stopped at the parent and reported no
// nested fields at all, and every unit test passed because each built its
// fixtures as bson.M by hand.
func TestASubdocumentDecodedAsBsonDIsStillDescended(t *testing.T) {
	t.Parallel()

	for name, document := range map[string]bson.M{
		"ordered document": {"address": bson.D{
			{Key: "city", Value: "London"},
			{Key: "postcode", Value: "SW1A 1AA"},
		}},
		"plain map": {"address": map[string]any{
			"city":     "London",
			"postcode": "SW1A 1AA",
		}},
	} {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			columns := infer(document).columns()

			assert.Equal(t, dbschema.JSONType(), columnNamed(t, columns, "address").DataType)
			assert.Equal(t, dbschema.StringType(nil), columnNamed(t, columns, "address.city").DataType)
			assert.Equal(t, dbschema.StringType(nil), columnNamed(t, columns, "address.postcode").DataType)
		})
	}
}

// TestANestedValueIsRenderedWhateverDocumentTypeItArrivesAs is the sampling half
// of the same driver-type problem.
//
// Normalization used to name only bson.M, so a subdocument that decoded as
// bson.D reached encoding/json holding a raw ObjectID rather than its hex.
func TestANestedValueIsRenderedWhateverDocumentTypeItArrivesAs(t *testing.T) {
	t.Parallel()

	id := bson.NewObjectID()

	for name, nested := range map[string]any{
		"bson.M":         bson.M{"ref": id},
		"bson.D":         bson.D{{Key: "ref", Value: id}},
		"map[string]any": map[string]any{"ref": id},
	} {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			rendered := normalizeValue(nested)

			document, ok := rendered.(map[string]any)
			require.True(t, ok, "a subdocument renders as a map, got %T", rendered)
			assert.Equal(t, id.Hex(), document["ref"], "the nested ObjectID is rendered, not passed through raw")
		})
	}
}
