package dbadapter

import (
	"errors"
	"fmt"
	"log/slog"
	"runtime"
)

// Redacted is what a Secret renders as, everywhere, always.
const Redacted = "[REDACTED]"

// ErrSecretNotDecodable reports an attempt to read a Secret out of a document.
var ErrSecretNotDecodable = errors.New("a secret cannot be decoded from a document")

// Secret holds credential bytes that must not reach output.
//
// R17 requires that credentials never pass through fmt, logging, or error
// paths. Stating that as a rule makes it a discipline every future caller has
// to remember; stating it as a type makes it the default. Secret implements
// fmt.Formatter, fmt.Stringer, fmt.GoStringer, json.Marshaler, and
// slog.LogValuer, all returning Redacted, so the ordinary ways a value ends up
// in a log line or an error string produce the marker rather than the bytes.
//
// fmt.Formatter is the one that matters most. A Stringer alone covers %s and
// %v, but %x on a []byte field would print the credential in hex and %#v would
// print it as a byte slice literal. Formatter takes precedence over every other
// fmt interface and receives the verb, so a single implementation covers verbs
// nobody thought to test.
//
// The bytes are reachable only through Reveal, which is named to be greppable
// and is the sole exit. TestNoSecretIsConvertedToString in tools/ fails on any
// string conversion applied to its result.
//
// The zero Secret is valid and empty.
type Secret struct {
	// b is the credential. It is unexported so that no struct literal outside
	// this package can construct a Secret whose bytes are also held elsewhere.
	b []byte
}

// NewSecret takes ownership of b.
//
// The caller must not retain or mutate b afterwards: Secret does not copy it,
// so that Zero can wipe the one array the credential lives in rather than
// wiping a copy while the original stays in memory.
func NewSecret(b []byte) Secret {
	return Secret{b: b}
}

// Len reports the number of bytes held. It is safe to log: a length is not a
// credential, and "the password is empty" is a diagnosis an operator needs.
func (s Secret) Len() int {
	return len(s.b)
}

// IsZero reports whether the secret holds no bytes.
func (s Secret) IsZero() bool {
	return len(s.b) == 0
}

// Reveal returns the underlying bytes.
//
// This is the only way out, and every call site is a place a credential could
// escape. Pass the result to a driver and nowhere else; do not convert it to a
// string, store it, or include it in a message.
func (s Secret) Reveal() []byte {
	return s.b
}

// RevealString returns the underlying bytes as a string.
//
// It exists because every database driver this project uses takes its password
// as a string field, so the conversion has to happen somewhere. Making it a
// named method puts it in one greppable place rather than scattering
// string(secret.Reveal()) through the adapters, and gives the repository test
// in tools/ a single symbol to reserve.
//
// It is not a convenience, and it is not the method to reach for. A string
// cannot be zeroed -- the credential outlives every attempt to erase it -- and
// it is the form that flows effortlessly into an error message and a structured
// log field. TestNoSecretIsConvertedToString reserves this method, and the
// equivalent hand-rolled conversion, to the connect.go of an adapter package:
// the one file per engine where a driver is handed a credential.
func (s Secret) RevealString() string {
	return string(s.b)
}

// Format implements fmt.Formatter for every verb.
//
// The verb is deliberately ignored. There is no formatting of a credential that
// is correct, so %s, %q, %x, %d, and %#v all produce the same marker. %q and
// %#v add quotes so the output stays syntactically what the verb promised.
func (s Secret) Format(state fmt.State, verb rune) {
	var out string

	switch verb {
	case 'q', 'v':
		if verb == 'q' || state.Flag('#') {
			out = `"` + Redacted + `"`
		} else {
			out = Redacted
		}
	default:
		out = Redacted
	}

	if _, err := state.Write([]byte(out)); err != nil {
		// fmt discards write errors from Formatter implementations, and there
		// is no caller here to report one to. Ignoring it cannot leak the
		// secret, which is the property that matters.
		_ = err
	}
}

// String implements fmt.Stringer for callers that reach for it directly. fmt
// itself uses Format instead.
func (s Secret) String() string {
	return Redacted
}

// GoString implements fmt.GoStringer, so %#v in a context that bypasses Format
// still yields the marker.
func (s Secret) GoString() string {
	return `"` + Redacted + `"`
}

// MarshalJSON emits the marker.
//
// A Secret should not be in a document at all, and the credential scan in
// dbschema would reject one that were. Emitting the marker rather than an error
// means a config dump of a struct that happens to hold a Secret is safe to
// write, which is the case this is actually protecting.
func (s Secret) MarshalJSON() ([]byte, error) {
	return []byte(`"` + Redacted + `"`), nil
}

// UnmarshalJSON always fails.
//
// A document that contains a secret is either a redaction marker being read
// back as though it were the password, or a credential stored on disk. Both are
// wrong, and failing loudly is better than either. Credentials come from the
// environment or a prompt.
func (s *Secret) UnmarshalJSON([]byte) error {
	return ErrSecretNotDecodable
}

// LogValue implements slog.LogValuer, so a Secret passed to a structured logger
// as an attribute value renders as the marker.
func (s Secret) LogValue() slog.Value {
	return slog.StringValue(Redacted)
}

// Zero overwrites the credential in place.
//
// This is best effort and is documented as such wherever it is offered. Go's
// garbage collector may have copied the bytes while moving them, the runtime
// offers no way to find or clear those copies, and a credential that arrived as
// a string was already un-erasable before it got here. What Zero does
// guarantee is that this array no longer holds the credential.
//
// runtime.KeepAlive keeps the backing array reachable across the clear, so the
// compiler cannot conclude the write is dead and drop it.
func (s *Secret) Zero() {
	clear(s.b)
	runtime.KeepAlive(s.b)

	s.b = nil
}
