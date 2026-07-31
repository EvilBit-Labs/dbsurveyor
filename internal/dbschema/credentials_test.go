package dbschema

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// leakedPassword is the value planted in the credential-scan tests. Every
// assertion that a scan result does not reproduce it checks for this string.
const leakedPassword = "hunter2"

const leakedConnectionURL = "postgres://svc_reader:" + leakedPassword + "@db.internal:5432/app"

func scannableSchema() *Schema {
	schema := New(NewDatabaseInfo("app", PostgreSQL), "0.1.0", testTime)
	schema.Tables = []Table{{
		Name:    "users",
		Columns: []Column{{Name: "id", DataType: IntegerType(64, true), OrdinalPosition: 1}},
	}}

	return schema
}

func TestCredentialScanFindsConnectionStringInErrorText(t *testing.T) {
	schema := scannableSchema()
	schema.DatabaseInfo.CollectionStatus = CollectionFailure("dial failed for " + leakedConnectionURL)

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	require.Len(t, findings, 1)
	assert.Equal(t, "database_info.collection_status.error", findings[0].Path)
	assert.Equal(t, "connection-url-credentials", findings[0].Rule)
}

func TestCredentialScanFindsCredentialNestedInSampleRow(t *testing.T) {
	schema := scannableSchema()
	schema.Samples = []TableSample{{
		TableName: "audit_log",
		Rows: []map[string]any{
			{"id": 1, "detail": "connected ok"},
			{"id": 2, "detail": "retry against " + leakedConnectionURL},
		},
		SamplingStrategy: MostRecent(2),
		CollectedAt:      testTime,
		Warnings:         []string{},
	}}

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	require.Len(t, findings, 1)
	assert.Equal(t, "samples[0].rows[1].detail", findings[0].Path)
}

func TestCredentialScanFindsCredentialInTableComment(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Comment = ptr("legacy loader used PASSWORD=" + leakedPassword + " here")

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	require.Len(t, findings, 1)
	assert.Equal(t, "tables[0].comment", findings[0].Path)
	assert.Equal(t, "credential-assignment", findings[0].Rule)
}

// A column named "password" is schema metadata an operator needs to see. Only
// values are scanned, so the name alone must not trip the scanner.
func TestCredentialScanIgnoresAColumnNamedPassword(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Columns = append(schema.Tables[0].Columns, Column{
		Name:            "password_hash",
		DataType:        StringType(ptr(uint32(64))),
		OrdinalPosition: 2,
		Comment:         ptr("bcrypt digest; never the plaintext password"),
	})

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	assert.Empty(t, findings)
}

func TestCredentialScanIgnoresAConnectionStringWithoutCredentials(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Comment = ptr("loaded from postgres://db.internal:5432/app")

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	assert.Empty(t, findings)
}

// The scan exists to keep credentials out of files and terminals. A finding that
// quoted the offending value would defeat that at the moment of reporting.
func TestCredentialFindingNeverReproducesTheValue(t *testing.T) {
	schema := scannableSchema()
	schema.DatabaseInfo.CollectionStatus = CollectionFailure(leakedConnectionURL)

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	require.Len(t, findings, 1)

	rendered := findings[0].String()
	assert.NotContains(t, rendered, leakedPassword)
	assert.NotContains(t, rendered, leakedConnectionURL)

	wrapped := (&CredentialError{Finding: findings[0]}).Error()
	assert.NotContains(t, wrapped, leakedPassword)
}

func TestCredentialScanReportsEveryOffendingNode(t *testing.T) {
	schema := scannableSchema()
	schema.DatabaseInfo.CollectionStatus = CollectionFailure(leakedConnectionURL)
	schema.Tables[0].Comment = ptr(leakedConnectionURL)
	schema.AddWarning("could not reach " + leakedConnectionURL)

	findings, err := ScanForCredentials(schema)
	require.NoError(t, err)
	require.Len(t, findings, 3)

	paths := make([]string, len(findings))
	for i, finding := range findings {
		paths[i] = finding.Path
	}

	assert.Equal(t, []string{
		"collection_metadata.warnings[0]",
		"database_info.collection_status.error",
		"tables[0].comment",
	}, paths)
}

// Object keys are walked in sorted order so a document with many fields reports
// its findings the same way on every run.
func TestCredentialScanOrdersFindingsDeterministically(t *testing.T) {
	schema := scannableSchema()
	schema.Tables[0].Comment = ptr(leakedConnectionURL)
	schema.AddWarning(leakedConnectionURL)

	first, err := ScanForCredentials(schema)
	require.NoError(t, err)

	for range 5 {
		next, scanErr := ScanForCredentials(schema)
		require.NoError(t, scanErr)
		assert.Equal(t, first, next)
	}
}

func TestMatchCredentialRule(t *testing.T) {
	cases := []struct {
		name  string
		value string
		rule  string
	}{
		{"postgres url with password", leakedConnectionURL, "connection-url-credentials"},
		{"mongodb srv url", "mongodb+srv://u:p@cluster0.example.net", "connection-url-credentials"},
		{"https url with credentials", "https://admin:s3cret@example.com/api", "connection-url-credentials"},
		{"dsn keyword form", "Server=db;Database=app;Password=s3cret;", "credential-assignment"},
		{"environment form", "API_KEY=abcdef123456", "credential-assignment"},
		{"pem block", "-----BEGIN RSA PRIVATE KEY-----", "private-key-block"},
		{"url without userinfo", "postgres://db.internal:5432/app", ""},
		{"column name", "password", ""},
		{"prose mentioning a password", "the password is rotated nightly", ""},
		{"empty", "", ""},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			assert.Equal(t, testCase.rule, MatchCredentialRule(testCase.value))
		})
	}
}

func TestScanJSONForCredentialsRejectsMalformedJSON(t *testing.T) {
	_, err := ScanJSONForCredentials([]byte("{not json"))
	require.Error(t, err)
	assert.Contains(t, err.Error(), "credential scan")
}
