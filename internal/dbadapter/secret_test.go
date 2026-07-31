package dbadapter

import (
	"bytes"
	"encoding/json"
	"fmt"
	"log/slog"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// plaintext is the value no test output may ever contain.
const plaintext = "hunter2-the-actual-password"

func newTestSecret() Secret {
	return NewSecret([]byte(plaintext))
}

// TestEveryFormatVerbRedacts is the guarantee R17 rests on. %s and %v would be
// covered by a Stringer alone; %x, %d, and %#v are the ones that print a byte
// slice in the clear when only String is implemented.
func TestEveryFormatVerbRedacts(t *testing.T) {
	secret := newTestSecret()

	for _, format := range []string{"%s", "%v", "%q", "%#v", "%x", "%X", "%d", "%+v", "%8s"} {
		t.Run(format, func(t *testing.T) {
			rendered := fmt.Sprintf(format, secret)
			assert.Contains(t, rendered, Redacted)
			assert.NotContains(t, rendered, plaintext)
			assert.NotContains(t, rendered, "hunter2")
		})
	}
}

// TestQuotedVerbsStayQuoted keeps the marker syntactically what the verb
// promised, so a %q in a log format does not produce unbalanced output.
func TestQuotedVerbsStayQuoted(t *testing.T) {
	secret := newTestSecret()

	// The verbs are held in a variable rather than written inline because
	// perfsprint rewrites a literal Sprintf("%v", x) into x.String(), and
	// autofix applies that rewrite whether or not a suppression is present.
	// The rewritten test would assert that String returns the marker -- which
	// TestStringAndGoStringRedact already covers -- and quietly drop the claim
	// that matters here: that the fmt path never reaches the bytes.
	for verb, want := range map[string]string{
		"%s":  Redacted,
		"%v":  Redacted,
		"%q":  `"` + Redacted + `"`,
		"%#v": `"` + Redacted + `"`,
	} {
		t.Run(verb, func(t *testing.T) {
			assert.Equal(t, want, fmt.Sprintf(verb, secret))
		})
	}
}

// TestASecretNestedInAStructRedacts is the realistic case: nobody formats a
// Secret directly, they format the config that holds one.
func TestASecretNestedInAStructRedacts(t *testing.T) {
	cfg := NewConnectionConfig("db.internal")
	cfg.Username = "surveyor"
	cfg.Password = newTestSecret()

	for _, format := range []string{"%v", "%+v", "%#v", "%s"} {
		rendered := fmt.Sprintf(format, cfg)
		assert.NotContains(t, rendered, plaintext, "format %s leaked the password", format)
	}
}

func TestStringAndGoStringRedact(t *testing.T) {
	secret := newTestSecret()

	assert.Equal(t, Redacted, secret.String())
	assert.Equal(t, `"`+Redacted+`"`, secret.GoString())
}

func TestMarshallingASecretEmitsTheMarker(t *testing.T) {
	encoded, err := json.Marshal(newTestSecret())
	require.NoError(t, err)
	assert.JSONEq(t, `"`+Redacted+`"`, string(encoded))
}

// TestMarshallingAStructContainingASecretEmitsTheMarker covers the case a
// config dump actually takes.
func TestMarshallingAStructContainingASecretEmitsTheMarker(t *testing.T) {
	encoded, err := json.Marshal(struct {
		User     string `json:"user"`
		Password Secret `json:"password"`
	}{User: "surveyor", Password: newTestSecret()})
	require.NoError(t, err)

	assert.Contains(t, string(encoded), Redacted)
	assert.NotContains(t, string(encoded), plaintext)
}

// TestASecretCannotBeDecoded records the deliberate asymmetry: a Secret
// marshals so that dumping a config is safe, and refuses to unmarshal so that a
// credential on disk, or a redaction marker read back as a password, fails
// loudly instead of being used.
func TestASecretCannotBeDecoded(t *testing.T) {
	var target struct {
		Password Secret `json:"password"`
	}

	err := json.Unmarshal([]byte(`{"password":"`+plaintext+`"}`), &target)
	require.ErrorIs(t, err, ErrSecretNotDecodable)
	assert.True(t, target.Password.IsZero())
}

// TestALogRecordRedacts covers the slog path, which reaches for LogValue rather
// than for fmt.
func TestALogRecordRedacts(t *testing.T) {
	var buffer bytes.Buffer

	logger := slog.New(slog.NewJSONHandler(&buffer, nil))
	logger.Info("connecting", "password", newTestSecret(), "user", "surveyor")

	assert.Contains(t, buffer.String(), Redacted)
	assert.NotContains(t, buffer.String(), plaintext)
}

// TestALogRecordWithANestedSecretRedacts checks the attribute-group path, where
// the Secret is inside a value rather than being the value.
func TestALogRecordWithANestedSecretRedacts(t *testing.T) {
	var buffer bytes.Buffer

	logger := slog.New(slog.NewTextHandler(&buffer, nil))
	logger.Info("connecting", "config", NewConnectionConfig("db.internal"), "password", newTestSecret())

	assert.NotContains(t, buffer.String(), plaintext)
}

func TestAnErrorWrappingASecretRedacts(t *testing.T) {
	err := fmt.Errorf("connect as %s with %v: refused", "surveyor", newTestSecret())

	assert.Contains(t, err.Error(), Redacted)
	assert.NotContains(t, err.Error(), plaintext)
}

// TestRevealReturnsTheBytes confirms the one exit works, since a Secret nothing
// can read out of is not useful to a driver.
func TestRevealReturnsTheBytes(t *testing.T) {
	assert.Equal(t, []byte(plaintext), newTestSecret().Reveal())
}

// TestZeroErasesTheBackingArray checks that Zero wipes the array the caller
// handed over, not a copy of it.
func TestZeroErasesTheBackingArray(t *testing.T) {
	backing := []byte(plaintext)
	secret := NewSecret(backing)

	secret.Zero()

	assert.Equal(t, make([]byte, len(plaintext)), backing, "the caller's array was wiped in place")
	assert.True(t, secret.IsZero())
	assert.Nil(t, secret.Reveal())
}

func TestZeroIsSafeOnAZeroValue(t *testing.T) {
	var secret Secret

	assert.NotPanics(t, secret.Zero)
}

// TestLenAndIsZeroAreSafeToReport records that a length is not a credential.
// "The password is empty" is a diagnosis an operator needs, and refusing to
// report it would push callers toward looking at the bytes.
func TestLenAndIsZeroAreSafeToReport(t *testing.T) {
	assert.Equal(t, len(plaintext), newTestSecret().Len())
	assert.False(t, newTestSecret().IsZero())

	assert.Equal(t, 0, Secret{}.Len())
	assert.True(t, Secret{}.IsZero())
	assert.True(t, NewSecret(nil).IsZero())
	assert.True(t, NewSecret([]byte{}).IsZero())
}

// TestTheZeroSecretIsUsable means a config built by struct literal without a
// password behaves rather than panicking.
func TestTheZeroSecretIsUsable(t *testing.T) {
	var secret Secret

	verb := "%v" // Held in a variable so autofix cannot collapse it to String.
	assert.Equal(t, Redacted, fmt.Sprintf(verb, secret))

	encoded, err := json.Marshal(secret)
	require.NoError(t, err)
	assert.JSONEq(t, `"`+Redacted+`"`, string(encoded))
}
