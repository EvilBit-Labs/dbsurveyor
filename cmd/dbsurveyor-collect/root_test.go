package main

import (
	"bytes"
	"context"
	"database/sql"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/sqlite"
	"github.com/EvilBit-Labs/dbsurveyor/internal/survey"
)

// plaintext is the password no output may ever contain.
const plaintext = "hunter2-the-actual-password"

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

// TestHelpAndVersionNeedNoDatabase covers the plan's requirement that both exit
// cleanly without a connection. A tool whose --help needs a database is a tool
// nobody can find out how to use.
func TestHelpAndVersionNeedNoDatabase(t *testing.T) {
	help, err := execute(t, "--help")
	require.NoError(t, err)
	assert.Equal(t, 0, exitCode(err))
	assert.Contains(t, help, "dbsurveyor-collect")
	assert.Contains(t, help, "--sample")

	version, err := execute(t, "--version")
	require.NoError(t, err)
	assert.Equal(t, 0, exitCode(err))
	assert.Contains(t, version, "1.2.3")
}

// TestHelpNamesEverySupportedScheme keeps the documented surface honest: an
// operator reading --help learns which connection strings will work.
func TestHelpNamesEverySupportedScheme(t *testing.T) {
	help, err := execute(t, "--help")
	require.NoError(t, err)

	for _, scheme := range survey.SupportedSchemes() {
		assert.Contains(t, help, scheme)
	}
}

// TestTheConnectionStringComesFromTheEnvironmentOrTheArgument covers the
// precedence the plan specifies. The argument wins because an operator who typed
// one meant it, and a stale DATABASE_URL in a shell is a common way to survey
// the wrong database.
func TestTheConnectionStringComesFromTheEnvironmentOrTheArgument(t *testing.T) {
	t.Setenv(connectionEnvVar, "postgres://db.internal/from-the-environment")

	fromEnv, err := resolveTarget(nil)
	require.NoError(t, err)
	assert.Equal(t, "from-the-environment", fromEnv.Connection.Database)

	fromArgument, err := resolveTarget([]string{"mysql://db.internal/from-the-argument"})
	require.NoError(t, err)
	assert.Equal(t, "from-the-argument", fromArgument.Connection.Database)
	assert.Equal(t, survey.SchemeMySQL, fromArgument.Scheme)
}

// TestAnEmptyArgumentFallsBackToTheEnvironment keeps a shell that expanded an
// unset variable from looking like an explicit empty target.
func TestAnEmptyArgumentFallsBackToTheEnvironment(t *testing.T) {
	t.Setenv(connectionEnvVar, "postgres://db.internal/shop")

	target, err := resolveTarget([]string{"  "})
	require.NoError(t, err)
	assert.Equal(t, "shop", target.Connection.Database)
}

func TestNoConnectionStringAnywhereIsAUsageError(t *testing.T) {
	t.Setenv(connectionEnvVar, "")

	_, err := resolveTarget(nil)
	require.ErrorIs(t, err, ErrNoConnectionString)
	assert.Contains(t, err.Error(), connectionEnvVar, "the message says where else it could come from")
	assert.Equal(t, usageExitCode, exitCode(err))
}

// TestMutuallyExclusiveFlagsFailWithAUsageError covers the plan's requirement
// and the exit code a wrapping script reads.
func TestMutuallyExclusiveFlagsFailWithAUsageError(t *testing.T) {
	_, err := execute(t, "--compress", "--encrypt", "sqlite:///tmp/x.db")

	require.Error(t, err)
	assert.Contains(t, err.Error(), "compress")
	assert.Contains(t, err.Error(), "encrypt")
}

func TestAnUnknownSchemeIsAUsageError(t *testing.T) {
	_, err := execute(t, "cassandra://db.internal/shop")

	require.ErrorIs(t, err, survey.ErrUnknownScheme)
	assert.Equal(t, usageExitCode, exitCode(err))
	assert.Contains(t, err.Error(), "postgres", "the message lists what would have worked")
}

// TestAMalformedConnectionStringNeverEchoesItself is R17 at the command layer,
// which is where a connection string is most likely to reach a terminal.
func TestAMalformedConnectionStringNeverEchoesItself(t *testing.T) {
	raw := "postgres://surveyor:" + plaintext + "@db.internal\x7f/shop"

	out, err := execute(t, raw)

	require.ErrorIs(t, err, survey.ErrMalformedTarget)
	assert.NotContains(t, err.Error(), plaintext)
	assert.NotContains(t, out, plaintext)
	assert.Equal(t, usageExitCode, exitCode(err))
}

func TestAnUnknownRedactModeIsRejectedBeforeConnecting(t *testing.T) {
	_, err := execute(t, "--redact-mode", "aggressive", "postgres://db.internal/shop")

	require.ErrorContains(t, err, "aggressive")
	assert.Contains(t, err.Error(), "conservative", "the message lists the modes that exist")
}

// TestEveryRedactModeIsAccepted keeps the flag's documented values and the
// dbschema enum from drifting apart.
func TestEveryRedactModeIsAccepted(t *testing.T) {
	for _, mode := range []dbschema.RedactionMode{
		dbschema.RedactNone,
		dbschema.RedactMinimal,
		dbschema.RedactBalanced,
		dbschema.RedactConservative,
	} {
		opts := &options{redaction: string(mode), sampleSiz: 10}

		built, err := opts.surveyOptions(survey.Target{Scheme: survey.SchemePostgres}, "test", &bytes.Buffer{})
		require.NoError(t, err, "mode %q", mode)
		assert.Equal(t, mode, built.Redaction)
	}
}

// TestTheSamplingFlagsReachTheSurvey is the plan's requirement that --sample and
// --throttle change behavior. They are checked where they become configuration,
// since internal/survey already proves the configuration changes what happens.
func TestTheSamplingFlagsReachTheSurvey(t *testing.T) {
	opts := &options{
		redaction: string(dbschema.RedactBalanced),
		sample:    true,
		sampleSiz: 42,
		throttle:  250,
		maxTables: 7,
	}

	built, err := opts.surveyOptions(survey.Target{Scheme: survey.SchemePostgres}, "test", &bytes.Buffer{})
	require.NoError(t, err)

	assert.True(t, built.Collection.Sample)
	assert.Equal(t, uint32(42), built.Collection.Sampling.SampleSize)
	assert.Equal(t, opts.throttle, built.Collection.Sampling.Throttle)
	assert.Equal(t, 7, built.MaxTablesSampled)
}

// TestSamplingIsOffByDefault records the decision rather than leaving it to be
// discovered: sampling is the only part of a survey that reads user data.
func TestSamplingIsOffByDefault(t *testing.T) {
	command := newRootCommand("test")

	sample, err := command.Flags().GetBool("sample")
	require.NoError(t, err)
	assert.False(t, sample)
}

// TestTheCollectorVersionIsStampedIntoTheDocument keeps a document able to say
// what produced it.
func TestTheCollectorVersionIsStampedIntoTheDocument(t *testing.T) {
	opts := &options{redaction: string(dbschema.RedactBalanced), sampleSiz: 10}

	built, err := opts.surveyOptions(survey.Target{Scheme: survey.SchemePostgres}, "1.2.3", &bytes.Buffer{})
	require.NoError(t, err)

	assert.Equal(t, "1.2.3", built.Collection.CollectorVersion)
}

// TestQuietSuppressesProgressEntirely covers the flag, and that the suppression
// is a reporter rather than a discarded writer -- which would still do the work.
func TestQuietSuppressesProgressEntirely(t *testing.T) {
	var out bytes.Buffer

	quiet := (&options{quiet: true}).reporter(&out)
	quiet.Start(1, "collecting")
	quiet.Step("users")
	quiet.Warn("something")
	quiet.Done()

	assert.Empty(t, out.String())

	loud := (&options{}).reporter(&out)
	loud.Start(1, "collecting")
	assert.Contains(t, out.String(), "collecting")
}

// TestEveryEngineIsWired is the other half of R11: the registry is the only
// place engines are enabled, so it has to name all six.
func TestEveryEngineIsWired(t *testing.T) {
	registry := adapters()

	for _, scheme := range []string{
		survey.SchemePostgres,
		survey.SchemeMySQL,
		survey.SchemeSQLite,
		survey.SchemeMongoDB,
		survey.SchemeSQLServer,
		survey.SchemeOracle,
	} {
		assert.Contains(t, registry, scheme)
		assert.NotNil(t, registry[scheme])
	}

	assert.Len(t, registry, 6, "six engines, no more and no fewer")
}

// TestAFailedConstructorReturnsANilAdapterRatherThanATypedNil is why the wiring
// wrapper exists. A constructor returning a typed nil pointer and no error would
// produce a non-nil interface holding a nil pointer, and the survey would call a
// method on it instead of reporting the failure.
func TestAFailedConstructorReturnsANilAdapterRatherThanATypedNil(t *testing.T) {
	failing := open(func(context.Context, dbadapter.ConnectionConfig) (*sqlite.Adapter, error) {
		return nil, errors.New("dial tcp: connection refused")
	})

	adapter, err := failing(t.Context(), dbadapter.ConnectionConfig{})

	require.Error(t, err)
	assert.Nil(t, adapter, "a failed construction hands back nothing, not a typed nil in an interface")
}

func TestExitCodesDistinguishUsageFromFailure(t *testing.T) {
	assert.Equal(t, 0, exitCode(nil))
	assert.Equal(t, usageExitCode, exitCode(survey.ErrEmptyTarget))
	assert.Equal(t, usageExitCode, exitCode(survey.ErrMissingHost))
	assert.Equal(t, usageExitCode, exitCode(ErrNoConnectionString))
	assert.Equal(t, failureExitCode, exitCode(errors.New("the server closed the connection")),
		"a wrapping script has to tell a bad argument from a bad database")
}

func TestTheResultIsReportedWithoutAnyConnectionDetail(t *testing.T) {
	var out bytes.Buffer

	report(&out, survey.Result{
		Path:     "schema.json",
		Tables:   12,
		Objects:  40,
		Sampled:  3,
		Warnings: []string{"one table could not be read"},
	})

	written := out.String()
	assert.Contains(t, written, "wrote schema.json")
	assert.Contains(t, written, "12 tables")
	assert.Contains(t, written, "40 objects")
	assert.Contains(t, written, "3 sampled")
	assert.Contains(t, written, "1 warnings")
	assert.NotContains(t, strings.ToLower(written), "password")
}

// fixtureDDL is a small real database, so the end-to-end test below exercises
// the actual SQLite adapter rather than a double.
const fixtureDDL = `
CREATE TABLE users (
    id       INTEGER PRIMARY KEY,
    email    TEXT NOT NULL,
    password TEXT NOT NULL
);
CREATE INDEX users_by_email ON users(email);
INSERT INTO users (email, password) VALUES
    ('ada@example.test',   'first-secret'),
    ('grace@example.test', 'second-secret');
`

// newFixtureDatabase writes a real SQLite database and returns its path.
func newFixtureDatabase(t *testing.T) string {
	t.Helper()

	path := filepath.Join(t.TempDir(), "fixture.db")

	db, err := sql.Open("sqlite", path)
	require.NoError(t, err)

	defer func() { require.NoError(t, db.Close()) }()

	_, err = db.ExecContext(t.Context(), fixtureDDL)
	require.NoError(t, err)

	return path
}

// TestAnEndToEndCollectionWritesACleanArtifact is the plan's last scenario, run
// through the real command against a real database.
//
// It is the only test that exercises flag parsing, target resolution, adapter
// wiring, collection, sampling, redaction, and the artifact write together --
// each of which is tested in isolation elsewhere, and none of which proves they
// are connected.
func TestAnEndToEndCollectionWritesACleanArtifact(t *testing.T) {
	output := filepath.Join(t.TempDir(), "survey")

	out, err := execute(t,
		"--output", output,
		"--sample",
		"--sample-size", "1",
		"--redact-mode", string(dbschema.RedactMinimal),
		"sqlite://"+filepath.ToSlash(newFixtureDatabase(t)),
	)
	require.NoError(t, err)

	written := output + ".json"
	require.FileExists(t, written)
	assert.Contains(t, out, "wrote "+written)
	assert.Contains(t, out, "1 tables")

	// The load path is the enforcement point: ReadSchema scans the bytes for
	// credentials before decoding, and validation scans again.
	loaded, err := artifact.ReadSchema(written, nil)
	require.NoError(t, err, "the artifact loads, which means its credential scan came back clean")
	require.NoError(t, loaded.Validate())

	require.Len(t, loaded.Tables, 1)
	assert.Equal(t, "users", loaded.Tables[0].Name)

	require.Len(t, loaded.Samples, 1)
	require.Len(t, loaded.Samples[0].Rows, 1, "--sample-size 1 reached the engine")
	assert.Equal(t, dbschema.RedactedValue, loaded.Samples[0].Rows[0]["password"],
		"--redact-mode reached the sampled rows")
	assert.Contains(t, loaded.Samples[0].Rows[0]["email"], "@example.test",
		"a column the mode does not cover is left alone")
}

// TestACollectionWithoutSampleReadsNoRows records the default that matters most:
// sampling is the only part of a survey that touches user data.
func TestACollectionWithoutSampleReadsNoRows(t *testing.T) {
	output := filepath.Join(t.TempDir(), "survey")

	_, err := execute(t, "--output", output, "sqlite://"+filepath.ToSlash(newFixtureDatabase(t)))
	require.NoError(t, err)

	contents, err := os.ReadFile(output + ".json")
	require.NoError(t, err)

	assert.Contains(t, string(contents), "users", "the schema is there")
	assert.NotContains(t, string(contents), "ada@example.test", "no row value is")
}
