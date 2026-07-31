package artifact

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/envelope"
)

// ErrFormatMismatch reports an artifact whose contents do not match the framing
// its extension claims: a plain JSON file named ".zst", or a file named ".enc"
// that is not an envelope.
var ErrFormatMismatch = errors.New("artifact contents do not match its extension")

// ReadSchema loads a single-database artifact from path.
//
// password is consulted only when the artifact is encrypted, so loading a plain
// file never prompts.
func ReadSchema(path string, password PasswordFunc) (*dbschema.Schema, error) {
	schema := &dbschema.Schema{}
	if err := read(path, password, schema); err != nil {
		return nil, err
	}

	return schema, nil
}

// ReadServerSchema loads a multi-database artifact from path.
func ReadServerSchema(path string, password PasswordFunc) (*dbschema.ServerSchema, error) {
	schema := &dbschema.ServerSchema{}
	if err := read(path, password, schema); err != nil {
		return nil, err
	}

	return schema, nil
}

// read loads path, unframes it, and decodes it into document.
//
// Every failure names the path. An operator loading an artifact usually has
// several on hand, and an error that says only "malformed" does not say which
// file to look at.
func read(path string, password PasswordFunc, document Document) error {
	data, err := unframe(path, password)
	if err != nil {
		return fmt.Errorf("%s: %w", path, err)
	}

	if err := decode(data, document); err != nil {
		return fmt.Errorf("%s: %w", path, err)
	}

	return nil
}

// unframe reads path and undoes the framing its extension declares.
//
// Dispatch is on the final extension only. Within an envelope it is on the zstd
// frame magic instead: a compressed-and-encrypted artifact is named ".enc" and
// nothing in the name says a zstd frame is inside, so the bytes are asked
// rather than the filename. Notably the ".zst" path does not sniff -- a file
// named ".zst" that does not begin with a zstd frame is a mislabelled file, and
// silently reading it as JSON would teach an operator that the extension does
// not mean anything.
func unframe(path string, password PasswordFunc) ([]byte, error) {
	// The size is checked before the read rather than after it. Every other
	// ceiling in this package guards a length the file declares; this one guards
	// the length the file simply has, which os.ReadFile would otherwise allocate
	// in full before a single magic byte has been looked at. An artifact is
	// bounded by maxDecompressedSize once expanded, so on disk -- compressed,
	// encrypted, or neither -- it has no business exceeding that.
	info, err := os.Stat(path)
	if err != nil {
		return nil, fmt.Errorf("read artifact: %w", err)
	}

	if info.Size() > maxArtifactSize {
		return nil, fmt.Errorf("%w: %d bytes on disk exceeds the %d byte limit",
			ErrTooLarge, info.Size(), maxArtifactSize)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read artifact: %w", err)
	}

	switch strings.ToLower(filepath.Ext(path)) {
	case extEncrypted:
		opened, err := open(data, password)
		if err != nil {
			return nil, err
		}

		if !hasZstdMagic(opened) {
			return opened, nil
		}

		return decompress(opened)
	case extZstd:
		if !hasZstdMagic(data) {
			return nil, fmt.Errorf("%w: named %s but does not begin with a zstd frame", ErrFormatMismatch, extZstd)
		}

		return decompress(data)
	default:
		return data, nil
	}
}

// open decrypts an envelope, translating "this is not an envelope" into the
// mismatch error so a mislabelled file reads the same way whichever extension
// it was given.
func open(sealed []byte, password PasswordFunc) ([]byte, error) {
	secret, err := resolvePassword(password)
	if err != nil {
		return nil, err
	}

	defer envelope.Zero(secret)

	data, err := envelope.Open(sealed, secret)
	if err != nil {
		if errors.Is(err, envelope.ErrNotAnEnvelope) {
			return nil, fmt.Errorf("%w: named %s but is not an envelope", ErrFormatMismatch, extEncrypted)
		}

		return nil, fmt.Errorf("open artifact: %w", err)
	}

	return data, nil
}

// decode scans the JSON for credentials, then unmarshals it into document and
// validates it.
//
// The scan runs on the bytes rather than on the decoded document, and it runs
// first. Unmarshalling silently drops any field the Go type does not declare,
// so a scan that ran only after decoding would pronounce a file clean while the
// file on disk carried a credential in a field this version does not know
// about. What the operator holds is the bytes, so the bytes are what is
// scanned.
//
// Validate then runs its own scan over the decoded document. The overlap is
// deliberate: Validate is the enforcement point for documents that never
// touched disk, and this package does not get to assume it is the only caller.
func decode(data []byte, document Document) error {
	findings, err := dbschema.ScanJSONForCredentials(data)
	if err != nil {
		return err
	}

	if len(findings) > 0 {
		problems := make([]error, 0, len(findings))
		for _, finding := range findings {
			problems = append(problems, &dbschema.CredentialError{Finding: finding})
		}

		return errors.Join(problems...)
	}

	if err := json.Unmarshal(data, document); err != nil {
		return fmt.Errorf("decode document: %w", err)
	}

	return document.Validate()
}
