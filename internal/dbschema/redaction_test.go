package dbschema

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func redactionFixture() []TableSample {
	return []TableSample{{
		TableName:  "users",
		SchemaName: ptr("public"),
		Rows: []map[string]any{{
			"id":            42,
			"user_id":       7,
			"username":      "ada",
			"password_hash": "$2b$12$abcdefghijklmnop",
			"apiKey":        "ak_live_123456",
			"email":         "ada@example.com",
			"ssn":           "123-45-6789",
			"description":   "operator notes about the row",
			"monkey_count":  "eleven",
			"created_at":    "2026-01-01T00:00:00Z",
			"birth_date":    "1815-12-10",
		}},
		SampleSize:       1,
		SamplingStrategy: MostRecent(1),
		CollectedAt:      testTime,
		Warnings:         []string{},
	}}
}

func redactedRow(t *testing.T, mode RedactionMode) map[string]any {
	t.Helper()

	out := NewRedactor(mode).Redact(redactionFixture())
	require.Len(t, out, 1)
	require.Len(t, out[0].Rows, 1)

	return out[0].Rows[0]
}

func TestRedactNoneLeavesEveryValueIntact(t *testing.T) {
	assert.Equal(t, redactionFixture()[0].Rows[0], redactedRow(t, RedactNone))
}

func TestRedactMinimalMasksSecretsAndNothingElse(t *testing.T) {
	row := redactedRow(t, RedactMinimal)

	assert.Equal(t, RedactedValue, row["password_hash"])
	assert.Equal(t, RedactedValue, row["apiKey"])
	assert.Equal(t, "ada", row["username"])
	assert.Equal(t, "ada@example.com", row["email"])
}

func TestRedactBalancedAddsPersonalData(t *testing.T) {
	row := redactedRow(t, RedactBalanced)

	assert.Equal(t, RedactedValue, row["password_hash"])
	assert.Equal(t, RedactedValue, row["email"])
	assert.Equal(t, RedactedValue, row["ssn"])
	assert.Equal(t, RedactedValue, row["birth_date"])
	assert.Equal(t, "ada", row["username"])
	assert.Equal(t, 42, row["id"], "a non-string value is never replaced by a string marker")
}

func TestRedactConservativeKeepsIdentifiersAndTimestamps(t *testing.T) {
	row := redactedRow(t, RedactConservative)

	assert.Equal(t, RedactedValue, row["description"])
	assert.Equal(t, RedactedValue, row["username"])
	assert.Equal(t, "2026-01-01T00:00:00Z", row["created_at"], "a value under an _at key stays visible")
	assert.Equal(t, "1815-12-10", row["birth_date"], "a date-shaped value stays orderable")
	assert.Equal(t, 42, row["id"])
}

// The Rust implementation kept any value containing a hyphen or a colon, so an
// email address survived the mode meant to give the most privacy.
func TestRedactConservativeMasksHyphenatedNonTimestamps(t *testing.T) {
	samples := []TableSample{{
		TableName: "people",
		Rows: []map[string]any{{
			"full_name": "ada-lovelace",
			"contact":   "ada@example.com",
			"note":      "12:34 pm meeting",
		}},
		SamplingStrategy: MostRecent(1),
		CollectedAt:      testTime,
	}}

	row := NewRedactor(RedactConservative).Redact(samples)[0].Rows[0]
	assert.Equal(t, RedactedValue, row["full_name"])
	assert.Equal(t, RedactedValue, row["contact"])
	assert.Equal(t, RedactedValue, row["note"])
}

func TestRedactionIsIdempotent(t *testing.T) {
	for _, mode := range []RedactionMode{RedactNone, RedactMinimal, RedactBalanced, RedactConservative} {
		t.Run(string(mode), func(t *testing.T) {
			redactor := NewRedactor(mode)
			once := redactor.Redact(redactionFixture())
			twice := redactor.Redact(once)

			assert.Equal(t, once, twice)
		})
	}
}

func TestRedactionDoesNotMutateItsInput(t *testing.T) {
	samples := redactionFixture()
	NewRedactor(RedactConservative).Redact(samples)

	assert.Equal(t, redactionFixture(), samples)
}

// Each mode redacts a superset of the mode before it.
func TestRedactionIsProgressive(t *testing.T) {
	counts := make([]int, 0, 4)
	for _, mode := range []RedactionMode{RedactNone, RedactMinimal, RedactBalanced, RedactConservative} {
		counts = append(counts, countRedacted(redactedRow(t, mode)))
	}

	for i := 1; i < len(counts); i++ {
		assert.GreaterOrEqual(t, counts[i], counts[i-1], "mode %d redacted fewer values than mode %d", i, i-1)
	}
}

func TestRedactionReachesNestedStructures(t *testing.T) {
	samples := []TableSample{{
		TableName: "events",
		Rows: []map[string]any{{
			"id": 1,
			"payload": map[string]any{
				"actor":  "ada",
				"secret": "s3cret",
				"tags":   []any{"one", "two"},
			},
			"tokens": []any{"tok_a", "tok_b"},
		}},
		SamplingStrategy: MostRecent(1),
		CollectedAt:      testTime,
	}}

	row := NewRedactor(RedactMinimal).Redact(samples)[0].Rows[0]
	payload, ok := row["payload"].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, RedactedValue, payload["secret"])
	assert.Equal(t, "ada", payload["actor"])

	// An array under a secret-named key is redacted element by element, since
	// a list of tokens is still a list of tokens.
	assert.Equal(t, []any{RedactedValue, RedactedValue}, row["tokens"])
	assert.Equal(t, []any{"one", "two"}, payload["tags"])
}

// An unrecognized mode fails closed rather than emitting raw values.
func TestNewRedactorFallsBackToConservative(t *testing.T) {
	assert.Equal(t, RedactConservative, NewRedactor("bogus").Mode())
}

func TestKeyTokensSplitsEveryNamingStyle(t *testing.T) {
	cases := map[string][]string{
		"password":      {"password"},
		"password_hash": {"password", "hash"},
		"apiKey":        {"api", "key"},
		"API_KEY":       {"api", "key"},
		"api-key":       {"api", "key"},
		"monkey_count":  {"monkey", "count"},
		"created_at":    {"created", "at"},
		"":              nil,
	}

	for key, want := range cases {
		t.Run(key, func(t *testing.T) {
			assert.Equal(t, want, keyTokens(key))
		})
	}
}

// "key" as a token must not match inside "monkey", which substring matching
// would have done.
func TestSecretPatternsMatchTokensNotSubstrings(t *testing.T) {
	row := redactedRow(t, RedactMinimal)
	assert.Equal(t, "eleven", row["monkey_count"])
}

func countRedacted(row map[string]any) int {
	count := 0

	for _, value := range row {
		if text, ok := value.(string); ok && text == RedactedValue {
			count++
		}
	}

	return count
}
