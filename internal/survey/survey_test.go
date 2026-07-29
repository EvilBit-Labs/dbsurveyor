package survey

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// plaintext is the password no output may ever contain.
const plaintext = "hunter2-the-actual-password"

// fakeAdapter is a database that never existed.
//
// The survey's whole job is orchestration, so the only way to test it is to
// remove the database -- and the fact that this is possible at all is the
// property R11 buys: the survey knows adapters through a map it was handed, so a
// map with one fake in it is a complete substitute for six real engines.
type fakeAdapter struct {
	schema *dbschema.Schema
	// sampled records the tables SampleTable was asked for, in order.
	sampled []dbadapter.TableRef
	// unreadable names tables that fail to sample.
	unreadable map[string]bool
	// sampleSize records the row limit each sample was asked for.
	sampleSize uint32
	closed     bool
	pingErr    error
}

func (f *fakeAdapter) DatabaseType() dbschema.DatabaseType { return dbschema.SQLite }
func (f *fakeAdapter) Supports(dbadapter.Feature) bool     { return true }
func (f *fakeAdapter) Ping(context.Context) error          { return f.pingErr }

func (f *fakeAdapter) Close() error {
	f.closed = true

	return nil
}

func (f *fakeAdapter) CollectSchema(
	context.Context,
	dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	return f.schema, nil
}

func (f *fakeAdapter) SampleTable(
	_ context.Context,
	table dbadapter.TableRef,
	cfg dbadapter.SamplingConfig,
) (dbschema.TableSample, error) {
	f.sampled = append(f.sampled, table)
	f.sampleSize = cfg.SampleSize

	if f.unreadable[table.Table] {
		return dbschema.TableSample{}, errors.New("permission denied for relation " + table.Table)
	}

	status := dbschema.Complete()
	rows := make([]map[string]any, 0, cfg.SampleSize)

	for i := range int(cfg.SampleSize) {
		// The password column is the fixture the redaction tests need: a column
		// whose *name* denotes a secret is what the minimal mode masks.
		//nolint:gosec // G101: fabricated fixture data in a fake adapter, not a credential.
		rows = append(rows, map[string]any{
			"id":       i,
			"note":     "a plain value",
			"password": "s3cret-in-a-column",
		})
	}

	return dbschema.TableSample{
		TableName:        table.Table,
		Rows:             rows,
		SampleSize:       cfg.SampleSize,
		SamplingStrategy: dbschema.MostRecent(cfg.SampleSize),
		CollectedAt:      time.Now(),
		Warnings:         []string{},
		Status:           &status,
	}, nil
}

// newFakeSchema builds a schema with the given table names.
func newFakeSchema(names ...string) *dbschema.Schema {
	document := dbschema.New(
		dbschema.NewDatabaseInfo("fixture", dbschema.SQLite),
		"test",
		time.Now(),
	)

	for _, name := range names {
		document.Tables = append(document.Tables, dbschema.Table{
			Name:        name,
			Columns:     []dbschema.Column{{Name: "id", DataType: dbschema.IntegerType(64, true), OrdinalPosition: 1}},
			ForeignKeys: []dbschema.ForeignKey{},
			Indexes:     []dbschema.Index{},
			Constraints: []dbschema.Constraint{},
		})
	}

	return document
}

// recorder is a progress reporter that keeps what it was told.
type recorder struct {
	lines []string
}

func (r *recorder) Start(total int, label string) {
	r.lines = append(r.lines, fmt.Sprintf("start %d %s", total, label))
}

func (r *recorder) Step(label string)   { r.lines = append(r.lines, "step "+label) }
func (r *recorder) Warn(message string) { r.lines = append(r.lines, "warn "+message) }
func (r *recorder) Done()               { r.lines = append(r.lines, "done") }

func (r *recorder) joined() string { return strings.Join(r.lines, "\n") }

// newOptions returns options that survey the given fake into a temporary file.
func newOptions(t *testing.T, adapter *fakeAdapter) Options {
	t.Helper()

	target, err := ParseTarget("sqlite:///" + filepath.ToSlash(filepath.Join(t.TempDir(), "fixture.db")))
	require.NoError(t, err)

	collection := dbadapter.NewCollectionConfig(target.Connection.Host)
	collection.Connection = target.Connection

	return Options{
		Target:     target,
		Adapters:   Registry{SchemeSQLite: constantConstructor(adapter)},
		Collection: collection,
		Output:     filepath.Join(t.TempDir(), "schema"),
		Redaction:  dbschema.RedactNone,
	}
}

// constantConstructor returns a constructor that always hands back the same
// adapter.
func constantConstructor(adapter *fakeAdapter) Constructor {
	return func(context.Context, dbadapter.ConnectionConfig) (dbadapter.Adapter, error) {
		return adapter, nil
	}
}

// TestASurveyRunsAgainstAnInjectedAdapter is the claim R11 exists for: the only
// coupling between orchestration and an engine is the map, so a fake in the map
// is a complete substitute for a database.
func TestASurveyRunsAgainstAnInjectedAdapter(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("users", "orders")}

	result, err := Run(t.Context(), newOptions(t, adapter))
	require.NoError(t, err)

	assert.Equal(t, 2, result.Tables)
	assert.Equal(t, 2, result.Objects)
	assert.Zero(t, result.Sampled, "sampling is off by default")
	assert.True(t, adapter.closed, "the adapter is closed before the artifact is written")

	require.FileExists(t, result.Path)
	assert.True(t, strings.HasSuffix(result.Path, ".json"))
}

// TestTheWrittenArtifactLoadsAndIsClean checks the whole path: what the survey
// wrote has to survive the same load an operator's postprocessor performs,
// including the recursive credential scan.
func TestTheWrittenArtifactLoadsAndIsClean(t *testing.T) {
	result, err := Run(t.Context(), newOptions(t, &fakeAdapter{schema: newFakeSchema("users")}))
	require.NoError(t, err)

	loaded, err := artifact.ReadSchema(result.Path, nil)
	require.NoError(t, err)

	assert.Len(t, loaded.Tables, 1)
	require.NoError(t, loaded.Validate())
}

func TestARunWithAnUnregisteredSchemeFails(t *testing.T) {
	options := newOptions(t, &fakeAdapter{schema: newFakeSchema()})
	options.Target.Scheme = "cassandra"

	_, err := Run(t.Context(), options)
	require.ErrorIs(t, err, ErrUnknownScheme)
	assert.Contains(t, err.Error(), "cassandra")
}

func TestOptionsAreValidatedBeforeAnythingConnects(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema()}

	options := newOptions(t, adapter)
	options.Output = ""

	_, err := Run(t.Context(), options)
	require.ErrorIs(t, err, ErrNoOutput)

	options = newOptions(t, adapter)
	options.Adapters = nil
	_, err = Run(t.Context(), options)
	require.ErrorIs(t, err, ErrNoAdapters)
}

// TestSamplingHonorsTheConfiguredSize is one half of the plan's requirement that
// --sample and --sample-size change behavior rather than emit a warning.
func TestSamplingHonorsTheConfiguredSize(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("users", "orders")}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Collection.Sampling = dbadapter.NewSamplingConfig(7)

	result, err := Run(t.Context(), options)
	require.NoError(t, err)

	assert.Equal(t, 2, result.Sampled)
	assert.Equal(t, uint32(7), adapter.sampleSize, "the row limit reached the adapter")
	assert.Equal(t, []dbadapter.TableRef{{Table: "users"}, {Table: "orders"}}, adapter.sampled)
}

// TestTheThrottleDelaysBetweenSamples is the other half. The delay is asserted
// as elapsed time rather than as a flag value, because a throttle that is stored
// and never applied is exactly the bug this replaces.
func TestTheThrottleDelaysBetweenSamples(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("a", "b", "c")}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Collection.Sampling = dbadapter.NewSamplingConfig(1)
	options.Collection.Sampling.Throttle = 20 * time.Millisecond

	started := time.Now()

	_, err := Run(t.Context(), options)
	require.NoError(t, err)

	assert.GreaterOrEqual(t, time.Since(started), 60*time.Millisecond,
		"three tables at a 20ms throttle cannot finish sooner")
}

// TestAThrottledSurveyIsInterruptible is why the throttle selects on the context
// rather than sleeping. A run with a long throttle over many tables would
// otherwise ignore an interrupt for as long as the current sleep had left.
func TestAThrottledSurveyIsInterruptible(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("a", "b", "c")}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Collection.Sampling = dbadapter.NewSamplingConfig(1)
	options.Collection.Sampling.Throttle = time.Hour

	ctx, cancel := context.WithCancel(t.Context())
	cancel()

	started := time.Now()

	result, err := Run(ctx, options)
	require.NoError(t, err, "a cancelled sampling pass still writes what it collected")

	assert.Less(t, time.Since(started), 5*time.Second, "the throttle did not sleep through the cancellation")
	assert.Empty(t, adapter.sampled, "no table was sampled after the cancellation")
	assert.Zero(t, result.Sampled)
}

// TestAnUnreadableTableIsSkippedRatherThanFatal covers the plan's requirement
// that a table the role cannot read reports Skipped with a reason.
func TestAnUnreadableTableIsSkippedRatherThanFatal(t *testing.T) {
	adapter := &fakeAdapter{
		schema:     newFakeSchema("readable", "forbidden"),
		unreadable: map[string]bool{"forbidden": true},
	}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Collection.Sampling = dbadapter.NewSamplingConfig(2)

	result, err := Run(t.Context(), options)
	require.NoError(t, err, "one unreadable table is not a reason to lose the schema")

	loaded, err := artifact.ReadSchema(result.Path, nil)
	require.NoError(t, err)
	require.Len(t, loaded.Samples, 2)

	byName := map[string]dbschema.TableSample{}
	for _, sample := range loaded.Samples {
		byName[sample.TableName] = sample
	}

	require.NotNil(t, byName["forbidden"].Status)
	assert.Equal(t, dbschema.SampleSkipped, byName["forbidden"].Status.State)
	require.NotNil(t, byName["forbidden"].Status.Reason)
	assert.Contains(t, *byName["forbidden"].Status.Reason, "forbidden")

	require.NotNil(t, byName["readable"].Status)
	assert.Equal(t, dbschema.SampleComplete, byName["readable"].Status.State)
}

// TestAnUnreadableTablesErrorNeverReachesTheDocument records why the skip reason
// is composed rather than wrapped: an adapter's error can carry a server address
// or a DSN, and a document carrying one is rejected by the credential scan.
func TestAnUnreadableTablesErrorNeverReachesTheDocument(t *testing.T) {
	adapter := &fakeAdapter{
		schema:     newFakeSchema("forbidden"),
		unreadable: map[string]bool{"forbidden": true},
	}

	options := newOptions(t, adapter)
	options.Collection.Sample = true

	result, err := Run(t.Context(), options)
	require.NoError(t, err)

	contents, err := os.ReadFile(result.Path)
	require.NoError(t, err)

	assert.NotContains(t, string(contents), "permission denied",
		"the adapter's error text is not copied into the artifact")
}

// TestRedactionIsAppliedBeforeTheArtifactIsWritten checks the mode reaches the
// samples rather than being carried and dropped.
func TestRedactionIsAppliedBeforeTheArtifactIsWritten(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("users")}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Collection.Sampling = dbadapter.NewSamplingConfig(1)
	options.Redaction = dbschema.RedactMinimal

	result, err := Run(t.Context(), options)
	require.NoError(t, err)

	loaded, err := artifact.ReadSchema(result.Path, nil)
	require.NoError(t, err)
	require.Len(t, loaded.Samples, 1)
	require.Len(t, loaded.Samples[0].Rows, 1)

	assert.Equal(t, dbschema.RedactedValue, loaded.Samples[0].Rows[0]["password"],
		"a column whose name denotes a secret is masked")
	assert.Equal(t, "a plain value", loaded.Samples[0].Rows[0]["note"],
		"an ordinary column is left alone at this mode")
}

// TestSamplingStopsAtTheConfiguredTableLimit covers the bound, and that it is
// applied in document order so two runs sample the same tables.
func TestSamplingStopsAtTheConfiguredTableLimit(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("a", "b", "c", "d")}

	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.MaxTablesSampled = 2

	result, err := Run(t.Context(), options)
	require.NoError(t, err)

	assert.Equal(t, 2, result.Sampled)
	assert.Equal(t, []dbadapter.TableRef{{Table: "a"}, {Table: "b"}}, adapter.sampled)
}

// TestAConnectionPasswordReachesNoOutput is the R17 claim at the orchestration
// level: the survey holds a parsed target whose password is a Secret, and never
// the string it was parsed from.
func TestAConnectionPasswordReachesNoOutput(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("users")}

	target, err := ParseTarget("postgres://surveyor:" + plaintext + "@db.internal:5432/shop")
	require.NoError(t, err)

	collection := dbadapter.NewCollectionConfig(target.Connection.Host)
	collection.Connection = target.Connection

	report := &recorder{}
	options := Options{
		Target:     target,
		Adapters:   Registry{SchemePostgres: constantConstructor(adapter)},
		Collection: collection,
		Output:     filepath.Join(t.TempDir(), "schema"),
		Redaction:  dbschema.RedactNone,
		Progress:   report,
	}

	result, err := Run(t.Context(), options)
	require.NoError(t, err)

	contents, err := os.ReadFile(result.Path)
	require.NoError(t, err)

	assert.NotContains(t, string(contents), plaintext, "not in the artifact")
	assert.NotContains(t, report.joined(), plaintext, "not in a progress line")
	assert.NotContains(t, fmt.Sprintf("%v", options), plaintext, "not in a formatted options value")
	assert.NotContains(t, fmt.Sprintf("%+v", result), plaintext, "not in the result")
}

// TestAFailedConnectionDoesNotEchoTheTarget keeps the same property on the error
// path, which is where a connection string most often escapes.
func TestAFailedConnectionDoesNotEchoTheTarget(t *testing.T) {
	target, err := ParseTarget("postgres://surveyor:" + plaintext + "@db.internal:5432/shop")
	require.NoError(t, err)

	collection := dbadapter.NewCollectionConfig(target.Connection.Host)
	collection.Connection = target.Connection

	_, err = Run(t.Context(), Options{
		Target:     target,
		Collection: collection,
		Output:     filepath.Join(t.TempDir(), "schema"),
		Adapters: Registry{SchemePostgres: func(
			context.Context, dbadapter.ConnectionConfig,
		) (dbadapter.Adapter, error) {
			return nil, errors.New("dial tcp: connection refused")
		}},
	})

	require.Error(t, err)
	assert.NotContains(t, err.Error(), plaintext)
	assert.Contains(t, err.Error(), "connection refused", "the operator still learns what failed")
}

func TestAFailedPingStopsTheSurvey(t *testing.T) {
	adapter := &fakeAdapter{
		schema:  newFakeSchema("users"),
		pingErr: errors.New("server closed the connection"),
	}

	_, err := Run(t.Context(), newOptions(t, adapter))
	require.ErrorContains(t, err, "server closed the connection")
	assert.True(t, adapter.closed, "the pool is released even when the ping fails")
}

// TestProgressIsReportedThroughEveryPhase checks the reporter is driven rather
// than merely accepted.
func TestProgressIsReportedThroughEveryPhase(t *testing.T) {
	adapter := &fakeAdapter{schema: newFakeSchema("users", "orders")}

	report := &recorder{}
	options := newOptions(t, adapter)
	options.Collection.Sample = true
	options.Progress = report

	_, err := Run(t.Context(), options)
	require.NoError(t, err)

	joined := report.joined()
	assert.Contains(t, joined, "connecting")
	assert.Contains(t, joined, "collecting the schema")
	assert.Contains(t, joined, "start 2 sampling")
	assert.Contains(t, joined, "step users")
	assert.Contains(t, joined, "writing the artifact")
}

// TestANilReporterIsNotAPanic covers the default: a caller that wants no output
// leaves the field alone.
func TestANilReporterIsNotAPanic(t *testing.T) {
	options := newOptions(t, &fakeAdapter{schema: newFakeSchema("users")})
	options.Progress = nil

	_, err := Run(t.Context(), options)
	require.NoError(t, err)
}

// TestTheDatabaseCredentialIsZeroedOnceCollectionIsDone pins the half of R17
// that no type can enforce on its own.
//
// Secret redacts itself everywhere it could be printed, but "zeroed best effort
// after connection setup" is something the orchestration has to actually do, and
// for a while nothing called Zero at all. NewSecret does not copy the caller's
// bytes, so the array handed in here is the array the credential lives in and
// the assertion can be made against it directly.
func TestTheDatabaseCredentialIsZeroedOnceCollectionIsDone(t *testing.T) {
	backing := []byte(plaintext)

	options := newOptions(t, &fakeAdapter{schema: newFakeSchema("users")})
	options.Target.Connection.Password = dbadapter.NewSecret(backing)
	options.Collection.Connection = options.Target.Connection

	_, err := Run(t.Context(), options)
	require.NoError(t, err)

	assert.Equal(t, make([]byte, len(backing)), backing,
		"the credential's backing array still holds it after the survey")
}

// TestTheDatabaseCredentialIsZeroedEvenWhenTheSurveyFails covers the path an
// operator hits more often than the happy one: a refused connection, a dropped
// session, an unreachable host.
func TestTheDatabaseCredentialIsZeroedEvenWhenTheSurveyFails(t *testing.T) {
	backing := []byte(plaintext)

	options := newOptions(t, &fakeAdapter{
		schema:  newFakeSchema("users"),
		pingErr: errors.New("server closed the connection"),
	})
	options.Target.Connection.Password = dbadapter.NewSecret(backing)
	options.Collection.Connection = options.Target.Connection

	_, err := Run(t.Context(), options)
	require.Error(t, err)

	assert.Equal(t, make([]byte, len(backing)), backing,
		"a failed survey must not leave the credential in memory")
}
