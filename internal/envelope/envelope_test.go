package envelope

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const testPassword = "example-password"

var testPlaintext = []byte(`{"format":"dbsurveyor/schema"}`)

// countingReader yields bytes counting up from zero. It stands in for
// crypto/rand so the worked example in docs/formats/encrypted-envelope.md can
// be reproduced exactly. It is never used outside tests.
type countingReader struct{ next byte }

func (c *countingReader) Read(p []byte) (int, error) {
	for i := range p {
		p[i] = c.next
		c.next++
	}

	return len(p), nil
}

var _ io.Reader = (*countingReader)(nil)

func sealedFixture(t *testing.T) []byte {
	t.Helper()

	sealed, err := Seal(testPlaintext, []byte(testPassword))
	require.NoError(t, err)

	return sealed
}

// tamperByte returns a copy of sealed with one byte replaced.
func tamperByte(t *testing.T, sealed []byte, offset int, value byte) []byte {
	t.Helper()

	altered := bytes.Clone(sealed)
	altered[offset] = value

	return altered
}

// tamperUint32 returns a copy of sealed with one big-endian uint32 replaced.
func tamperUint32(t *testing.T, sealed []byte, offset int, value uint32) []byte {
	t.Helper()

	altered := bytes.Clone(sealed)
	binary.BigEndian.PutUint32(altered[offset:offset+4], value)

	return altered
}

func TestSealThenOpenReturnsTheOriginalPlaintext(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(sealed, []byte(testPassword))
	require.NoError(t, err)
	assert.Equal(t, testPlaintext, opened)
}

func TestSealedEnvelopeHasTheSpecifiedShape(t *testing.T) {
	sealed := sealedFixture(t)

	require.Len(t, sealed, headerLength+len(testPlaintext)+tagLength)
	assert.Equal(t, magic, string(sealed[:len(magic)]))
	assert.Equal(t, FormatVersion, sealed[offVersion])
	assert.Equal(t, kdfArgon2id, sealed[offKDFID])
	assert.Equal(t, cipherAES256GCM, sealed[offCipherID])
	assert.Equal(t, DefaultParallelism, sealed[offParallelism])
	assert.Equal(t, DefaultMemoryKiB, binary.BigEndian.Uint32(sealed[offMemoryKiB:offMemoryKiB+4]))
	assert.Equal(t, DefaultTime, binary.BigEndian.Uint32(sealed[offTime:offTime+4]))
	assert.Equal(t, byte(SaltLength), sealed[offSaltLength])
	assert.Equal(t, byte(NonceLength), sealed[offNonceLength])

	// The ciphertext is the same length as the plaintext, so an envelope
	// reveals the payload size. The specification says so rather than implying
	// otherwise by omission.
	assert.Len(t, sealed[headerLength:len(sealed)-tagLength], len(testPlaintext))
}

func TestAnEmptyPayloadRoundTrips(t *testing.T) {
	sealed, err := Seal(nil, []byte(testPassword))
	require.NoError(t, err)
	require.Len(t, sealed, headerLength+tagLength)

	opened, err := Open(sealed, []byte(testPassword))
	require.NoError(t, err)
	assert.Empty(t, opened)
}

func TestOpenWithTheWrongPasswordFailsAndReturnsNoPlaintext(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(sealed, []byte("not-the-password"))
	require.ErrorIs(t, err, ErrAuthentication)
	assert.Nil(t, opened)
}

func TestFlippingACiphertextBitFailsTheTagCheck(t *testing.T) {
	sealed := sealedFixture(t)
	altered := bytes.Clone(sealed)
	altered[headerLength] ^= 0x01

	opened, err := Open(altered, []byte(testPassword))
	require.ErrorIs(t, err, ErrAuthentication)
	assert.Nil(t, opened)
}

func TestFlippingATagBitFailsTheTagCheck(t *testing.T) {
	sealed := sealedFixture(t)
	altered := bytes.Clone(sealed)
	altered[len(altered)-1] ^= 0x01

	opened, err := Open(altered, []byte(testPassword))
	require.ErrorIs(t, err, ErrAuthentication)
	assert.Nil(t, opened)
}

func TestAlteringARecordedCostParameterFailsToOpen(t *testing.T) {
	sealed := sealedFixture(t)

	// One KiB above what was written: still inside the floor and the ceiling,
	// so the header passes validation and the failure has to come from the
	// cryptography rather than from a bounds check.
	altered := tamperUint32(t, sealed, offMemoryKiB, DefaultMemoryKiB+1)

	opened, err := Open(altered, []byte(testPassword))
	require.ErrorIs(t, err, ErrAuthentication)
	assert.Nil(t, opened)
}

func TestAlteringTheNonceFailsToOpen(t *testing.T) {
	sealed := sealedFixture(t)
	altered := bytes.Clone(sealed)
	altered[offNonce] ^= 0x01

	opened, err := Open(altered, []byte(testPassword))
	require.ErrorIs(t, err, ErrAuthentication)
	assert.Nil(t, opened)
}

// TestTheHeaderIsAuthenticatedAsAdditionalData proves the binding directly.
//
// Tampering with a header field cannot prove it: every field either feeds key
// derivation, feeds GCM, or is rejected by a bounds check, so an altered
// envelope would fail even if the header were not authenticated at all. What
// the AAD adds is that the header a reader parsed is the header the writer
// sealed under -- shown here by opening the same body under the correct header
// and under an altered one.
func TestTheHeaderIsAuthenticatedAsAdditionalData(t *testing.T) {
	sealed := sealedFixture(t)

	head, err := decodeHeader(sealed)
	require.NoError(t, err)

	key := head.params.derive([]byte(testPassword))
	defer Zero(key)

	aead, err := newAEAD(key)
	require.NoError(t, err)

	body := sealed[headerLength:]

	opened, err := aead.Open(nil, head.nonce, body, sealed[:headerLength])
	require.NoError(t, err)
	require.Equal(t, testPlaintext, opened)

	// Same key, same nonce, same body -- only the additional data differs.
	otherHeader := tamperByte(t, sealed[:headerLength], offVersion, FormatVersion+1)

	_, err = aead.Open(nil, head.nonce, body, otherHeader)
	require.Error(t, err)
}

func TestTwoSealsOfTheSamePlaintextDiffer(t *testing.T) {
	first := sealedFixture(t)
	second := sealedFixture(t)

	assert.NotEqual(t, first[offSalt:offSalt+SaltLength], second[offSalt:offSalt+SaltLength],
		"each envelope must carry its own salt")
	assert.NotEqual(t, first[offNonce:offNonce+NonceLength], second[offNonce:offNonce+NonceLength],
		"a repeated nonce under one key is what breaks GCM")
	assert.NotEqual(t, first[headerLength:], second[headerLength:],
		"identical plaintext must not produce identical ciphertext")
}

func TestOpenRejectsDataThatIsNotAnEnvelope(t *testing.T) {
	cases := map[string][]byte{
		"empty":       nil,
		"too short":   make([]byte, headerLength-1),
		"wrong magic": make([]byte, headerLength+tagLength),
	}

	for name, data := range cases {
		t.Run(name, func(t *testing.T) {
			opened, err := Open(data, []byte(testPassword))
			require.ErrorIs(t, err, ErrNotAnEnvelope)
			assert.Nil(t, opened)
		})
	}
}

func TestOpenRejectsAnUnsupportedVersion(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(tamperByte(t, sealed, offVersion, FormatVersion+1), []byte(testPassword))
	require.ErrorIs(t, err, ErrUnsupportedVersion)
	assert.Nil(t, opened)
}

func TestOpenRejectsAnUnsupportedAlgorithm(t *testing.T) {
	sealed := sealedFixture(t)

	for name, offset := range map[string]int{"kdf": offKDFID, "cipher": offCipherID} {
		t.Run(name, func(t *testing.T) {
			opened, err := Open(tamperByte(t, sealed, offset, 0xFF), []byte(testPassword))
			require.ErrorIs(t, err, ErrUnsupportedAlgorithm)
			assert.Nil(t, opened)
		})
	}
}

func TestOpenRejectsADeclaredNonceLengthOtherThanTwelve(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(tamperByte(t, sealed, offNonceLength, NonceLength+1), []byte(testPassword))
	require.ErrorIs(t, err, ErrMalformedHeader)
	assert.Nil(t, opened)
}

func TestOpenRejectsADeclaredSaltLengthOtherThanSixteen(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(tamperByte(t, sealed, offSaltLength, SaltLength-1), []byte(testPassword))
	require.ErrorIs(t, err, ErrMalformedHeader)
	assert.Nil(t, opened)
}

func TestOpenRejectsCostParametersBelowTheFloor(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(tamperUint32(t, sealed, offMemoryKiB, DefaultMemoryKiB-1), []byte(testPassword))
	require.ErrorIs(t, err, ErrWeakKDFParameters)
	assert.Nil(t, opened)
}

// TestOpenRejectsCostParametersAboveTheCeiling is the allocation-bomb guard.
// The memory cost named here is four terabytes; that this test returns at all,
// rather than exhausting the machine, is the property being checked.
func TestOpenRejectsCostParametersAboveTheCeiling(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(tamperUint32(t, sealed, offMemoryKiB, ^uint32(0)), []byte(testPassword))
	require.ErrorIs(t, err, ErrExcessiveKDFParameters)
	assert.Nil(t, opened)
}

func TestOpenRejectsABodyShorterThanTheTag(t *testing.T) {
	sealed := sealedFixture(t)

	opened, err := Open(sealed[:headerLength+tagLength-1], []byte(testPassword))
	require.ErrorIs(t, err, ErrTruncated)
	assert.Nil(t, opened)
}

func TestSealAndOpenRefuseAShortPassword(t *testing.T) {
	short := []byte("short")

	sealed, err := Seal(testPlaintext, short)
	require.ErrorIs(t, err, ErrPasswordTooShort)
	assert.Nil(t, sealed)

	opened, err := Open(sealedFixture(t), short)
	require.ErrorIs(t, err, ErrPasswordTooShort)
	assert.Nil(t, opened)
}

// TestWorkedExampleMatchesTheSpecification is what keeps
// docs/formats/encrypted-envelope.md honest. The hex below is the same byte
// sequence the specification records; changing the layout without changing the
// document fails here.
func TestWorkedExampleMatchesTheSpecification(t *testing.T) {
	const expected = "44425356454e4331010101040001000000000003100c" +
		"000102030405060708090a0b0c0d0e0f" +
		"101112131415161718191a1b" +
		"b5404a04fed77a2e2f84a24f377847a4d9f98f848a2b6bfe76cf9f6f08e6" +
		"032b9c081bd8f9cc10aea109f14a7b6b"

	const expectedKey = "2f66cd3aefa45a0cf591eb28b6d1dd3bfc0113116b95f5d8ad3ab1a0d4aa55ec"

	sealed, err := seal(&countingReader{}, testPlaintext, []byte(testPassword))
	require.NoError(t, err)
	assert.Equal(t, expected, hex.EncodeToString(sealed))

	// The specification publishes the derived key so an implementer can tell a
	// mis-wired Argon2id apart from a mis-wired GCM.
	salt := make([]byte, SaltLength)
	_, err = io.ReadFull(&countingReader{}, salt)
	require.NoError(t, err)

	key := DefaultKDFParams(salt).derive([]byte(testPassword))
	defer Zero(key)

	assert.Equal(t, expectedKey, hex.EncodeToString(key))

	opened, err := Open(sealed, []byte(testPassword))
	require.NoError(t, err)
	assert.Equal(t, testPlaintext, opened)
}

func TestZeroErasesTheSlice(t *testing.T) {
	secret := []byte("a derived key")
	Zero(secret)
	assert.Equal(t, make([]byte, len("a derived key")), secret)
}
