package dbadapter

import (
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTableRefRenders(t *testing.T) {
	assert.Equal(t, "public.orders", TableRef{Schema: "public", Table: "orders"}.String())
	assert.Equal(t, "orders", TableRef{Table: "orders"}.String())
	assert.Empty(t, TableRef{}.String())
}

func TestTableRefRequiresATableName(t *testing.T) {
	require.NoError(t, TableRef{Table: "orders"}.Validate())
	require.ErrorIs(t, TableRef{Schema: "public"}.Validate(), ErrEmptyTableName)
}

// TestTheConstructorClampsAndValidationRejects is the distinction GOTCHAS 2.1
// records. The constructor clamps because a caller asking for more rows than
// the maximum wants as many as possible. Validation rejects because a struct
// literal and a JSON document both skip the constructor, so its clamping says
// nothing about a config that arrived by either route.
func TestTheConstructorClampsAndValidationRejects(t *testing.T) {
	for name, size := range map[string]struct {
		requested uint32
		clamped   uint32
	}{
		"below the minimum": {0, MinSampleSize},
		"above the maximum": {MaxSampleSize + 1, MaxSampleSize},
		"far above":         {^uint32(0), MaxSampleSize},
		"in range":          {500, 500},
	} {
		t.Run(name, func(t *testing.T) {
			built := NewSamplingConfig(size.requested)
			assert.Equal(t, size.clamped, built.SampleSize)
			require.NoError(t, built.Validate(), "the constructor never produces an invalid config")

			literal := SamplingConfig{SampleSize: size.requested, QueryTimeout: DefaultQueryTimeout}
			if size.requested != size.clamped {
				require.ErrorIs(t, literal.Validate(), ErrInvalidSampleSize,
					"a struct literal bypasses the clamp, so validation has to catch it")
			} else {
				require.NoError(t, literal.Validate())
			}
		})
	}
}

// TestAnOutOfRangeSampleSizeFromJSONFails is the same bypass by the other
// route. Deserialization does not call the constructor either.
func TestAnOutOfRangeSampleSizeFromJSONFails(t *testing.T) {
	for name, document := range map[string]string{
		"zero":              `{"sample_size":0,"query_timeout":30000000000}`,
		"above the maximum": `{"sample_size":100000,"query_timeout":30000000000}`,
	} {
		t.Run(name, func(t *testing.T) {
			var cfg SamplingConfig
			require.NoError(t, json.Unmarshal([]byte(document), &cfg))
			require.ErrorIs(t, cfg.Validate(), ErrInvalidSampleSize)
		})
	}
}

func TestSamplingValidationRejectsBadTimeouts(t *testing.T) {
	cfg := DefaultSamplingConfig()
	cfg.QueryTimeout = 0
	require.ErrorIs(t, cfg.Validate(), ErrInvalidTimeout)

	cfg = DefaultSamplingConfig()
	cfg.QueryTimeout = MaxQueryTimeout + time.Second
	require.ErrorIs(t, cfg.Validate(), ErrInvalidTimeout)

	cfg = DefaultSamplingConfig()
	cfg.Throttle = -time.Second
	require.ErrorIs(t, cfg.Validate(), ErrInvalidTimeout)
}

// TestValidationReportsEveryProblemAtOnce keeps a caller from fixing one
// mistake at a time.
func TestValidationReportsEveryProblemAtOnce(t *testing.T) {
	err := SamplingConfig{SampleSize: 0, QueryTimeout: 0}.Validate()
	require.Error(t, err)
	require.ErrorIs(t, err, ErrInvalidSampleSize)
	require.ErrorIs(t, err, ErrInvalidTimeout)
}

// TestAMatcherIsBuiltFromAConfigRatherThanCachedInIt is the fix for GOTCHAS
// 2.2. The previous implementation cached compiled patterns in the config, and
// deserialization skipped the cache, so a config read from a file silently
// matched nothing. Here the compiled form is a separate value with no way to
// exist half-built.
func TestAMatcherIsBuiltFromAConfigRatherThanCachedInIt(t *testing.T) {
	cfg := DefaultSamplingConfig()
	cfg.SensitivePatterns = []SensitivePattern{
		{Pattern: `\d{3}-\d{2}-\d{4}`, Description: "looks like a national identifier"},
		{Pattern: `(?i)\bbearer\s+[a-z0-9._-]+`},
	}

	encoded, err := json.Marshal(cfg)
	require.NoError(t, err)

	var decoded SamplingConfig
	require.NoError(t, json.Unmarshal(encoded, &decoded))

	matcher, err := decoded.Matcher()
	require.NoError(t, err)
	require.Equal(t, 2, matcher.Len(), "a decoded config compiles the same patterns as a built one")

	description, matched := matcher.Match("ssn 123-45-6789 on file")
	assert.True(t, matched)
	assert.Equal(t, "looks like a national identifier", description)

	description, matched = matcher.Match("Authorization: Bearer abc.def")
	assert.True(t, matched)
	assert.Equal(t, `(?i)\bbearer\s+[a-z0-9._-]+`, description,
		"a pattern with no description falls back to itself so a warning is never empty")

	_, matched = matcher.Match("nothing interesting")
	assert.False(t, matched)
}

func TestAnUncompilablePatternFailsValidation(t *testing.T) {
	cfg := DefaultSamplingConfig()
	cfg.SensitivePatterns = []SensitivePattern{{Pattern: `([unclosed`}}

	_, err := cfg.Matcher()
	require.ErrorIs(t, err, ErrInvalidPattern)
	require.ErrorIs(t, cfg.Validate(), ErrInvalidPattern)
}

func TestAnEmptyMatcherMatchesNothing(t *testing.T) {
	matcher, err := DefaultSamplingConfig().Matcher()
	require.NoError(t, err)
	assert.Equal(t, 0, matcher.Len())

	_, matched := matcher.Match("anything at all")
	assert.False(t, matched)
}

func TestConnectionDefaultsValidate(t *testing.T) {
	cfg := NewConnectionConfig("db.internal")
	require.NoError(t, cfg.Validate())
	assert.True(t, cfg.ReadOnly, "read-only is the default, not an opt-in")
}

func TestConnectionValidationRejectsUnusableSettings(t *testing.T) {
	for name, mutate := range map[string]struct {
		change func(*ConnectionConfig)
		want   error
	}{
		"no host":            {func(c *ConnectionConfig) { c.Host = "" }, ErrMissingHost},
		"no connect timeout": {func(c *ConnectionConfig) { c.ConnectTimeout = 0 }, ErrInvalidTimeout},
		"no query timeout":   {func(c *ConnectionConfig) { c.QueryTimeout = 0 }, ErrInvalidTimeout},
		"query timeout too long": {
			func(c *ConnectionConfig) { c.QueryTimeout = MaxQueryTimeout + time.Second },
			ErrInvalidTimeout,
		},
		"no connections": {func(c *ConnectionConfig) { c.MaxConnections = 0 }, ErrInvalidPoolSize},
		"idle above max": {
			func(c *ConnectionConfig) { c.MinIdleConnections = c.MaxConnections + 1 },
			ErrInvalidPoolSize,
		},
		"writable session": {func(c *ConnectionConfig) { c.ReadOnly = false }, ErrNotReadOnly},
	} {
		t.Run(name, func(t *testing.T) {
			cfg := NewConnectionConfig("db.internal")
			mutate.change(&cfg)
			require.ErrorIs(t, cfg.Validate(), mutate.want)
		})
	}
}

// TestAWritableSessionIsRefused states the policy in the type system's terms:
// read-only is not a setting an operator turns on, it is the only accepted
// value, and a config that asks for anything else does not validate.
func TestAWritableSessionIsRefused(t *testing.T) {
	cfg := NewConnectionConfig("db.internal")
	cfg.ReadOnly = false

	require.ErrorIs(t, cfg.Validate(), ErrNotReadOnly)
}

func TestCollectionDefaultsValidate(t *testing.T) {
	cfg := NewCollectionConfig("db.internal")
	require.NoError(t, cfg.Validate())

	assert.False(t, cfg.Sample, "sampling is the only part that reads user data, so it is opt-in")
	assert.False(t, cfg.IncludeSystemDatabases)
	assert.True(t, cfg.IncludeViews)
	assert.True(t, cfg.IncludeIndexes)
}

// TestSamplingIsOnlyValidatedWhenEnabled keeps an unused sampling section from
// blocking a schema-only survey.
func TestSamplingIsOnlyValidatedWhenEnabled(t *testing.T) {
	cfg := NewCollectionConfig("db.internal")
	cfg.Sampling = SamplingConfig{}

	require.NoError(t, cfg.Validate(), "sampling is off, so its settings do not matter")

	cfg.Sample = true
	require.ErrorIs(t, cfg.Validate(), ErrInvalidSampleSize)
}

func TestCollectionValidationSurfacesConnectionProblems(t *testing.T) {
	cfg := NewCollectionConfig("")
	require.ErrorIs(t, cfg.Validate(), ErrMissingHost)

	cfg = NewCollectionConfig("db.internal")
	cfg.MaxConcurrentQueries = 0
	require.ErrorIs(t, cfg.Validate(), ErrInvalidPoolSize)
}
