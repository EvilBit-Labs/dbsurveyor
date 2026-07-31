package envelope

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// scriptedPrompt answers prompts with a fixed list of replies in order and
// records the labels it was shown, so a test can assert both what was returned
// and how many times the operator was asked.
type scriptedPrompt struct {
	replies []string
	labels  []string
	index   int
}

func (s *scriptedPrompt) prompt(label string) ([]byte, error) {
	s.labels = append(s.labels, label)

	if s.index >= len(s.replies) {
		return nil, errors.New("prompted more times than the test scripted")
	}

	reply := s.replies[s.index]
	s.index++

	return []byte(reply), nil
}

// readerWithEnv returns a reader whose environment holds the given value.
func readerWithEnv(value string, prompt func(string) ([]byte, error)) *PasswordReader {
	return &PasswordReader{
		lookupEnv: func(key string) (string, bool) {
			if key != PasswordEnvVar {
				return "", false
			}

			return value, true
		},
		prompt: prompt,
	}
}

// readerWithoutEnv returns a reader whose environment holds nothing.
func readerWithoutEnv(prompt func(string) ([]byte, error)) *PasswordReader {
	return &PasswordReader{
		lookupEnv: func(string) (string, bool) { return "", false },
		prompt:    prompt,
	}
}

func TestValidatePasswordEnforcesTheMinimumLength(t *testing.T) {
	require.ErrorIs(t, ValidatePassword(nil), ErrPasswordTooShort)
	require.ErrorIs(t, ValidatePassword([]byte("1234567")), ErrPasswordTooShort)
	require.NoError(t, ValidatePassword([]byte("12345678")))
}

func TestTheEnvironmentIsUsedWithoutPrompting(t *testing.T) {
	scripted := &scriptedPrompt{}

	for name, resolve := range map[string]func(*PasswordReader) ([]byte, error){
		"seal": (*PasswordReader).ForSeal,
		"open": (*PasswordReader).ForOpen,
	} {
		t.Run(name, func(t *testing.T) {
			password, err := resolve(readerWithEnv(testPassword, scripted.prompt))
			require.NoError(t, err)
			assert.Equal(t, []byte(testPassword), password)
			assert.Empty(t, scripted.labels, "a password from the environment must not prompt")
		})
	}
}

// TestSealConfirmsAPromptedPassword covers the reason for the second prompt: a
// typo would otherwise produce a file nobody can open.
func TestSealConfirmsAPromptedPassword(t *testing.T) {
	scripted := &scriptedPrompt{replies: []string{testPassword, testPassword}}

	password, err := readerWithoutEnv(scripted.prompt).ForSeal()
	require.NoError(t, err)
	assert.Equal(t, []byte(testPassword), password)
	assert.Len(t, scripted.labels, 2)
}

func TestSealRejectsAMismatchedConfirmation(t *testing.T) {
	scripted := &scriptedPrompt{replies: []string{testPassword, "example-passward"}}

	password, err := readerWithoutEnv(scripted.prompt).ForSeal()
	require.ErrorIs(t, err, ErrPasswordMismatch)
	assert.Nil(t, password)
}

// TestSealDoesNotConfirmAPasswordFromTheEnvironment records the asymmetry:
// reading the same variable twice cannot disagree with itself, so a second read
// would only be ceremony.
func TestSealDoesNotConfirmAPasswordFromTheEnvironment(t *testing.T) {
	scripted := &scriptedPrompt{}

	password, err := readerWithEnv(testPassword, scripted.prompt).ForSeal()
	require.NoError(t, err)
	assert.Equal(t, []byte(testPassword), password)
	assert.Empty(t, scripted.labels)
}

func TestOpenPromptsOnce(t *testing.T) {
	scripted := &scriptedPrompt{replies: []string{testPassword}}

	password, err := readerWithoutEnv(scripted.prompt).ForOpen()
	require.NoError(t, err)
	assert.Equal(t, []byte(testPassword), password)
	assert.Len(t, scripted.labels, 1)
}

// TestAShortPasswordIsRefusedIdenticallyFromBothSources is the guarantee that
// the minimum is a property of the password rather than of where it came from.
func TestAShortPasswordIsRefusedIdenticallyFromBothSources(t *testing.T) {
	const short = "short"

	fromEnvScripted := &scriptedPrompt{}
	fromEnv, envErr := readerWithEnv(short, fromEnvScripted.prompt).ForSeal()

	fromPrompt, promptErr := readerWithoutEnv(func(string) ([]byte, error) {
		return []byte(short), nil
	}).ForSeal()

	require.ErrorIs(t, envErr, ErrPasswordTooShort)
	require.ErrorIs(t, promptErr, ErrPasswordTooShort)
	assert.Equal(t, envErr.Error(), promptErr.Error())
	assert.Nil(t, fromEnv)
	assert.Nil(t, fromPrompt)
}

// TestAnEmptyEnvironmentVariableIsRefusedRatherThanIgnored keeps a common shell
// accident visible: falling through to a prompt would hide it.
func TestAnEmptyEnvironmentVariableIsRefusedRatherThanIgnored(t *testing.T) {
	scripted := &scriptedPrompt{replies: []string{testPassword}}

	password, err := readerWithEnv("", scripted.prompt).ForOpen()
	require.ErrorIs(t, err, ErrPasswordTooShort)
	assert.Nil(t, password)
	assert.Empty(t, scripted.labels)
}

func TestAPromptFailurePropagates(t *testing.T) {
	sentinel := errors.New("terminal closed")

	password, err := readerWithoutEnv(func(string) ([]byte, error) {
		return nil, sentinel
	}).ForSeal()

	require.ErrorIs(t, err, sentinel)
	assert.Nil(t, password)
}

// TestNoTerminalNamesTheEnvironmentVariable exercises the real prompt. The test
// process has no terminal on stdin, which is the same situation as a CI job or
// a container, and the error has to point somewhere useful.
func TestNoTerminalNamesTheEnvironmentVariable(t *testing.T) {
	reader := &PasswordReader{
		lookupEnv: func(string) (string, bool) { return "", false },
		prompt:    promptTerminal,
	}

	password, err := reader.ForOpen()
	require.ErrorIs(t, err, ErrNoTerminal)
	assert.Contains(t, err.Error(), PasswordEnvVar)
	assert.Nil(t, password)
}

// TestARejectedPasswordIsErased checks that a password refused for length does
// not linger in the caller's buffer.
func TestARejectedPasswordIsErased(t *testing.T) {
	password := []byte("short")

	returned, err := validated(password)
	require.ErrorIs(t, err, ErrPasswordTooShort)
	assert.Nil(t, returned)
	assert.Equal(t, make([]byte, len("short")), password)
}

func TestNewPasswordReaderUsesTheProcessEnvironment(t *testing.T) {
	t.Setenv(PasswordEnvVar, testPassword)

	password, err := NewPasswordReader().ForOpen()
	require.NoError(t, err)
	assert.Equal(t, []byte(testPassword), password)
}
