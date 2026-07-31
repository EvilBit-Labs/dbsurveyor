package dbschema

import (
	"regexp"
	"slices"
	"strings"
	"unicode"
)

// RedactedValue replaces a redacted string. It is a fixed marker rather than a
// length-preserving mask so that a redacted value cannot be distinguished from
// another by its shape.
const RedactedValue = "[REDACTED]"

// RedactionMode selects how aggressively sampled values are masked.
type RedactionMode string

// The redaction modes, in increasing order of aggressiveness. Each mode redacts
// a superset of what the mode before it redacts.
const (
	// RedactNone emits sampled values unchanged.
	RedactNone RedactionMode = "none"
	// RedactMinimal masks columns whose names denote a secret.
	RedactMinimal RedactionMode = "minimal"
	// RedactBalanced adds columns whose names denote personal data.
	RedactBalanced RedactionMode = "balanced"
	// RedactConservative masks every string except identifiers, timestamps, and
	// values whose shape is a date or time.
	RedactConservative RedactionMode = "conservative"
)

var redactionModes = map[RedactionMode]struct{}{
	RedactNone:         {},
	RedactMinimal:      {},
	RedactBalanced:     {},
	RedactConservative: {},
}

// Valid reports whether m is a recognized redaction mode.
func (m RedactionMode) Valid() bool {
	_, ok := redactionModes[m]

	return ok
}

// UnmarshalJSON rejects unrecognized redaction modes.
func (m *RedactionMode) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "redaction mode", m)
}

// Redactor masks sampled values without mutating the samples it is given.
//
// Redaction is idempotent: a value already replaced by RedactedValue is left
// alone, so redacting an already-redacted sample changes nothing regardless of
// mode.
type Redactor struct {
	mode RedactionMode
}

// NewRedactor returns a redactor for the given mode. An unrecognized mode is
// treated as RedactConservative, so a configuration mistake fails closed.
func NewRedactor(mode RedactionMode) *Redactor {
	if !mode.Valid() {
		mode = RedactConservative
	}

	return &Redactor{mode: mode}
}

// Mode reports the mode the redactor applies.
func (r *Redactor) Mode() RedactionMode {
	return r.mode
}

// Redact returns copies of the samples with their row values masked according to
// the redactor's mode. The input samples are not modified.
func (r *Redactor) Redact(samples []TableSample) []TableSample {
	if samples == nil {
		return nil
	}

	out := make([]TableSample, len(samples))

	for i, sample := range samples {
		out[i] = sample
		out[i].Rows = r.redactRows(sample.Rows)
	}

	return out
}

func (r *Redactor) redactRows(rows []map[string]any) []map[string]any {
	if rows == nil {
		return nil
	}

	out := make([]map[string]any, len(rows))

	for i, row := range rows {
		redacted := make(map[string]any, len(row))
		for key, value := range row {
			redacted[key] = r.redactValue(value, key)
		}

		out[i] = redacted
	}

	return out
}

// redactValue rebuilds value with its strings masked as the mode requires. key
// is the name of the column or object field the value was found under; nested
// arrays inherit the key of the field that holds them, since an array of values
// under "password" is still a password.
func (r *Redactor) redactValue(value any, key string) any {
	switch typed := value.(type) {
	case string:
		if r.shouldRedact(key, typed) {
			return RedactedValue
		}

		return typed
	case map[string]any:
		nested := make(map[string]any, len(typed))
		for nestedKey, nestedValue := range typed {
			nested[nestedKey] = r.redactValue(nestedValue, nestedKey)
		}

		return nested
	case []any:
		items := make([]any, len(typed))
		for i, item := range typed {
			items[i] = r.redactValue(item, key)
		}

		return items
	default:
		return value
	}
}

func (r *Redactor) shouldRedact(key, value string) bool {
	// An already-redacted value is left alone. This is what makes redaction
	// idempotent by construction rather than by coincidence of the patterns.
	if value == RedactedValue {
		return false
	}

	tokens := keyTokens(key)

	switch r.mode {
	case RedactNone:
		return false
	case RedactMinimal:
		return matchesAny(tokens, secretKeyPatterns)
	case RedactBalanced:
		return matchesAny(tokens, secretKeyPatterns) || matchesAny(tokens, personalKeyPatterns)
	case RedactConservative:
		return !isIdentifierKey(tokens) && !timestampLike.MatchString(value)
	default:
		return true
	}
}

// secretKeyPatterns name columns that hold a secret. Each pattern is a token
// sequence matched against the tokens of a column name, so "password" matches
// "password_hash" and "apiKey" without "key" matching "monkey".
var secretKeyPatterns = tokenizePatterns(
	"password", "passwd", "pwd", "passphrase",
	"secret", "token", "credential",
	"api_key", "access_key", "private_key", "secret_key", "session_key",
	"signature", "salt", "nonce",
)

// personalKeyPatterns name columns that hold personal data.
var personalKeyPatterns = tokenizePatterns(
	"email", "mail", "ssn", "social_security", "national_id", "tax_id",
	"phone", "mobile", "address", "dob", "birth", "birthdate", "birthday",
	"credit_card", "card_number", "cvv", "iban", "passport", "license",
)

// identifierKeyNames are the whole column names conservative mode leaves
// visible. Anything else is masked.
var identifierKeyNames = []string{"id", "uuid", "guid", "timestamp", "date", "time", "version"}

// timestampLike matches a date, a time, or an ISO-8601 datetime.
//
// The Rust implementation kept any value containing a hyphen or a colon, which
// preserved hyphenated names and email addresses in the mode meant to give the
// most privacy. Matching the whole value against a date or time shape closes
// that.
var timestampLike = regexp.MustCompile(
	`^(\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:?\d{2})?)?|\d{2}:\d{2}(:\d{2})?)$`,
)

// isIdentifierKey reports whether a column name denotes an identifier or a
// timestamp, which conservative mode leaves visible so that redacted output is
// still joinable and orderable.
func isIdentifierKey(tokens []string) bool {
	if len(tokens) == 0 {
		return false
	}

	if len(tokens) == 1 && slices.Contains(identifierKeyNames, tokens[0]) {
		return true
	}

	last := tokens[len(tokens)-1]

	return last == "id" || last == "at" || last == "uuid" || last == "guid"
}

// tokenizePatterns splits each pattern into its tokens once, at package
// initialization.
func tokenizePatterns(patterns ...string) [][]string {
	out := make([][]string, len(patterns))
	for i, pattern := range patterns {
		out[i] = keyTokens(pattern)
	}

	return out
}

// matchesAny reports whether any pattern appears as a run of consecutive tokens
// within the key's tokens.
func matchesAny(tokens []string, patterns [][]string) bool {
	for _, pattern := range patterns {
		if containsRun(tokens, pattern) {
			return true
		}
	}

	return false
}

func containsRun(tokens, pattern []string) bool {
	if len(pattern) == 0 || len(pattern) > len(tokens) {
		return false
	}

	for start := 0; start+len(pattern) <= len(tokens); start++ {
		if runMatches(tokens[start:start+len(pattern)], pattern) {
			return true
		}
	}

	return false
}

func runMatches(tokens, pattern []string) bool {
	for i, want := range pattern {
		if !tokenMatches(tokens[i], want) {
			return false
		}
	}

	return true
}

// tokenMatches compares one token against one pattern token, accepting a plain
// plural. Column names are written both ways -- "token" and "tokens", "api_key"
// and "api_keys" -- and a mode that masked only the singular would be a trap.
func tokenMatches(token, pattern string) bool {
	return token == pattern || token == pattern+"s"
}

// keyTokens splits a column name into lowercase word tokens, breaking on
// non-alphanumeric characters and on lower-to-upper transitions so that
// snake_case, kebab-case, and camelCase all tokenize the same way.
func keyTokens(key string) []string {
	var (
		tokens  []string
		current strings.Builder
	)

	flush := func() {
		if current.Len() > 0 {
			tokens = append(tokens, current.String())
			current.Reset()
		}
	}

	runes := []rune(key)
	for i, r := range runes {
		switch {
		case !unicode.IsLetter(r) && !unicode.IsDigit(r):
			flush()
		case unicode.IsUpper(r) && i > 0 && unicode.IsLower(runes[i-1]):
			flush()

			current.WriteRune(unicode.ToLower(r))
		default:
			current.WriteRune(unicode.ToLower(r))
		}
	}

	flush()

	return tokens
}
