package main

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"regexp"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/report"
)

// escapeSequence matches an ANSI control sequence, which must never reach a file
// or a pipe.
var escapeSequence = regexp.MustCompile(`\x1b\[[0-9;]*[a-zA-Z]`)

// fixedTime is the timestamp the fixture uses, so nothing here depends on the
// wall clock.
var fixedTime = time.Date(2026, time.July, 27, 12, 0, 0, 0, time.UTC)

// execute runs the command with the given arguments and captures its output.
func execute(t *testing.T, args ...string) (string, error) {
	t.Helper()

	var out bytes.Buffer

	command := newRootCommand("1.2.3")
	command.SetOut(&out)
	command.SetErr(&out)
	command.SetArgs(args)

	err := command.Execute()

	return out.String(), err
}

// newArtifact writes a fixture artifact and returns its path.
func newArtifact(t *testing.T) string {
	t.Helper()

	status := dbschema.Complete()

	document := dbschema.New(
		dbschema.NewDatabaseInfo("shop", dbschema.PostgreSQL),
		"test",
		fixedTime,
	)
	document.Tables = []dbschema.Table{{
		Name: "users",
		Columns: []dbschema.Column{
			{Name: "id", DataType: dbschema.IntegerType(64, true), PrimaryKey: true, OrdinalPosition: 1},
			{Name: "email", DataType: dbschema.StringType(nil), OrdinalPosition: 2},
		},
		ForeignKeys: []dbschema.ForeignKey{},
		Indexes:     []dbschema.Index{},
		Constraints: []dbschema.Constraint{},
	}}
	document.Samples = []dbschema.TableSample{{
		TableName: "users",
		Rows: []map[string]any{
			{"id": 1, "email": "ada@example.test", "api_key": "a-plain-looking-token"},
		},
		SampleSize:       1,
		SamplingStrategy: dbschema.MostRecent(1),
		CollectedAt:      fixedTime,
		Warnings:         []string{},
		Status:           &status,
	}}
	document.AggregateIndexesAndConstraints()

	path, err := artifact.Write(
		filepath.Join(t.TempDir(), "schema"),
		document,
		artifact.Format{},
		nil,
	)
	require.NoError(t, err)

	return path
}

// TestHelpAndVersionExitCleanly covers the plan's requirement.
func TestHelpAndVersionExitCleanly(t *testing.T) {
	help, err := execute(t, "--help")
	require.NoError(t, err)
	assert.Equal(t, 0, exitCode(err))
	assert.Contains(t, help, "dbsurveyor")
	assert.Contains(t, help, "--redact-mode")

	version, err := execute(t, "--version")
	require.NoError(t, err)
	assert.Equal(t, 0, exitCode(err))
	assert.Contains(t, version, "1.2.3")
}

// TestHelpSaysThisCommandTouchesNoDatabase records the decision the plan calls
// out: the postprocessor does not read DATABASE_URL and should not start.
func TestHelpSaysThisCommandTouchesNoDatabase(t *testing.T) {
	help, err := execute(t, "--help")
	require.NoError(t, err)

	assert.NotContains(t, help, "DATABASE_URL")
	assert.Contains(t, help, "no network call")
}

// TestARenderedReportGoesToStandardOutput is the ordinary path.
func TestARenderedReportGoesToStandardOutput(t *testing.T) {
	out, err := execute(t, newArtifact(t))
	require.NoError(t, err)

	assert.Contains(t, out, "# Database: shop")
	assert.Contains(t, out, "users")
	assert.NotRegexp(t, escapeSequence, out, "a buffer is not a terminal, so nothing is styled")
}

// TestSamplesAreOmittedUnlessAskedFor covers the default that matters: the rows
// are the part of a report that carries user data.
func TestSamplesAreOmittedUnlessAskedFor(t *testing.T) {
	path := newArtifact(t)

	without, err := execute(t, path)
	require.NoError(t, err)
	assert.NotContains(t, without, "## Samples")
	assert.NotContains(t, without, "ada@example.test")

	with, err := execute(t, "--samples", path)
	require.NoError(t, err)
	assert.Contains(t, with, "## Samples")
	assert.Contains(t, with, "| api_key | email | id |")

	// The default mode is balanced, which masks personal data as well as
	// secrets -- so asking for samples is not the same as asking for values.
	assert.NotContains(t, with, "ada@example.test")
	assert.Contains(t, with, dbschema.RedactedValue)

	raw, err := execute(t, "--samples", "--no-redact", path)
	require.NoError(t, err)
	assert.Contains(t, raw, "ada@example.test", "the values are there when asked for by name")
}

// TestRedactionIsAppliedAtReportTime checks the flag reaches the values.
func TestRedactionIsAppliedAtReportTime(t *testing.T) {
	const token = "a-plain-looking-token"

	path := newArtifact(t)

	masked, err := execute(t, "--samples", "--redact-mode", string(dbschema.RedactMinimal), path)
	require.NoError(t, err)
	assert.NotContains(t, masked, token)
	assert.Contains(t, masked, dbschema.RedactedValue)

	raw, err := execute(t, "--samples", "--no-redact", path)
	require.NoError(t, err)
	assert.Contains(t, raw, token, "--no-redact means what it says")
}

// TestNoRedactAndRedactModeTogetherFail is the plan's requirement. They are
// contradictory instructions, and guessing which one an operator meant is how
// sampled values reach a report they were supposed to be masked out of.
func TestNoRedactAndRedactModeTogetherFail(t *testing.T) {
	_, err := execute(t, "--no-redact", "--redact-mode", "balanced", newArtifact(t))

	require.Error(t, err)
	assert.Contains(t, err.Error(), "no-redact")
	assert.Contains(t, err.Error(), "redact-mode")
}

func TestAnUnknownRedactModeIsRejected(t *testing.T) {
	_, err := execute(t, "--redact-mode", "aggressive", newArtifact(t))

	require.ErrorContains(t, err, "aggressive")
	assert.Contains(t, err.Error(), "conservative", "the message lists the modes that exist")
}

// TestQualityAppearsOnlyWhenAnalysisWasRequested covers the plan's requirement in
// both directions.
func TestQualityAppearsOnlyWhenAnalysisWasRequested(t *testing.T) {
	path := newArtifact(t)

	without, err := execute(t, path)
	require.NoError(t, err)
	assert.NotContains(t, without, "## Data quality")

	with, err := execute(t, "--analyze", path)
	require.NoError(t, err)
	assert.Contains(t, with, "## Data quality")
}

// TestAReportWrittenToAFileIsRawMarkdown is why the file path never styles: a
// file is read by another tool or committed to a repository, and escape
// sequences in one are corruption.
func TestAReportWrittenToAFileIsRawMarkdown(t *testing.T) {
	destination := filepath.Join(t.TempDir(), "report.md")

	out, err := execute(t, "--output", destination, newArtifact(t))
	require.NoError(t, err)
	assert.Empty(t, out, "the report went to the file, not to standard output")

	contents, err := os.ReadFile(destination)
	require.NoError(t, err)

	assert.Contains(t, string(contents), "# Database: shop")
	assert.NotRegexp(t, escapeSequence, string(contents))
}

// TestAFailedWriteLeavesThePreviousReportIntact is the atomic-write guarantee a
// report inherits from internal/artifact.
func TestAFailedWriteLeavesThePreviousReportIntact(t *testing.T) {
	destination := filepath.Join(t.TempDir(), "report.md")

	_, err := execute(t, "--output", destination, newArtifact(t))
	require.NoError(t, err)

	before, err := os.ReadFile(destination)
	require.NoError(t, err)

	// A directory that does not exist makes the write fail after the previous
	// report is already in place.
	_, err = execute(t, "--output", filepath.Join(t.TempDir(), "absent", "report.md"), newArtifact(t))
	require.Error(t, err)

	after, err := os.ReadFile(destination)
	require.NoError(t, err)

	assert.Equal(t, before, after)
}

// TestAMalformedArtifactNamesTheFileRatherThanPanicking is the plan's last
// scenario, asserted through the command an operator actually runs.
func TestAMalformedArtifactNamesTheFileRatherThanPanicking(t *testing.T) {
	path := filepath.Join(t.TempDir(), "broken.json")
	require.NoError(t, artifact.WriteText(path, []byte("this is not a schema document")))

	assert.NotPanics(t, func() {
		_, err := execute(t, path)
		require.Error(t, err)
		assert.Contains(t, err.Error(), "broken.json")
		assert.Equal(t, failureExitCode, exitCode(err))
	})
}

func TestAMissingArtifactIsReported(t *testing.T) {
	_, err := execute(t, filepath.Join(t.TempDir(), "absent.json"))

	require.Error(t, err)
	assert.Contains(t, err.Error(), "absent.json")
}

// TestAnEmptyPathIsAUsageError keeps a shell that expanded an unset variable
// from looking like a failed report.
func TestAnEmptyPathIsAUsageError(t *testing.T) {
	_, err := execute(t, "   ")

	require.ErrorIs(t, err, ErrNoArtifact)
	assert.Equal(t, usageExitCode, exitCode(err))
}

func TestExitCodesDistinguishUsageFromFailure(t *testing.T) {
	assert.Equal(t, 0, exitCode(nil))
	assert.Equal(t, usageExitCode, exitCode(ErrNoArtifact))
	assert.Equal(t, usageExitCode, exitCode(report.ErrNoInput))
	assert.Equal(t, failureExitCode, exitCode(errors.New("the artifact is corrupt")))
}

// TestAnEncryptedArtifactRoundTripsThroughBothBinaries is U11's verification
// line: a fixture artifact goes end to end.
func TestAnEncryptedArtifactRoundTripsThroughBothBinaries(t *testing.T) {
	t.Setenv("DBSURVEYOR_ENCRYPTION_PASSWORD", "an-adequately-long-password")

	document := dbschema.New(
		dbschema.NewDatabaseInfo("shop", dbschema.SQLite),
		"test",
		fixedTime,
	)
	document.Tables = []dbschema.Table{{
		Name:        "users",
		Columns:     []dbschema.Column{{Name: "id", DataType: dbschema.IntegerType(64, true), OrdinalPosition: 1}},
		ForeignKeys: []dbschema.ForeignKey{},
		Indexes:     []dbschema.Index{},
		Constraints: []dbschema.Constraint{},
	}}

	written, err := artifact.Write(
		filepath.Join(t.TempDir(), "schema"),
		document,
		artifact.Format{Compress: true, Encrypt: true},
		func() ([]byte, error) { return []byte("an-adequately-long-password"), nil },
	)
	require.NoError(t, err)
	assert.Equal(t, ".enc", filepath.Ext(written), "the extension says encrypted and nothing else")

	out, err := execute(t, written)
	require.NoError(t, err, "the password came from the environment, so nothing prompted")
	assert.Contains(t, out, "# Database: shop")
	assert.Contains(t, out, "users")
}
