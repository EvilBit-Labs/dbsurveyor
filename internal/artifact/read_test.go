package artifact

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/envelope"
)

// writeRaw puts bytes on disk without going through Write, so a test can build
// the mislabelled and hand-edited artifacts the read path has to reject.
func writeRaw(t *testing.T, path string, data []byte) string {
	t.Helper()

	// The forbidigo rule points writers at this package, and this is a fixture
	// inside it. The path is built from t.TempDir() by the caller, so there is
	// no untrusted input for G703 to follow.
	//nolint:gosec,forbidigo // G703: the path is a test fixture, not operator input.
	err := os.WriteFile(path, data, 0o600)
	require.NoError(t, err)

	return path
}

// TestRoundTripThroughEveryFormat is the central contract: what Write put on
// disk is what Read gives back, whichever framing was used.
func TestRoundTripThroughEveryFormat(t *testing.T) {
	for name, format := range formats {
		t.Run(name, func(t *testing.T) {
			written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), format, staticPassword)
			require.NoError(t, err)

			loaded, err := ReadSchema(written, staticPassword)
			require.NoError(t, err)
			assert.Equal(t, testSchema(), loaded)
		})
	}
}

func TestRoundTripOfAServerDocument(t *testing.T) {
	for name, format := range formats {
		t.Run(name, func(t *testing.T) {
			written, err := Write(filepath.Join(t.TempDir(), "server"), testServerSchema(), format, staticPassword)
			require.NoError(t, err)

			loaded, err := ReadServerSchema(written, staticPassword)
			require.NoError(t, err)
			assert.Equal(t, testServerSchema(), loaded)
		})
	}
}

// TestCompressionInsideAnEnvelopeIsDetectedByItsMagic is the behavior GOTCHAS
// 3.5 records: a compressed-and-encrypted artifact is named ".enc" and nothing
// in the name says a zstd frame is inside, so the reader has to ask the bytes.
func TestCompressionInsideAnEnvelopeIsDetectedByItsMagic(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"),
		testSchema(), Format{Compress: true, Encrypt: true}, staticPassword)
	require.NoError(t, err)
	require.Equal(t, ".enc", filepath.Ext(written))

	sealed, err := os.ReadFile(written)
	require.NoError(t, err)

	opened, err := envelope.Open(sealed, []byte(testPassword))
	require.NoError(t, err)
	require.True(t, hasZstdMagic(opened), "the envelope holds a zstd frame, not JSON")

	loaded, err := ReadSchema(written, staticPassword)
	require.NoError(t, err)
	assert.Equal(t, testSchema(), loaded)
}

// TestAnUncompressedEnvelopeIsReadAsJSON is the other half of the sniff: the
// same ".enc" extension must also work when nothing was compressed.
func TestAnUncompressedEnvelopeIsReadAsJSON(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, staticPassword)
	require.NoError(t, err)

	loaded, err := ReadSchema(written, staticPassword)
	require.NoError(t, err)
	assert.Equal(t, testSchema(), loaded)
}

// TestAMislabelledArtifactFailsClearly covers the rename accident: a plain JSON
// file given a framing extension it does not have.
func TestAMislabelledArtifactFailsClearly(t *testing.T) {
	plain, err := json.Marshal(testSchema())
	require.NoError(t, err)

	for name, suffix := range map[string]string{
		"json named .zst": ".json.zst",
		"json named .enc": ".enc",
	} {
		t.Run(name, func(t *testing.T) {
			path := writeRaw(t, filepath.Join(t.TempDir(), "schema"+suffix), plain)

			loaded, err := ReadSchema(path, staticPassword)
			require.ErrorIs(t, err, ErrFormatMismatch)
			assert.Contains(t, err.Error(), path, "the error names the file")
			assert.Nil(t, loaded)
		})
	}
}

// TestAZstdArtifactIsNotSniffed records the deliberate asymmetry: the ".zst"
// path trusts its extension and rejects a mismatch, rather than falling back to
// reading the file as JSON. Silently accepting it would teach an operator that
// the extension carries no meaning.
func TestAZstdArtifactIsNotSniffed(t *testing.T) {
	plain, err := json.Marshal(testSchema())
	require.NoError(t, err)

	path := writeRaw(t, filepath.Join(t.TempDir(), "schema.json.zst"), plain)

	_, err = ReadSchema(path, nil)
	require.ErrorIs(t, err, ErrFormatMismatch)
}

// TestACredentialIsCaughtOnEveryReadPath checks R16 independently per framing,
// because each path reaches the scan by a different route and a missing call on
// any one of them would be invisible from the others.
func TestACredentialIsCaughtOnEveryReadPath(t *testing.T) {
	schema := testSchema()
	schema.AddWarning("could not connect to postgres://admin:hunter2@db.internal/app")

	plain, err := json.Marshal(schema)
	require.NoError(t, err)

	compressed, err := compress(plain)
	require.NoError(t, err)

	sealed, err := envelope.Seal(plain, []byte(testPassword))
	require.NoError(t, err)

	sealedCompressed, err := envelope.Seal(compressed, []byte(testPassword))
	require.NoError(t, err)

	for name, planted := range map[string]struct {
		suffix string
		data   []byte
	}{
		"plain":                 {".json", plain},
		"compressed":            {".json.zst", compressed},
		"encrypted":             {".enc", sealed},
		"compressed, encrypted": {".enc", sealedCompressed},
	} {
		t.Run(name, func(t *testing.T) {
			path := writeRaw(t, filepath.Join(t.TempDir(), "schema"+planted.suffix), planted.data)

			loaded, err := ReadSchema(path, staticPassword)
			require.Error(t, err)
			assert.Contains(t, err.Error(), "credential pattern")
			assert.NotContains(t, err.Error(), "hunter2", "the error must not reproduce the credential")
			assert.Nil(t, loaded)
		})
	}
}

// TestACredentialInAnUndeclaredFieldIsStillCaught is why the scan runs on the
// bytes rather than on the decoded document. Unmarshalling drops a field the Go
// type does not declare, so a scan that ran after decoding would call this file
// clean while the credential sat in it.
func TestACredentialInAnUndeclaredFieldIsStillCaught(t *testing.T) {
	document := map[string]any{}

	plain, err := json.Marshal(testSchema())
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(plain, &document))

	document["collector_notes"] = "retried with postgres://admin:hunter2@db.internal/app"

	planted, err := json.Marshal(document)
	require.NoError(t, err)

	path := writeRaw(t, filepath.Join(t.TempDir(), "schema.json"), planted)

	_, err = ReadSchema(path, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "collector_notes")
	assert.NotContains(t, err.Error(), "hunter2")
}

func TestReadRejectsAnInvalidDocument(t *testing.T) {
	schema := testSchema()
	schema.Format = "somebody-elses/schema"

	planted, err := json.Marshal(schema)
	require.NoError(t, err)

	path := writeRaw(t, filepath.Join(t.TempDir(), "schema.json"), planted)

	_, err = ReadSchema(path, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "somebody-elses/schema")
}

func TestReadRejectsMalformedJSON(t *testing.T) {
	path := writeRaw(t, filepath.Join(t.TempDir(), "schema.json"), []byte("{not json"))

	_, err := ReadSchema(path, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), path)
}

func TestReadReportsAMissingFile(t *testing.T) {
	_, err := ReadSchema(filepath.Join(t.TempDir(), "absent.json"), nil)
	require.ErrorIs(t, err, os.ErrNotExist)
}

func TestReadingAnEncryptedArtifactWithTheWrongPasswordFails(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, staticPassword)
	require.NoError(t, err)

	_, err = ReadSchema(written, func() ([]byte, error) { return []byte("not-the-password"), nil })
	require.ErrorIs(t, err, envelope.ErrAuthentication)
}

func TestReadingAnEncryptedArtifactWithoutAPasswordSourceIsAnError(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, staticPassword)
	require.NoError(t, err)

	_, err = ReadSchema(written, nil)
	require.ErrorIs(t, err, ErrPasswordRequired)
}

// TestAnEncryptedArtifactRejectsATamperedByte confirms the envelope's
// authentication survives the artifact layer rather than being bypassed by it.
func TestAnEncryptedArtifactRejectsATamperedByte(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, staticPassword)
	require.NoError(t, err)

	sealed, err := os.ReadFile(written)
	require.NoError(t, err)

	sealed[len(sealed)-1] ^= 0x01
	writeRaw(t, written, sealed)

	_, err = ReadSchema(written, staticPassword)
	require.ErrorIs(t, err, envelope.ErrAuthentication)
}

// TestDecompressionRefusesABomb checks the ceiling on the one path where
// nothing is authenticated: a plain ".zst" artifact carries no tag, so the
// limit is all that stands between a small hostile file and an out-of-memory
// crash.
func TestDecompressionRefusesABomb(t *testing.T) {
	bomb, err := compress(make([]byte, maxDecompressedSize+1))
	require.NoError(t, err)
	require.Less(t, len(bomb), 1<<20, "the point is that a small file expands past the limit")

	path := writeRaw(t, filepath.Join(t.TempDir(), "schema.json.zst"), bomb)

	_, err = ReadSchema(path, nil)
	require.ErrorIs(t, err, ErrTooLarge)
}

func TestCompressRoundTrip(t *testing.T) {
	payload := []byte(`{"format":"dbsurveyor/schema"}`)

	framed, err := compress(payload)
	require.NoError(t, err)
	require.True(t, hasZstdMagic(framed))

	restored, err := decompress(framed)
	require.NoError(t, err)
	assert.Equal(t, payload, restored)
}

func TestDecompressRejectsBytesThatAreNotAFrame(t *testing.T) {
	_, err := decompress([]byte("plain text"))
	require.Error(t, err)
}

// TestAnOversizedArtifactIsRefusedBeforeItIsRead pins the ceiling on the raw
// read.
//
// Every other bound in this package guards a length the file declares. This one
// guards the length the file simply has: os.ReadFile would allocate the whole
// thing before a single magic byte had been looked at.
func TestAnOversizedArtifactIsRefusedBeforeItIsRead(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "huge.json")

	file, err := os.Create(path) //nolint:forbidigo // G-none: staging an oversized fixture, not writing an artifact.
	require.NoError(t, err)

	// Truncate rather than write: a sparse file reports the size without
	// costing the disk a gigabyte.
	require.NoError(t, file.Truncate(maxArtifactSize+1))
	require.NoError(t, file.Close())

	_, err = unframe(path, nil)
	require.ErrorIs(t, err, ErrTooLarge)
}
