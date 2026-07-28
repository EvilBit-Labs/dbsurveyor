// Command dbsurveyor reads a schema artifact and renders it as a report.
//
// The package parses flags and wires dependencies; loading, redaction, analysis,
// and rendering all live in internal/report (R9).
//
// It makes no network call and reads no connection string. The collector talks
// to databases; this reads files.
package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/charmbracelet/fang"
)

// The build stamps these. GoReleaser sets them through ldflags; a `go build`
// leaves the development defaults, which is what such a binary should report.
var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

func main() {
	// The exit is here and the work is below, so that the deferred signal
	// teardown in runCommand actually runs: os.Exit skips every pending defer in
	// the function that calls it.
	os.Exit(runCommand())
}

// runCommand executes the postprocessor and reports its exit status.
func runCommand() int {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	// fang supplies its own version handling and would otherwise report the
	// build info of a binary that has none, so the stamped version is passed
	// through explicitly.
	return exitCode(fang.Execute(
		ctx,
		newRootCommand(buildVersion()),
		fang.WithVersion(buildVersion()),
		fang.WithCommit(commit),
	))
}

// buildVersion renders the version string the binary reports.
func buildVersion() string {
	if commit == "none" {
		return version
	}

	return fmt.Sprintf("%s (%s, %s)", version, commit, date)
}
