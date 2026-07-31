// Package envelope implements the dbsurveyor encrypted envelope: a payload
// sealed with AES-256-GCM under a key derived from a password by Argon2id.
//
// The byte layout is specified in docs/formats/encrypted-envelope.md, and
// TestWorkedExampleMatchesTheSpecification holds the two together: the
// specification records a complete envelope in hex, and the test reproduces it
// from injected salt and nonce bytes. A change to the layout that is not also
// a change to the specification is a test failure.
//
// The envelope is opaque. It records what a reader needs in order to derive the
// same key -- the algorithm identifiers, the Argon2id cost parameters, the
// salt, and the nonce -- and nothing about what it wraps. The payload's format,
// its size before encryption, and the file it came from are not recoverable
// from the header.
package envelope

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
)

// magic identifies a dbsurveyor envelope. The trailing digit is not the format
// version -- FormatVersion is -- it distinguishes this magic from any later
// format that is not a version of this one.
const magic = "DBSVENC1"

// FormatVersion is the envelope layout version this package reads and writes.
const FormatVersion byte = 1

// Algorithm identifiers. They are single bytes rather than names because a
// length-prefixed string in the header would be one more thing to parse before
// anything has been authenticated.
const (
	kdfArgon2id     byte = 1
	cipherAES256GCM byte = 1
)

// NonceLength is the AES-GCM nonce size in bytes. NIST SP 800-38D section 8.2.1
// specifies 96 bits, which is the only size GCM uses without an extra
// derivation step.
const NonceLength = 12

// tagLength is the AES-GCM authentication tag size in bytes: 128 bits, the
// full tag, giving a 2^-128 forgery probability.
const tagLength = 16

// Header field offsets. The header is fixed width, so a reader can bounds-check
// once and then index without arithmetic.
const (
	offVersion     = 8
	offKDFID       = 9
	offCipherID    = 10
	offParallelism = 11
	offMemoryKiB   = 12
	offTime        = 16
	offSaltLength  = 20
	offNonceLength = 21
	offSalt        = 22
	offNonce       = offSalt + SaltLength
	headerLength   = offNonce + NonceLength
)

// ErrNotAnEnvelope reports data that does not begin with the envelope magic.
var ErrNotAnEnvelope = errors.New("not a dbsurveyor encrypted envelope")

// ErrUnsupportedVersion reports an envelope written by a newer format version.
var ErrUnsupportedVersion = errors.New("unsupported envelope format version")

// ErrUnsupportedAlgorithm reports an envelope naming a cipher or KDF this
// package does not implement.
var ErrUnsupportedAlgorithm = errors.New("unsupported envelope algorithm")

// ErrMalformedHeader reports a header whose declared field sizes do not match
// the format.
var ErrMalformedHeader = errors.New("malformed envelope header")

// ErrTruncated reports an envelope whose header parsed but whose body is too
// short to hold an authentication tag.
var ErrTruncated = errors.New("envelope is truncated")

// ErrAuthentication reports a failed GCM tag check.
//
// It is deliberately one error rather than two. A wrong password and an altered
// file are the same event to the cipher, and reporting them apart would tell
// whoever altered the file whether their password guess was right.
var ErrAuthentication = errors.New("envelope authentication failed: wrong password or altered file")

// Seal encrypts plaintext under a key derived from password and returns a
// complete envelope: header, ciphertext, tag.
//
// The salt and nonce come from crypto/rand, so sealing the same plaintext twice
// under the same password produces two unrelated envelopes. Nonce reuse under a
// single key is what breaks GCM, and a fresh salt per envelope means there is
// no single key to reuse a nonce under.
func Seal(plaintext, password []byte) ([]byte, error) {
	return seal(rand.Reader, plaintext, password)
}

// Open decrypts an envelope with a key derived from password.
//
// It returns nil plaintext on any failure. The whole header is passed to GCM as
// additional authenticated data, so the header a reader parses is bound to the
// body it sealed: an altered header fails the tag check exactly as an altered
// ciphertext does, and a downgrade is not available to anyone who cannot already
// produce a valid tag.
func Open(sealed, password []byte) ([]byte, error) {
	head, err := decodeHeader(sealed)
	if err != nil {
		return nil, err
	}

	if err := ValidatePassword(password); err != nil {
		return nil, err
	}

	body := sealed[headerLength:]
	if len(body) < tagLength {
		return nil, fmt.Errorf("%w: body is %d bytes, shorter than the %d-byte tag",
			ErrTruncated, len(body), tagLength)
	}

	key := head.params.derive(password)
	defer Zero(key)

	aead, err := newAEAD(key)
	if err != nil {
		return nil, err
	}

	plaintext, err := aead.Open(nil, head.nonce, body, sealed[:headerLength])
	if err != nil {
		// The underlying error is discarded rather than wrapped: it carries no
		// detail a caller may act on, and ErrAuthentication is the whole of
		// what a caller is told.
		return nil, ErrAuthentication
	}

	return plaintext, nil
}

// Zero overwrites b with zeroes.
//
// This is best effort. Go's garbage collector may have copied the bytes while
// moving them, and the runtime offers no way to find or clear those copies, so
// no guarantee of erasure is claimed here or anywhere else in this tree.
func Zero(b []byte) {
	clear(b)
}

// header is the parsed fixed-width prefix of an envelope.
type header struct {
	params KDFParams
	nonce  []byte
}

// decodeHeader parses and checks the header of an envelope.
//
// Every field is checked before the caller derives a key, because derivation is
// the expensive step and the header is the only thing available to reject a
// hostile file cheaply.
func decodeHeader(sealed []byte) (header, error) {
	if len(sealed) < headerLength {
		return header{}, fmt.Errorf("%w: %d bytes is shorter than the %d-byte header",
			ErrNotAnEnvelope, len(sealed), headerLength)
	}

	if string(sealed[:len(magic)]) != magic {
		return header{}, ErrNotAnEnvelope
	}

	if version := sealed[offVersion]; version != FormatVersion {
		return header{}, fmt.Errorf("%w: %d, this build reads %d", ErrUnsupportedVersion, version, FormatVersion)
	}

	if id := sealed[offKDFID]; id != kdfArgon2id {
		return header{}, fmt.Errorf("%w: key derivation id %d", ErrUnsupportedAlgorithm, id)
	}

	if id := sealed[offCipherID]; id != cipherAES256GCM {
		return header{}, fmt.Errorf("%w: cipher id %d", ErrUnsupportedAlgorithm, id)
	}

	if length := sealed[offSaltLength]; length != SaltLength {
		return header{}, fmt.Errorf("%w: declared salt length %d, expected %d",
			ErrMalformedHeader, length, SaltLength)
	}

	if length := sealed[offNonceLength]; length != NonceLength {
		return header{}, fmt.Errorf("%w: declared nonce length %d, expected %d",
			ErrMalformedHeader, length, NonceLength)
	}

	params := KDFParams{
		MemoryKiB:   binary.BigEndian.Uint32(sealed[offMemoryKiB : offMemoryKiB+4]),
		Time:        binary.BigEndian.Uint32(sealed[offTime : offTime+4]),
		Parallelism: sealed[offParallelism],
		Salt:        sealed[offSalt : offSalt+SaltLength],
	}
	if err := params.Validate(); err != nil {
		return header{}, err
	}

	return header{params: params, nonce: sealed[offNonce : offNonce+NonceLength]}, nil
}

// encode writes the header into dst, which must be headerLength bytes.
func (h header) encode(dst []byte) {
	copy(dst, magic)
	dst[offVersion] = FormatVersion
	dst[offKDFID] = kdfArgon2id
	dst[offCipherID] = cipherAES256GCM
	dst[offParallelism] = h.params.Parallelism
	binary.BigEndian.PutUint32(dst[offMemoryKiB:offMemoryKiB+4], h.params.MemoryKiB)
	binary.BigEndian.PutUint32(dst[offTime:offTime+4], h.params.Time)
	dst[offSaltLength] = SaltLength
	dst[offNonceLength] = NonceLength
	copy(dst[offSalt:offNonce], h.params.Salt)
	copy(dst[offNonce:headerLength], h.nonce)
}

// seal is Seal with the randomness source injected, so a test can reproduce the
// worked example in the specification byte for byte.
func seal(random io.Reader, plaintext, password []byte) ([]byte, error) {
	if err := ValidatePassword(password); err != nil {
		return nil, err
	}

	// Salt first, then nonce, from one stream: the specification records that
	// order so the worked example can be reproduced from a known byte sequence.
	salt := make([]byte, SaltLength)
	if _, err := io.ReadFull(random, salt); err != nil {
		return nil, fmt.Errorf("read envelope salt: %w", err)
	}

	nonce := make([]byte, NonceLength)
	if _, err := io.ReadFull(random, nonce); err != nil {
		return nil, fmt.Errorf("read envelope nonce: %w", err)
	}

	head := header{params: DefaultKDFParams(salt), nonce: nonce}

	key := head.params.derive(password)
	defer Zero(key)

	aead, err := newAEAD(key)
	if err != nil {
		return nil, err
	}

	encoded := make([]byte, headerLength)
	head.encode(encoded)

	sealed := make([]byte, 0, headerLength+len(plaintext)+tagLength)
	sealed = append(sealed, encoded...)

	return aead.Seal(sealed, nonce, plaintext, encoded), nil
}

// newAEAD builds the AES-256-GCM cipher for a derived key.
func newAEAD(key []byte) (cipher.AEAD, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, fmt.Errorf("construct aes cipher: %w", err)
	}

	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, fmt.Errorf("construct gcm mode: %w", err)
	}

	return aead, nil
}
