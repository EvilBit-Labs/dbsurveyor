// Command genschema writes the published JSON Schema and the published example
// documents for the dbsurveyor on-disk formats to docs/formats/.
//
// Both are derived from the Go types rather than hand-maintained, so neither can
// drift from the implementation. Regenerate with `just gen-schema`;
// TestPublishedArtifactsAreCurrent fails when a committed file and its
// generated form disagree.
package main

import (
	"fmt"
	"os"
	"path/filepath"
)

func main() {
	outDir := "docs/formats"
	if len(os.Args) > 1 {
		outDir = os.Args[1]
	}

	for _, artifact := range publishedArtifacts() {
		data, err := artifact.render()
		if err != nil {
			fmt.Fprintf(os.Stderr, "genschema: %v\n", err)
			os.Exit(1)
		}

		path := filepath.Join(outDir, artifact.filename)
		if err := writeFile(path, data); err != nil {
			fmt.Fprintf(os.Stderr, "genschema: %v\n", err)
			os.Exit(1)
		}

		fmt.Println("wrote", path)
	}
}

// writeFile creates the parent directory and writes data.
//
// This is a generator for tracked documentation, not a schema-document writer,
// so it is deliberately outside the internal/artifact atomic-write contract.
func writeFile(path string, data []byte) error {
	//nolint:gosec // G703: the output directory comes from this generator's own argv, run by a maintainer via `just gen-schema`, not from untrusted input.
	if err := os.MkdirAll(filepath.Dir(path), 0o750); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(path), err)
	}

	//nolint:forbidigo,gosec // See the doc comment above: this generator writes tracked documentation, not schema documents, so the internal/artifact contract does not apply. Published docs are world-readable by design.
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}

	return nil
}
