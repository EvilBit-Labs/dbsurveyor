// Package progress reports the progress of a survey to a terminal.
//
// The one thing it has to get right is knowing when to say nothing decorative.
// A survey runs in CI logs, in a pipeline, and over an SSH session on a host
// whose terminal cannot render anything -- and escape sequences written into any
// of those are noise an operator has to read past forever afterwards.
package progress

import (
	"fmt"
	"io"
	"os"
	"strings"
	"sync"

	"github.com/charmbracelet/lipgloss"
	"golang.org/x/term"
)

// dumbTerminal is the TERM value that means "this terminal understands nothing".
//
// It is honored explicitly rather than left to a capability probe because it is
// the value a caller sets deliberately: an editor's shell, a CI runner, and a
// script that wants clean output all set it to ask for exactly this.
const dumbTerminal = "dumb"

// Reporter is what a survey reports through.
//
// It is an interface so a caller that wants no output at all -- a test, a
// library use -- passes Discard rather than a reporter writing to a discarded
// writer, which would still do the formatting work.
type Reporter interface {
	// Start announces a phase with a known number of steps. A total of zero
	// means the count is not known ahead of time.
	Start(total int, label string)
	// Step announces one completed unit of work.
	Step(label string)
	// Warn reports a non-fatal problem. It is written even when decoration is
	// off, because a warning is information rather than decoration.
	Warn(message string)
	// Done closes the current phase.
	Done()
}

// Discard is a Reporter that reports nothing.
var Discard Reporter = discard{}

// discard implements Reporter by doing nothing.
type discard struct{}

func (discard) Start(int, string) {}
func (discard) Step(string)       {}
func (discard) Warn(string)       {}
func (discard) Done()             {}

// Writer reports progress to a writer, with or without decoration.
type Writer struct {
	out       io.Writer
	decorated bool

	// mu guards the counters. A survey is sequential today, but a reporter is
	// exactly the thing a future concurrent collection would share.
	mu        sync.Mutex
	total     int
	completed int
	phase     string
}

// Writer is a Reporter. The assertion is here rather than in a test so that a
// signature drift is a build failure in this package.
var _ Reporter = (*Writer)(nil)

// New returns a reporter for out, deciding for itself whether to decorate.
func New(out io.Writer) *Writer {
	return &Writer{out: out, decorated: Decorate(out, os.Getenv("TERM"))}
}

// NewPlain returns a reporter that never decorates, whatever out is.
func NewPlain(out io.Writer) *Writer {
	return &Writer{out: out}
}

// Decorate reports whether output to out should carry styling.
//
// The terminal name is a parameter rather than read from the environment here,
// so the decision is testable without mutating the process's environment -- which
// is global state a parallel test would race on.
//
// Both conditions have to hold. A TTY with TERM=dumb has asked for plain output;
// a pipe with TERM=xterm is a pipe whatever the terminal would have supported,
// and writing escape sequences into it puts them in a file somebody greps later.
func Decorate(out io.Writer, terminal string) bool {
	if strings.EqualFold(terminal, dumbTerminal) || terminal == "" {
		return false
	}

	file, isFile := out.(*os.File)
	if !isFile {
		return false
	}

	return term.IsTerminal(int(file.Fd()))
}

// Start announces a phase.
func (w *Writer) Start(total int, label string) {
	w.mu.Lock()
	defer w.mu.Unlock()

	w.total = total
	w.completed = 0
	w.phase = label

	w.write(w.style(phaseStyle, label))
}

// Step announces one completed unit of work.
func (w *Writer) Step(label string) {
	w.mu.Lock()
	defer w.mu.Unlock()

	w.completed++

	w.write("  " + w.counter() + " " + label)
}

// Warn reports a non-fatal problem.
//
// A warning is written whether or not decoration is on. The decoration decision
// is about escape sequences, not about whether the operator gets told things.
func (w *Writer) Warn(message string) {
	w.mu.Lock()
	defer w.mu.Unlock()

	w.write(w.style(warningStyle, "warning: ") + message)
}

// Done closes the current phase.
func (w *Writer) Done() {
	w.mu.Lock()
	defer w.mu.Unlock()

	if w.phase == "" {
		return
	}

	w.write(w.style(phaseStyle, w.phase+": done") + " " + w.counter())
	w.phase = ""
}

// counter renders the position within the phase, or the empty string when there
// is nothing to count.
func (w *Writer) counter() string {
	if w.total <= 0 {
		return fmt.Sprintf("[%d]", w.completed)
	}

	return fmt.Sprintf("[%d/%d]", w.completed, w.total)
}

// style applies a style, or returns the text unchanged when decoration is off.
func (w *Writer) style(style lipgloss.Style, text string) string {
	if !w.decorated {
		return text
	}

	return style.Render(text)
}

// write emits one line, dropping the error.
//
// A survey that failed because its progress output could not be written would be
// failing over the least important thing it does. The error is dropped here
// rather than propagated for that reason, and the drop is a named call rather
// than a blank assignment so it stays visible.
func (w *Writer) write(line string) {
	discardError(fmt.Fprintln(w.out, line))
}

// The styles used when decoration is on. They are colors only: no cursor
// movement, no line rewriting, so the output is the same whether it is watched
// live or read back from a log.
var (
	phaseStyle   = lipgloss.NewStyle().Bold(true)
	warningStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("3"))
)

// discardError drops an error that carries no information a caller can act on.
func discardError(int, error) {}
