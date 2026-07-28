package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/charmbracelet/glamour"
	"github.com/spf13/cobra"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/envelope"
	"github.com/EvilBit-Labs/dbsurveyor/internal/progress"
	"github.com/EvilBit-Labs/dbsurveyor/internal/report"
)

// options are the flag values one run was given.
type options struct {
	output    string
	redaction string
	noRedact  bool
	analyze   bool
	samples   bool
	plain     bool
}

// ErrNoArtifact reports a run with no artifact to read.
var ErrNoArtifact = errors.New("no artifact path was given")

// newRootCommand builds the postprocessor's command.
//
// This file parses flags and wires; the loading, redaction, analysis, and
// rendering all live in internal/report (R9).
//
// Unlike the collector, this command reads no DATABASE_URL and takes no
// connection string. It works on artifacts, and an operator running it on a
// laptop with a stale DATABASE_URL in their shell should not be able to make it
// reach for a database by accident.
func newRootCommand(version string) *cobra.Command {
	opts := &options{}

	command := &cobra.Command{
		Use:   "dbsurveyor <artifact>",
		Short: "Render a schema artifact as a report",
		Long: strings.TrimSpace(`
Read a schema artifact and render it as Markdown.

The artifact may be plain, compressed, or encrypted; the extension decides, and
an encrypted one reads its password from ` + envelope.PasswordEnvVar + ` or a
prompt. Every load runs the recursive credential scan, so an artifact carrying a
credential fails before any report is produced.

This command makes no network call and reads no connection string. It works on
files.`),
		Args:          cobra.ExactArgs(1),
		Version:       version,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(command *cobra.Command, args []string) error {
			return run(command, args, opts)
		},
	}

	bindFlags(command, opts)

	// A mode and a request for no redaction are contradictory instructions, and
	// guessing which one an operator meant is how sampled values reach a report
	// they were supposed to be masked out of.
	command.MarkFlagsMutuallyExclusive("no-redact", "redact-mode")

	return command
}

// bindFlags declares the flag surface.
func bindFlags(command *cobra.Command, opts *options) {
	flags := command.Flags()

	flags.StringVarP(&opts.output, "output", "o", "",
		"write the report to a file instead of standard output")
	flags.StringVar(&opts.redaction, "redact-mode", string(dbschema.RedactBalanced),
		"how aggressively to mask sampled values: none, minimal, balanced, conservative")
	flags.BoolVar(&opts.noRedact, "no-redact", false,
		"render sampled values exactly as the artifact holds them")
	flags.BoolVar(&opts.analyze, "analyze", false,
		"score the sampled rows for completeness, consistency, and uniqueness")
	flags.BoolVar(&opts.samples, "samples", false,
		"include the sampled rows in the report")
	flags.BoolVar(&opts.plain, "plain", false,
		"emit raw Markdown even when standard output is a terminal")
}

// run loads the artifact and renders it.
func run(command *cobra.Command, args []string, opts *options) error {
	path := strings.TrimSpace(args[0])
	if path == "" {
		return ErrNoArtifact
	}

	reportOptions, err := opts.reportOptions()
	if err != nil {
		return err
	}

	document, err := report.Load(path, reportOptions)
	if err != nil {
		return err
	}

	rendered, err := report.Markdown(document, reportOptions)
	if err != nil {
		return err
	}

	return emit(command, opts, rendered)
}

// reportOptions turns the flag values into what the report package needs.
func (o *options) reportOptions() (report.Options, error) {
	mode := dbschema.RedactionMode(strings.ToLower(strings.TrimSpace(o.redaction)))
	if !mode.Valid() {
		return report.Options{}, fmt.Errorf(
			"unknown redact-mode %q: choose one of none, minimal, balanced, conservative",
			o.redaction,
		)
	}

	if o.noRedact {
		mode = dbschema.RedactNone
	}

	return report.Options{
		Redaction:      mode,
		Analyze:        o.analyze,
		IncludeSamples: o.samples,
		Password:       envelope.NewPasswordReader().ForOpen,
	}, nil
}

// emit writes the rendered report where the options ask for it.
//
// Writing to a file always writes raw Markdown: a file is read by another tool
// or committed to a repository, and escape sequences in one are corruption.
func emit(command *cobra.Command, opts *options, rendered string) error {
	if opts.output != "" {
		return writeFile(opts.output, rendered)
	}

	out := command.OutOrStdout()

	if opts.plain || !decorate(out) {
		if _, err := io.WriteString(out, rendered); err != nil {
			return fmt.Errorf("write the report: %w", err)
		}

		return nil
	}

	styled, err := glamour.Render(rendered, "auto")
	if err != nil {
		// Falling back to raw Markdown is right: the report is the deliverable
		// and the styling is not, so a failure to style is not a failure to
		// report.
		if _, writeErr := io.WriteString(out, rendered); writeErr != nil {
			return fmt.Errorf("write the report: %w", writeErr)
		}

		return nil
	}

	if _, err := io.WriteString(out, styled); err != nil {
		return fmt.Errorf("write the report: %w", err)
	}

	return nil
}

// decorate reports whether the output stream should be styled.
//
// It reuses the collector's decision so the two binaries agree: TERM=dumb and a
// non-terminal both mean plain output, and a report piped into a file must never
// carry escape sequences.
func decorate(out io.Writer) bool {
	return progress.Decorate(out, os.Getenv("TERM"))
}

// writeFile writes the report atomically, through internal/artifact.
//
// forbidigo refuses os.Create and os.WriteFile repository-wide and
// tools/architecture_test.go reserves the rest of the atomic-write primitives to
// that package, so this is not a detour -- it is the only way to write a file,
// and a report gets the same guarantee an artifact does: a failed write leaves
// yesterday's report intact rather than replacing it with half of today's.
func writeFile(path, rendered string) error {
	if err := artifact.WriteText(path, []byte(rendered)); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}

	return nil
}

// exitCode maps an error to a process exit status.
func exitCode(err error) int {
	switch {
	case err == nil:
		return 0
	case errors.Is(err, ErrNoArtifact), errors.Is(err, report.ErrNoInput):
		return usageExitCode
	default:
		return failureExitCode
	}
}

// The process exit statuses.
const (
	usageExitCode   = 2
	failureExitCode = 1
)
