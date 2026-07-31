package report

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// testPassword seals and opens the encrypted fixtures.
const testPassword = "report-fixture-password"

// staticPassword supplies the fixture password.
func staticPassword() ([]byte, error) {
	return []byte(testPassword), nil
}

// writeFixture writes the fixture document in the given framing and returns the
// path it landed at.
func writeFixture(t *testing.T, document *dbschema.Schema, format artifact.Format) string {
	t.Helper()

	path, err := artifact.Write(
		filepath.Join(t.TempDir(), "schema"),
		document,
		format,
		staticPassword,
	)
	require.NoError(t, err)

	return path
}

// TestEveryArtifactModeLoadsToTheSameSchema is the plan's first scenario. The
// four framings are four ways of holding one document, so a report built from
// any of them has to be the same report.
func TestEveryArtifactModeLoadsToTheSameSchema(t *testing.T) {
	document := newFixture(t)

	expected, err := Markdown(document, Options{})
	require.NoError(t, err)

	for name, format := range map[string]artifact.Format{
		"plain":                    {},
		"compressed":               {Compress: true},
		"encrypted":                {Encrypt: true},
		"compressed and encrypted": {Compress: true, Encrypt: true},
	} {
		t.Run(name, func(t *testing.T) {
			path := writeFixture(t, newFixture(t), format)

			loaded, err := Load(path, Options{Password: staticPassword})
			require.NoError(t, err)

			rendered, err := Markdown(loaded, Options{})
			require.NoError(t, err)

			assert.Equal(t, expected, rendered, "the framing does not change the report")
		})
	}
}

// TestAnArtifactCarryingACredentialFailsBeforeAnyReport is the plan's second
// scenario, and the reason loading goes through internal/artifact rather than
// through encoding/json here.
//
// The scan runs on the bytes before decoding, so a credential in a field this
// version's types do not declare is caught too -- which a scan over the decoded
// document would miss entirely.
func TestAnArtifactCarryingACredentialFailsBeforeAnyReport(t *testing.T) {
	status := dbschema.Complete()
	document := newFixture(t)
	document.Samples = []dbschema.TableSample{{
		TableName: "connections",
		Rows: []map[string]any{
			// The planted credential is the whole fixture: the scan has to
			// catch it, and a test that could not plant one would assert
			// nothing.
			//nolint:gosec // G101: deliberately planted so the scan can be seen to fire.
			{"dsn": "postgres://surveyor:hunter2@db.internal:5432/shop"},
		},
		SampleSize:       1,
		SamplingStrategy: dbschema.MostRecent(1),
		CollectedAt:      fixedTime,
		Warnings:         []string{},
		Status:           &status,
	}}

	// Writing it must fail too: Write validates first, and validation scans.
	_, err := artifact.Write(
		filepath.Join(t.TempDir(), "schema"),
		document,
		artifact.Format{},
		nil,
	)
	require.Error(t, err, "a document carrying a credential is not writable")
	assert.Contains(t, err.Error(), "credential")
}

// TestRedactionAtReportTimeRemovesSensitiveValues is the plan's fourth scenario.
// Redaction runs before rendering, so a value masked here cannot reach the
// output at all rather than being masked on the way out.
func TestRedactionAtReportTimeRemovesSensitiveValues(t *testing.T) {
	const token = "a-plain-looking-token"

	document := newFixture(t)
	document.Samples = newSamples()

	apply(document, Options{Redaction: dbschema.RedactMinimal, IncludeSamples: true})

	rendered, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.NotContains(t, rendered, token, "a column whose name denotes a secret is masked")
	assert.Contains(t, rendered, dbschema.RedactedValue)
	assert.Contains(t, rendered, "ada@example.test", "a column the mode does not cover is left alone")
}

// TestRedactionIsIdempotentAcrossCollectionAndReporting records why applying it
// twice is safe: a collector that already redacted and an operator who asks
// again must not produce a report with "[REDACTED]" masked into something else.
func TestRedactionIsIdempotentAcrossCollectionAndReporting(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	apply(document, Options{Redaction: dbschema.RedactConservative, IncludeSamples: true})

	once, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	apply(document, Options{Redaction: dbschema.RedactConservative, IncludeSamples: true})

	twice, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.Equal(t, once, twice)
}

// TestSamplesAreDroppedUnlessAskedFor covers the default. A report is usually
// about the schema, and the rows are the part that carries user data.
func TestSamplesAreDroppedUnlessAskedFor(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	apply(document, Options{})

	assert.Nil(t, document.Samples, "the rows are dropped, not left for the renderer to skip")
}

// TestAnalysisRunsOnlyWhenAskedForAndOnlyOverSamples covers both guards.
func TestAnalysisRunsOnlyWhenAskedForAndOnlyOverSamples(t *testing.T) {
	withoutRequest := newFixture(t)
	withoutRequest.Samples = newSamples()
	apply(withoutRequest, Options{IncludeSamples: true})
	assert.Empty(t, withoutRequest.QualityMetrics)

	withoutSamples := newFixture(t)
	apply(withoutSamples, Options{Analyze: true})
	assert.Empty(t, withoutSamples.QualityMetrics, "there is nothing to score")

	analyzed := newFixture(t)
	analyzed.Samples = newSamples()
	apply(analyzed, Options{
		Analyze:        true,
		IncludeSamples: true,
		Now:            func() time.Time { return fixedTime },
	})

	require.Len(t, analyzed.QualityMetrics, 1)
	assert.Equal(t, "users", analyzed.QualityMetrics[0].TableName)
	assert.Equal(t, fixedTime, analyzed.QualityMetrics[0].AnalyzedAt,
		"the clock is injected, so a golden file does not change every second")
}

// TestAnalysisRunsBeforeSamplesAreDropped is an ordering that is easy to get
// backwards: scoring rows that have already been discarded would silently
// produce no metrics whenever samples were not requested.
func TestAnalysisRunsBeforeSamplesAreDropped(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	apply(document, Options{Analyze: true, Now: func() time.Time { return fixedTime }})

	assert.Nil(t, document.Samples, "the rows are not in the report")
	assert.Len(t, document.QualityMetrics, 1, "but they were scored before being dropped")
}

// TestRedactionRunsBeforeAnalysis is the other ordering that matters. Scoring
// masked values rather than real ones would report the completeness of
// "[REDACTED]", which is a number about the redaction and not about the data.
func TestRedactionRunsBeforeAnalysis(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	apply(document, Options{
		Redaction: dbschema.RedactMinimal,
		Analyze:   true,
		Now:       func() time.Time { return fixedTime },
	})

	require.Len(t, document.QualityMetrics, 1)
	assert.Positive(t, document.QualityMetrics[0].AnalyzedRows,
		"analysis saw rows; what it saw in them is the redacted form, deliberately")
}

// TestAMalformedArtifactNamesTheFileAndTheFailure is the plan's last scenario.
// An operator running over a directory of artifacts needs to know which one
// failed, and a panic tells them nothing.
func TestAMalformedArtifactNamesTheFileAndTheFailure(t *testing.T) {
	path := filepath.Join(t.TempDir(), "broken.json")

	require.NoError(t, artifact.WriteText(path, []byte("this is not a schema document")))

	assert.NotPanics(t, func() {
		_, err := Load(path, Options{})
		require.Error(t, err)
		assert.Contains(t, err.Error(), "broken.json", "the message names the file")
	})
}

// TestATruncatedCompressedArtifactFailsCleanly covers the framing an extension
// promises but the bytes do not deliver.
func TestATruncatedCompressedArtifactFailsCleanly(t *testing.T) {
	path := filepath.Join(t.TempDir(), "truncated.zst")

	require.NoError(t, artifact.WriteText(path, []byte("not a zstd frame")))

	assert.NotPanics(t, func() {
		_, err := Load(path, Options{})
		require.Error(t, err)
		assert.Contains(t, err.Error(), "truncated.zst")
	})
}

func TestAMissingArtifactIsReportedRatherThanIgnored(t *testing.T) {
	_, err := Load(filepath.Join(t.TempDir(), "absent.json"), Options{})
	require.Error(t, err)
	assert.Contains(t, err.Error(), "absent.json")
}

func TestAnEmptyPathIsRejected(t *testing.T) {
	_, err := Load("", Options{})
	require.ErrorIs(t, err, ErrNoInput)
}

// TestAnEncryptedArtifactNeedsItsPassword keeps a wrong or missing password from
// producing an empty report instead of an error.
func TestAnEncryptedArtifactNeedsItsPassword(t *testing.T) {
	path := writeFixture(t, newFixture(t), artifact.Format{Encrypt: true})

	_, err := Load(path, Options{})
	require.Error(t, err, "no password at all")

	_, err = Load(path, Options{Password: func() ([]byte, error) {
		return []byte("the-wrong-password"), nil
	}})
	require.Error(t, err, "a wrong password fails the tag rather than decoding to nothing")
}

// TestRenderWritesTheReport covers the convenience entry point.
func TestRenderWritesTheReport(t *testing.T) {
	var out strings.Builder

	require.NoError(t, Render(&out, newFixture(t), Options{}))

	assert.Contains(t, out.String(), "# Database: shop")
}
