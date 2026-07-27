package envelope

import (
	"crypto/subtle"
	"errors"
	"fmt"
	"os"

	"golang.org/x/term"
)

// PasswordEnvVar names the environment variable that supplies a password
// without a prompt, for scripted and airgapped runs where no terminal exists.
//
//nolint:gosec // G101: this is the name of a variable to read, not a credential.
const PasswordEnvVar = "DBSURVEYOR_ENCRYPTION_PASSWORD"

// MinPasswordLength is the shortest password accepted, from any source.
//
// Eight characters is a floor against typos and empty variables, not a claim
// that an eight-character password is strong. What makes a guess expensive is
// the Argon2id cost, not this number.
const MinPasswordLength = 8

// ErrPasswordTooShort reports a password below MinPasswordLength. It is the
// same error whether the password came from the environment or a prompt.
var ErrPasswordTooShort = fmt.Errorf("password must be at least %d characters", MinPasswordLength)

// ErrPasswordMismatch reports that a password and its confirmation differ.
var ErrPasswordMismatch = errors.New("passwords do not match")

// ErrNoTerminal reports that no terminal is attached to prompt on.
var ErrNoTerminal = errors.New("no terminal available to prompt for a password")

// ValidatePassword reports whether a password may be used. Every source routes
// through it, so a short password is refused identically wherever it came from.
func ValidatePassword(password []byte) error {
	if len(password) < MinPasswordLength {
		return ErrPasswordTooShort
	}

	return nil
}

// PasswordReader resolves a password from the environment or from a terminal.
//
// The environment is checked first: a run with PasswordEnvVar set never blocks
// on a prompt, which is what makes the tool usable from a script or a container
// with no attached terminal.
//
// A password read from the environment cannot be erased. Go hands back a string
// from the process environment block, and neither that string nor the block is
// something this package can overwrite. Zero is applied where it can be, and no
// guarantee beyond best effort is claimed.
type PasswordReader struct {
	lookupEnv func(key string) (string, bool)
	prompt    func(label string) ([]byte, error)
}

// NewPasswordReader returns a reader over the real environment and terminal.
func NewPasswordReader() *PasswordReader {
	return &PasswordReader{lookupEnv: os.LookupEnv, prompt: promptTerminal}
}

// ForSeal resolves the password used to seal an envelope.
//
// A prompted password is asked for twice, because a typo would otherwise
// produce a file nobody can open. A password from the environment is not asked
// for twice: reading the same variable again cannot disagree with itself.
func (r *PasswordReader) ForSeal() ([]byte, error) {
	if password, ok := r.fromEnv(); ok {
		return validated(password)
	}

	password, err := r.prompt("Encryption password: ")
	if err != nil {
		return nil, err
	}

	password, err = validated(password)
	if err != nil {
		return nil, err
	}

	confirmation, err := r.prompt("Confirm encryption password: ")
	if err != nil {
		Zero(password)

		return nil, err
	}

	defer Zero(confirmation)

	if subtle.ConstantTimeCompare(password, confirmation) != 1 {
		Zero(password)

		return nil, ErrPasswordMismatch
	}

	return password, nil
}

// ForOpen resolves the password used to open an envelope. There is nothing to
// confirm against: the envelope itself is the check, and a wrong password fails
// the tag.
func (r *PasswordReader) ForOpen() ([]byte, error) {
	if password, ok := r.fromEnv(); ok {
		return validated(password)
	}

	password, err := r.prompt("Decryption password: ")
	if err != nil {
		return nil, err
	}

	return validated(password)
}

// fromEnv reports the password held in PasswordEnvVar.
//
// A variable that is set but empty counts as set, so it fails as too short
// rather than falling through to a prompt. An operator who exported the
// variable meant to use it, and silently ignoring it would hide the mistake.
func (r *PasswordReader) fromEnv() ([]byte, bool) {
	value, ok := r.lookupEnv(PasswordEnvVar)
	if !ok {
		return nil, false
	}

	return []byte(value), true
}

// validated returns the password when it passes ValidatePassword, and erases it
// otherwise so a rejected password does not outlive the rejection.
func validated(password []byte) ([]byte, error) {
	if err := ValidatePassword(password); err != nil {
		Zero(password)

		return nil, err
	}

	return password, nil
}

// promptTerminal reads a password from the terminal without echoing it.
//
// The prompt is written to stderr so that piping stdout to a file or another
// process never captures it.
func promptTerminal(label string) ([]byte, error) {
	//nolint:gosec // G115: a file descriptor is small on every supported platform.
	fd := int(os.Stdin.Fd())
	if !term.IsTerminal(fd) {
		return nil, fmt.Errorf("%w: set %s instead", ErrNoTerminal, PasswordEnvVar)
	}

	if _, err := fmt.Fprint(os.Stderr, label); err != nil {
		return nil, fmt.Errorf("write password prompt: %w", err)
	}

	password, readErr := term.ReadPassword(fd)

	// The newline is written whether or not the read succeeded, so the shell
	// prompt does not resume on the same line as the invisible input.
	if _, err := fmt.Fprintln(os.Stderr); err != nil {
		return nil, fmt.Errorf("write password prompt: %w", err)
	}

	if readErr != nil {
		return nil, fmt.Errorf("read password: %w", readErr)
	}

	return password, nil
}
