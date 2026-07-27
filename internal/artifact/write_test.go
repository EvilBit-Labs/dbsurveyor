package artifact

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// testPassword is long enough to clear envelope.MinPasswordLength.
const testPassword = "artifact-password"

// staticPassword is the PasswordFunc used wherever the password itself is not
// what a test is about.
func staticPassword() ([]byte, error) {
	return []byte(testPassword), nil
}

// formats enumerates the four write modes by the extension each produces, so a
// test can drive all of them without repeating the truth table.
var formats = map[string]Format{
	".json":     {},
	".json.zst": {Compress: true},
	".enc":      {Encrypt: true},
	".enc both": {Compress: true, Encrypt: true},
}

// testSchema returns a valid single-database document.
func testSchema() *dbschema.Schema {
	return dbschema.New(
		dbschema.NewDatabaseInfo("inventory", dbschema.PostgreSQL),
		"0.1.0",
		time.Date(2026, time.July, 26, 12, 0, 0, 0, time.UTC),
	)
}

// testServerSchema returns a valid multi-database document.
func testServerSchema() *dbschema.ServerSchema {
	server := dbschema.NewServerSchema(dbschema.ServerInfo{
		ServerType:     dbschema.PostgreSQL,
		Version:        "16.2",
		Host:           "db.internal",
		TotalDatabases: 1,
		ConnectionUser: "surveyor",
		CollectionMode: dbschema.SingleDatabase(),
	}, "0.1.0", time.Date(2026, time.July, 26, 12, 0, 0, 0, time.UTC))

	server.Databases = []dbschema.Schema{*testSchema()}

	return server
}

func TestExtensionFollowsTheFormat(t *testing.T) {
	assert.Equal(t, ".json", Format{}.Extension())
	assert.Equal(t, ".zst", Format{Compress: true}.Extension())
	assert.Equal(t, ".enc", Format{Encrypt: true}.Extension())
	assert.Equal(t, ".enc", Format{Compress: true, Encrypt: true}.Extension())
}

func TestNormalizePathAppendsTheFormatExtension(t *testing.T) {
	for name, expect := range map[string]struct {
		path   string
		format Format
		want   string
	}{
		"bare name, plain":       {"schema", Format{}, "schema.json"},
		"bare name, compressed":  {"schema", Format{Compress: true}, "schema.json.zst"},
		"bare name, encrypted":   {"schema", Format{Encrypt: true}, "schema.enc"},
		"bare name, both":        {"schema", Format{Compress: true, Encrypt: true}, "schema.enc"},
		"json name, compressed":  {"schema.json", Format{Compress: true}, "schema.json.zst"},
		"json name, encrypted":   {"schema.json", Format{Encrypt: true}, "schema.json.enc"},
		"directory in the path":  {filepath.Join("out", "schema"), Format{}, filepath.Join("out", "schema.json")},
		"dots in the base name":  {"schema.v2", Format{}, "schema.v2.json"},
		"already .json":          {"schema.json", Format{}, "schema.json"},
		"already .json.zst":      {"schema.json.zst", Format{Compress: true}, "schema.json.zst"},
		"already .enc":           {"schema.enc", Format{Encrypt: true}, "schema.enc"},
		"already .enc from both": {"schema.enc", Format{Compress: true, Encrypt: true}, "schema.enc"},
		"uppercase extension":    {"SCHEMA.ENC", Format{Encrypt: true}, "SCHEMA.ENC"},
	} {
		t.Run(name, func(t *testing.T) {
			assert.Equal(t, expect.want, NormalizePath(expect.path, expect.format))
		})
	}
}

// TestWriteReturnsThePathItCreated is the contract a caller depends on to
// report where the artifact went: the returned path is the file on disk, not
// the path that was requested.
func TestWriteReturnsThePathItCreated(t *testing.T) {
	for name, format := range formats {
		t.Run(name, func(t *testing.T) {
			dir := t.TempDir()
			requested := filepath.Join(dir, "schema")

			written, err := Write(requested, testSchema(), format, staticPassword)
			require.NoError(t, err)
			assert.Equal(t, NormalizePath(requested, format), written)
			assert.FileExists(t, written)

			if format != (Format{}) {
				assert.NotEqual(t, requested, written, "the requested path gained an extension")
			}
		})
	}
}

// TestNoTemporaryFileSurvivesASuccessfulWrite checks the tidiness half of the
// atomic-write contract: the destination directory holds the artifact and
// nothing else.
func TestNoTemporaryFileSurvivesASuccessfulWrite(t *testing.T) {
	dir := t.TempDir()

	written, err := Write(filepath.Join(dir, "schema"), testSchema(), Format{Compress: true}, nil)
	require.NoError(t, err)

	entries, err := os.ReadDir(dir)
	require.NoError(t, err)
	require.Len(t, entries, 1)
	assert.Equal(t, filepath.Base(written), entries[0].Name())
}

// TestAFailedWriteLeavesThePreviousArtifactIntact is the reason writes go
// through a temporary file at all. The failure is forced by making the
// destination a directory, which the rename cannot replace.
func TestAFailedWriteLeavesThePreviousArtifactIntact(t *testing.T) {
	dir := t.TempDir()
	destination := filepath.Join(dir, "schema.json")
	require.NoError(t, os.Mkdir(destination, 0o750))

	_, err := Write(destination, testSchema(), Format{}, nil)
	require.Error(t, err)

	info, statErr := os.Stat(destination)
	require.NoError(t, statErr, "the destination still exists")
	assert.True(t, info.IsDir(), "the destination was not replaced by a partial write")

	entries, err := os.ReadDir(dir)
	require.NoError(t, err)
	assert.Len(t, entries, 1, "the temporary file was cleaned up")
}

// TestWriteRefusesADocumentCarryingACredential is the point of routing writes
// through this package: the scan runs before any byte reaches disk.
func TestWriteRefusesADocumentCarryingACredential(t *testing.T) {
	dir := t.TempDir()
	schema := testSchema()
	schema.AddWarning("could not connect to postgres://admin:hunter2@db.internal/app")

	written, err := Write(filepath.Join(dir, "schema"), schema, Format{}, nil)
	require.Error(t, err)
	assert.Empty(t, written)
	assert.Contains(t, err.Error(), "credential pattern")
	assert.NotContains(t, err.Error(), "hunter2", "the error must not reproduce the credential")

	entries, err := os.ReadDir(dir)
	require.NoError(t, err)
	assert.Empty(t, entries, "nothing was written")
}

func TestWriteRefusesAnInvalidDocument(t *testing.T) {
	dir := t.TempDir()
	schema := testSchema()
	schema.Format = "somebody-elses/schema"

	_, err := Write(filepath.Join(dir, "schema"), schema, Format{}, nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "somebody-elses/schema")
}

// TestEncryptingWithoutAPasswordSourceIsAnError keeps the failure at the point
// of configuration rather than producing an artifact sealed under nothing.
func TestEncryptingWithoutAPasswordSourceIsAnError(t *testing.T) {
	_, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, nil)
	require.ErrorIs(t, err, ErrPasswordRequired)
}

func TestAPasswordFailurePropagates(t *testing.T) {
	_, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true},
		func() ([]byte, error) { return nil, os.ErrPermission })
	require.ErrorIs(t, err, os.ErrPermission)
}

// TestCompressedOutputIsSmallerThanPlain is a sanity check that the compression
// step is wired in rather than a no-op copy.
func TestCompressedOutputIsSmallerThanPlain(t *testing.T) {
	dir := t.TempDir()
	schema := testSchema()

	for i := range 200 {
		schema.AddWarning("skipped table audit.events: permission denied, attempt " + string(rune('a'+i%26)))
	}

	plain, err := Write(filepath.Join(dir, "plain"), schema, Format{}, nil)
	require.NoError(t, err)

	compressed, err := Write(filepath.Join(dir, "compressed"), schema, Format{Compress: true}, nil)
	require.NoError(t, err)

	plainInfo, err := os.Stat(plain)
	require.NoError(t, err)

	compressedInfo, err := os.Stat(compressed)
	require.NoError(t, err)

	assert.Less(t, compressedInfo.Size(), plainInfo.Size())
}

// TestAnEncryptedArtifactIsOpaque checks that the payload is not sitting in the
// file in the clear: neither the format discriminator nor the database name
// appears in the bytes.
func TestAnEncryptedArtifactIsOpaque(t *testing.T) {
	written, err := Write(filepath.Join(t.TempDir(), "schema"), testSchema(), Format{Encrypt: true}, staticPassword)
	require.NoError(t, err)

	sealed, err := os.ReadFile(written)
	require.NoError(t, err)

	assert.NotContains(t, string(sealed), dbschema.Format)
	assert.NotContains(t, string(sealed), "inventory")
}
