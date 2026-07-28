// Command dbsurveyor-collect surveys a database and writes a portable schema
// artifact.
//
// The package parses flags and wires dependencies; every decision about what a
// survey does lives in internal/survey (R9). wire.go is the only file in the
// tree that imports an adapter package (R11).
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

// runCommand executes the collector and reports its exit status.
func runCommand() int {
	// A survey holds a connection to somebody else's database, so an interrupt
	// has to reach the query rather than only the process. Cancelling the
	// context closes the pool through the deferred close in internal/survey,
	// which is the difference between a clean disconnect and a session the
	// server has to time out.
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	return exitCode(fang.Execute(ctx, newRootCommand(buildVersion())))
}

// buildVersion renders the version string the binary reports and stamps into
// every artifact it writes.
func buildVersion() string {
	if commit == "none" {
		return version
	}

	return fmt.Sprintf("%s (%s, %s)", version, commit, date)
}
