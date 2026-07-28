package progress

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// escapeSequence matches an ANSI control sequence, which is the thing that must
// never reach a log file or a pipe.
var escapeSequence = regexp.MustCompile(`\x1b\[[0-9;]*[a-zA-Z]`)

// TestDumbTerminalsGetNoDecoration is the case the plan calls out explicitly.
// TERM=dumb is set deliberately -- by an editor's shell, by a CI runner, by a
// script that wants clean output -- and it is a request, not a limitation to
// probe around.
func TestDumbTerminalsGetNoDecoration(t *testing.T) {
	assert.False(t, Decorate(os.Stdout, "dumb"))
	assert.False(t, Decorate(os.Stdout, "DUMB"), "matched without regard to case")
}

// TestANonTerminalGetsNoDecoration covers the other half. A pipe is a pipe
// whatever TERM says, and escape sequences written into one end up in a file
// somebody greps later.
func TestANonTerminalGetsNoDecoration(t *testing.T) {
	assert.False(t, Decorate(&bytes.Buffer{}, "xterm-256color"),
		"a buffer is not a terminal")
	assert.False(t, Decorate(io.Discard, "xterm-256color"))

	// A real file is an *os.File and still not a terminal, which is the case a
	// type check alone would get wrong.
	path := filepath.Join(t.TempDir(), "out.log")

	file, err := os.OpenFile(
		path,
		os.O_CREATE|os.O_WRONLY,
		0o600,
	)
	require.NoError(t, err)

	defer func() { require.NoError(t, file.Close()) }()

	assert.False(t, Decorate(file, "xterm-256color"), "a redirected file is not a terminal")
}

// TestAnUnsetTerminalGetsNoDecoration covers the environment a container or a
// cron job runs in, where TERM is absent rather than dumb.
func TestAnUnsetTerminalGetsNoDecoration(t *testing.T) {
	assert.False(t, Decorate(os.Stdout, ""))
}

// TestPlainOutputContainsNoEscapeSequences is the property the decision exists
// to produce, asserted on the bytes rather than on the decision.
func TestPlainOutputContainsNoEscapeSequences(t *testing.T) {
	var out bytes.Buffer

	reporter := NewPlain(&out)
	reporter.Start(3, "collecting the schema")
	reporter.Step("users")
	reporter.Step("orders")
	reporter.Warn("one table could not be read")
	reporter.Done()

	written := out.String()

	assert.NotRegexp(t, escapeSequence, written, "no escape sequence reaches a plain writer")
	assert.Contains(t, written, "collecting the schema")
	assert.Contains(t, written, "users")
	assert.Contains(t, written, "warning: one table could not be read")
}

// TestTheCounterReportsPositionWithinThePhase checks the one piece of state the
// reporter keeps.
func TestTheCounterReportsPositionWithinThePhase(t *testing.T) {
	var out bytes.Buffer

	reporter := NewPlain(&out)
	reporter.Start(2, "sampling")
	reporter.Step("users")
	reporter.Step("orders")
	reporter.Done()

	lines := strings.Split(strings.TrimSpace(out.String()), "\n")
	require.Len(t, lines, 4)

	assert.Contains(t, lines[1], "[1/2]")
	assert.Contains(t, lines[2], "[2/2]")
	assert.Contains(t, lines[3], "sampling: done")
}

// TestAnUnknownTotalStillCounts covers a phase whose size is not known ahead of
// time, which is every phase but sampling.
func TestAnUnknownTotalStillCounts(t *testing.T) {
	var out bytes.Buffer

	reporter := NewPlain(&out)
	reporter.Start(0, "collecting")
	reporter.Step("one thing")

	assert.Contains(t, out.String(), "[1]")
	assert.NotContains(t, out.String(), "/0", "a zero total is not rendered as a denominator")
}

// TestAPhaseResetsItsCounter keeps two phases from accumulating into one count.
func TestAPhaseResetsItsCounter(t *testing.T) {
	var out bytes.Buffer

	reporter := NewPlain(&out)
	reporter.Start(1, "first")
	reporter.Step("a")
	reporter.Done()

	reporter.Start(1, "second")
	reporter.Step("b")

	assert.Contains(t, out.String(), "[1/1] b", "the second phase starts from zero")
}

// TestDoneWithoutStartSaysNothing covers the ordering a caller can get wrong.
func TestDoneWithoutStartSaysNothing(t *testing.T) {
	var out bytes.Buffer

	NewPlain(&out).Done()

	assert.Empty(t, out.String())
}

// TestWarningsAreWrittenWhateverTheDecoration records the distinction the
// decoration decision does not cover: it is about escape sequences, not about
// whether the operator gets told things.
func TestWarningsAreWrittenWhateverTheDecoration(t *testing.T) {
	var plain bytes.Buffer

	NewPlain(&plain).Warn("a table could not be read")
	assert.Contains(t, plain.String(), "a table could not be read")

	var decorated bytes.Buffer

	reporter := &Writer{out: &decorated, decorated: true}
	reporter.Warn("a table could not be read")
	assert.Contains(t, decorated.String(), "a table could not be read")
}

// TestDecoratedOutputStillCarriesItsText keeps the styling from swallowing the
// message it is styling.
func TestDecoratedOutputStillCarriesItsText(t *testing.T) {
	var out bytes.Buffer

	reporter := &Writer{out: &out, decorated: true}
	reporter.Start(1, "collecting")
	reporter.Step("users")
	reporter.Done()

	assert.Contains(t, out.String(), "collecting")
	assert.Contains(t, out.String(), "users")
}

// TestNewDecidesForItself checks the constructor wires the decision rather than
// defaulting one way.
func TestNewDecidesForItself(t *testing.T) {
	var out bytes.Buffer

	assert.False(t, New(&out).decorated, "a buffer never gets decoration")
}

// TestDiscardReportsNothing covers the reporter a library caller passes.
func TestDiscardReportsNothing(t *testing.T) {
	assert.NotPanics(t, func() {
		Discard.Start(1, "collecting")
		Discard.Step("users")
		Discard.Warn("something")
		Discard.Done()
	})
}
