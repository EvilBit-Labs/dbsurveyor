// Package artifact owns every path by which a schema document reaches or leaves
// disk.
//
// Two properties are the reason it exists as a package rather than as helpers
// beside each caller. Writes are atomic: bytes go to a temporary file in the
// destination directory, are flushed, and are then renamed over the
// destination, so an interrupted run leaves the previous artifact intact rather
// than a truncated file. And every read terminates in the recursive credential
// scan, so no caller is in a position to decide that its load path is the one
// that does not need scanning.
//
// The on-disk framing is specified in docs/formats/compression.md. Writing
// appends the extension the requested format implies; reading dispatches on the
// final extension alone and then sniffs the zstd frame magic inside a decrypted
// payload rather than trusting the name. That asymmetry is deliberate and is
// described there.
package artifact

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/envelope"
)

// Extensions the write path produces and the read path dispatches on.
const (
	extJSON      = ".json"
	extZstd      = ".zst"
	extEncrypted = ".enc"
)

// tempPattern names the temporary file a write goes to before it is renamed
// over the destination. The leading dot keeps a partial write out of a casual
// directory listing, and the suffix makes an abandoned one recognizable.
const tempPattern = ".dbsurveyor-*.tmp"

// artifactMode is the permission an artifact is created with.
//
// A schema document describes the shape of a database an operator has access
// to, which is reconnaissance material even with every value redacted. Owner
// read and write is the useful floor; a caller who wants it shared can widen it
// deliberately.
const artifactMode os.FileMode = 0o600

// ErrPasswordRequired reports that an artifact is encrypted, or was asked to be,
// and no password source was supplied.
var ErrPasswordRequired = errors.New("a password is required for an encrypted artifact")

// Document is a schema document that can check itself. Both *dbschema.Schema
// and *dbschema.ServerSchema satisfy it.
//
// The interface is stated in terms of Validate rather than a marshalling method
// because validation is the property this package depends on: Validate runs the
// recursive credential scan, and a document that fails it must not reach disk.
type Document interface {
	Validate() error
}

// PasswordFunc supplies the password for an encrypted artifact.
//
// It is a function rather than a value so that it is called only when an
// artifact actually turns out to be encrypted. A run that loads a plain file
// never prompts, and a caller can pass envelope.(*PasswordReader).ForOpen or
// ForSeal directly.
type PasswordFunc func() ([]byte, error)

// Format describes how an artifact is framed on disk.
//
// It is a description only: it carries no key material, so it is safe to log,
// compare, and print in a dry run.
type Format struct {
	// Compress frames the JSON as a zstd frame.
	Compress bool
	// Encrypt seals the payload in an envelope. When both are set the JSON is
	// compressed first and the envelope wraps the compressed bytes.
	Encrypt bool
}

// Extension reports the final extension a format is written with.
//
// Encryption wins over compression because the envelope is the outermost frame:
// a compressed-and-encrypted artifact is an envelope, and that a zstd frame
// sits inside it is not visible until it has been opened.
func (f Format) Extension() string {
	switch {
	case f.Encrypt:
		return extEncrypted
	case f.Compress:
		return extZstd
	default:
		return extJSON
	}
}

// NormalizePath returns the path a format is actually written to.
//
// A path that already ends in the format's extension is returned unchanged, so
// an operator who spells out "schema.json.zst" does not get
// "schema.json.zst.zst". Plain and compressed output are otherwise named after
// what they hold -- JSON, optionally zstd-framed -- so a bare "schema" becomes
// "schema.json" or "schema.json.zst" rather than "schema" or "schema.zst".
//
// The comparison is case-insensitive because a path typed on a case-insensitive
// filesystem may arrive as "SCHEMA.ENC", and appending ".enc" to that would
// produce a second extension for a file that already has one.
func NormalizePath(path string, format Format) string {
	if hasExtension(path, format.Extension()) {
		return path
	}

	if format.Encrypt {
		return path + extEncrypted
	}

	if !hasExtension(path, extJSON) {
		path += extJSON
	}

	if format.Compress {
		path += extZstd
	}

	return path
}

// hasExtension reports whether path ends in ext, ignoring case.
func hasExtension(path, ext string) bool {
	return strings.EqualFold(filepath.Ext(path), ext)
}

// Write validates document, frames it according to format, and writes it
// atomically. It returns the path actually written, which differs from path
// when extension normalization applied.
//
// Validation comes first and is not optional: it runs the recursive credential
// scan, so a document carrying a credential fails here rather than reaching
// disk. That ordering is the whole reason callers hand a document to this
// package instead of marshalling one themselves.
//
// password is consulted only when format.Encrypt is set, and the password it
// returns is erased before Write returns.
func Write(path string, document Document, format Format, password PasswordFunc) (string, error) {
	if err := document.Validate(); err != nil {
		return "", fmt.Errorf("refusing to write an invalid document: %w", err)
	}

	data, err := json.Marshal(document)
	if err != nil {
		return "", fmt.Errorf("marshal document: %w", err)
	}

	data, err = frame(data, format, password)
	if err != nil {
		return "", err
	}

	written := NormalizePath(path, format)
	if err := writeAtomic(written, data); err != nil {
		return "", err
	}

	return written, nil
}

// frame applies compression and encryption in the order the format specifies.
//
// Compression runs before encryption, never the other way around. Ciphertext is
// indistinguishable from random and does not compress, so the reverse order
// would cost the encryption work and save nothing; it would also leak the
// compressed size of the plaintext through the artifact's length in a way this
// ordering already accepts but does not compound.
func frame(data []byte, format Format, password PasswordFunc) ([]byte, error) {
	if format.Compress {
		framed, err := compress(data)
		if err != nil {
			return nil, err
		}

		data = framed
	}

	if !format.Encrypt {
		return data, nil
	}

	secret, err := resolvePassword(password)
	if err != nil {
		return nil, err
	}

	defer envelope.Zero(secret)

	sealed, err := envelope.Seal(data, secret)
	if err != nil {
		return nil, fmt.Errorf("seal artifact: %w", err)
	}

	return sealed, nil
}

// resolvePassword calls password, treating a missing source as an error rather
// than as an empty password.
func resolvePassword(password PasswordFunc) ([]byte, error) {
	if password == nil {
		return nil, ErrPasswordRequired
	}

	secret, err := password()
	if err != nil {
		return nil, fmt.Errorf("resolve artifact password: %w", err)
	}

	return secret, nil
}

// writeAtomic writes data to path by way of a temporary file in the same
// directory.
//
// The temporary file goes beside the destination rather than in the system
// temporary directory so that the rename stays within one filesystem, where it
// is atomic. A rename across filesystems degrades to a copy, which is exactly
// the partially written destination this function exists to prevent.
//
// The contents are flushed before the rename, so a crash leaves the destination
// either absent or holding the previous complete artifact. Durability of the
// directory entry itself after the rename is left to the filesystem; what is
// guaranteed here is that a reader never observes a truncated artifact.
func writeAtomic(path string, data []byte) error {
	dir := filepath.Dir(path)

	temp, err := os.CreateTemp(dir, tempPattern)
	if err != nil {
		return fmt.Errorf("create temporary file in %s: %w", dir, err)
	}

	name := temp.Name()

	if err := fill(temp, data); err != nil {
		discardError(temp.Close())
		discardError(os.Remove(name))

		return fmt.Errorf("write %s: %w", name, err)
	}

	if err := temp.Close(); err != nil {
		discardError(os.Remove(name))

		return fmt.Errorf("close %s: %w", name, err)
	}

	if err := os.Rename(name, path); err != nil {
		discardError(os.Remove(name))

		return fmt.Errorf("rename %s to %s: %w", name, path, err)
	}

	return nil
}

// fill writes data to the open temporary file, fixes its permissions, and
// flushes it to storage.
func fill(temp *os.File, data []byte) error {
	if err := temp.Chmod(artifactMode); err != nil {
		return fmt.Errorf("set permissions: %w", err)
	}

	if _, err := temp.Write(data); err != nil {
		return err
	}

	if err := temp.Sync(); err != nil {
		return fmt.Errorf("flush to storage: %w", err)
	}

	return nil
}

// discardError drops an error from a cleanup path.
//
// The cleanup runs because a write already failed, and there is nothing useful
// to do with a second failure: reporting it would replace the reason the write
// failed with the reason the tidying failed, which is strictly less useful to
// an operator. It is a named function rather than an assignment to the blank
// identifier so the decision is visible and the linter does not have to be
// silenced.
func discardError(error) {}

// WriteText writes a rendered document to path atomically.
//
// It exists because a report is written by the same temp-file-and-rename
// discipline as an artifact, and the primitives that discipline needs are
// reserved to this package -- so a caller that wants an atomic write asks for one
// here rather than reaching for os.OpenFile and defeating the reservation.
//
// Unlike Write it applies no framing, no extension dispatch, and no credential
// scan. The scan would be redundant: a rendered report is derived from a
// document that already passed it on the way in, and its own load path scans
// again. What it does share is the property that matters most -- a failed write
// leaves whatever was at path untouched, so an interrupted run does not replace
// yesterday's report with half of today's.
func WriteText(path string, contents []byte) error {
	return writeAtomic(path, contents)
}
