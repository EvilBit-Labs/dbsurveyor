package dbschema

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestValidateAcceptsAWellFormedSchema(t *testing.T) {
	require.NoError(t, populatedSchema(t).Validate())
}

func TestValidateRejectsAnOrdinalPositionOfZero(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Columns[0].OrdinalPosition = 0

	err := schema.Validate()
	require.Error(t, err)
	assert.Contains(t, err.Error(), "tables[0].columns[0].ordinal_position")
	assert.Contains(t, err.Error(), "1-based")
}

func TestValidateRejectsAnEmptyColumnName(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Columns[0].Name = ""

	require.ErrorContains(t, schema.Validate(), "tables[0].columns[0].name")
}

func TestValidateRejectsTheWrongFormatDiscriminator(t *testing.T) {
	schema := scannableSchema()
	schema.Format = "dbsurveyor/server-schema"

	require.ErrorContains(t, schema.Validate(), "format is")
}

// The constructors set only the payload belonging to the discriminator. A
// document that arrived from disk never passed through them, so Validate is
// what enforces the invariant Go's type system cannot.
func TestValidateRejectsAPayloadFromAnotherUnionVariant(t *testing.T) {
	cases := []struct {
		name    string
		mutate  func(*Schema)
		wantMsg string
	}{
		{
			name: "complete sample carrying a retry limit",
			mutate: func(s *Schema) {
				s.Samples[0].Status = &SampleStatus{State: SampleComplete, OriginalLimit: ptr(uint32(500))}
			},
			wantMsg: `"complete" must not carry original_limit`,
		},
		{
			name: "partial retry without its limit",
			mutate: func(s *Schema) {
				s.Samples[0].Status = &SampleStatus{State: SamplePartialRetry}
			},
			wantMsg: `"partial_retry" requires original_limit`,
		},
		{
			name: "failed collection without its error",
			mutate: func(s *Schema) {
				s.DatabaseInfo.CollectionStatus = CollectionStatus{State: CollectionFailed}
			},
			wantMsg: `"failed" requires error`,
		},
		{
			name: "successful collection carrying a reason",
			mutate: func(s *Schema) {
				s.DatabaseInfo.CollectionStatus = CollectionStatus{State: CollectionSuccess, Reason: ptr("why")}
			},
			wantMsg: `"success" must not carry reason`,
		},
		{
			name: "sampling kind none carrying a limit",
			mutate: func(s *Schema) {
				s.Samples[0].SamplingStrategy = SamplingStrategy{Kind: SampleNone, Limit: ptr(uint32(10))}
			},
			wantMsg: `"none" must not carry limit`,
		},
		{
			name: "integer data type without its width",
			mutate: func(s *Schema) {
				s.Tables[0].Columns[0].DataType = UnifiedDataType{Kind: TypeInteger}
			},
			wantMsg: `"integer" requires bits`,
		},
		{
			name: "boolean data type carrying a length",
			mutate: func(s *Schema) {
				s.Tables[0].Columns[0].DataType = UnifiedDataType{Kind: TypeBoolean, MaxLength: ptr(uint32(8))}
			},
			wantMsg: `"boolean" must not carry max_length`,
		},
		{
			name: "nested array element is itself checked",
			mutate: func(s *Schema) {
				s.Tables[0].Columns[0].DataType = ArrayType(UnifiedDataType{Kind: TypeCustom})
			},
			wantMsg: `"custom" requires type_name`,
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			schema := sampledSchema()
			testCase.mutate(schema)
			require.ErrorContains(t, schema.Validate(), testCase.wantMsg)
		})
	}
}

// The schema-wide roll-up exists so a report can iterate it once instead of
// walking every table. That only holds while the two agree.
func TestValidateRejectsARollUpThatDisagreesWithTheTables(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Indexes = []Index{{
		Name:      "users_pkey",
		TableName: "users",
		Columns:   []IndexColumn{{Name: "id"}},
		Unique:    true,
		Primary:   true,
	}}

	require.ErrorContains(t, schema.Validate(), "roll-up holds 0 indexes but the tables hold 1")

	schema.AggregateIndexesAndConstraints()
	require.NoError(t, schema.Validate())
}

// ColumnNames reads the first row alone, so a sample with ragged rows silently
// drops columns everywhere downstream. It is defined as malformed instead.
func TestValidateRejectsRaggedSampleRows(t *testing.T) {
	schema := sampledSchema()
	schema.Samples[0].Rows = []map[string]any{
		{"id": 1, "email": "a@example.com"},
		{"id": 2},
	}

	require.ErrorContains(t, schema.Validate(), "samples[0].rows[1]")
}

func TestValidateRejectsAForeignKeyWithUnpairedColumns(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].ForeignKeys = []ForeignKey{{
		Columns:           []string{"org_id", "tenant_id"},
		ReferencedTable:   "orgs",
		ReferencedColumns: []string{"id"},
	}}

	require.ErrorContains(t, schema.Validate(), "positionally paired")
}

// The verification U4 owes: a planted credential fails validation, which is what
// every load path relies on rather than deciding to scan for itself.
func TestValidateRejectsAPlantedCredential(t *testing.T) {
	schema := sampledSchema()
	schema.Samples[0].Rows[0]["email"] = leakedConnectionURL

	err := schema.Validate()
	require.Error(t, err)

	var credentialErr *CredentialError
	require.ErrorAs(t, err, &credentialErr)
	assert.Equal(t, "samples[0].rows[0].email", credentialErr.Finding.Path)
	assert.NotContains(t, err.Error(), leakedPassword)
}

func TestServerSchemaValidateRejectsAPlantedCredentialInAnyDatabase(t *testing.T) {
	server := NewServerSchema(ServerInfo{
		ServerType:     PostgreSQL,
		Version:        "16.2",
		Host:           "db.internal",
		ConnectionUser: "svc_reader",
		CollectionMode: SingleDatabase(),
	}, "0.1.0", testTime)

	clean := scannableSchema()
	planted := scannableSchema()
	planted.DatabaseInfo.CollectionStatus = CollectionFailure(leakedConnectionURL)
	server.Databases = []Schema{*clean, *planted}

	err := server.Validate()
	require.Error(t, err)
	assert.Contains(t, err.Error(), "databases[1]")
	assert.NotContains(t, err.Error(), leakedPassword)
}

func TestServerSchemaValidateAcceptsAWellFormedDocument(t *testing.T) {
	server := NewServerSchema(ServerInfo{
		ServerType:     PostgreSQL,
		Version:        "16.2",
		Host:           "db.internal",
		ConnectionUser: "svc_reader",
		CollectionMode: MultiDatabase(2, 1, 1),
	}, "0.1.0", testTime)
	server.Databases = []Schema{*scannableSchema()}

	require.NoError(t, server.Validate())
}

// One pass reports every problem, so a malformed document does not have to be
// fixed and revalidated one field at a time.
func TestValidateReportsEveryProblemAtOnce(t *testing.T) {
	schema := scannableSchema()
	schema.FormatVersion = "0.9"
	schema.DatabaseInfo.Name = ""
	schema.Tables[0].Columns[0].OrdinalPosition = 0

	err := schema.Validate()
	require.Error(t, err)

	joined, ok := err.(interface{ Unwrap() []error })
	require.True(t, ok, "Validate must join its problems")
	assert.Len(t, joined.Unwrap(), 3)
}

func TestValidationErrorCarriesItsPath(t *testing.T) {
	err := error(&ValidationError{Path: "tables[0]", Detail: "table name must not be empty"})
	assert.Equal(t, "tables[0]: table name must not be empty", err.Error())

	var validationErr *ValidationError
	require.ErrorAs(t, err, &validationErr)
	assert.Equal(t, "tables[0]", validationErr.Path)
}

// sampledSchema is a valid single-table schema carrying one sample, used by the
// tests that mutate one field at a time to prove Validate catches it.
func sampledSchema() *Schema {
	schema := scannableSchema()
	schema.Tables[0].Columns = append(schema.Tables[0].Columns, Column{
		Name:            "email",
		DataType:        StringType(ptr(uint32(255))),
		OrdinalPosition: 2,
	})
	schema.Samples = []TableSample{{
		TableName:  "users",
		SchemaName: ptr("public"),
		Rows: []map[string]any{
			{"id": 1, "email": "ada@example.com"},
			{"id": 2, "email": "grace@example.com"},
		},
		SampleSize:       2,
		SamplingStrategy: MostRecent(2),
		Ordering:         &OrderingStrategy{Kind: OrderPrimaryKey, Columns: []string{"id"}},
		CollectedAt:      testTime,
		Warnings:         []string{},
		Status:           ptr(Complete()),
	}}

	return schema
}
