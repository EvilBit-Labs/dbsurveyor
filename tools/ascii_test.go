package tools

import (
	"io/fs"
	"os"
	"path/filepath"
	"testing"
	"unicode/utf8"
)

// TestSourcesAreASCII enforces R18 across whole files.
//
// The golangci-lint set includes asciicheck, but asciicheck only inspects
// declared identifiers. A curly quote in a doc comment, an em dash in an error
// string, or a checkmark in a Markdown table all pass it. Those are exactly the
// characters that have shown up in practice, so the requirement is checked
// against the file bytes here rather than assumed to be covered.
func TestSourcesAreASCII(t *testing.T) {
	root := repoRoot(t)

	skipDirs := map[string]struct{}{
		".git":         {},
		"target":       {},
		"dist":         {},
		"node_modules": {},
		// The Rust tree is retired and leaves the default branch in U12; it is
		// not held to the Go tree's source rules in the meantime.
		"dbsurveyor":         {},
		"dbsurveyor-collect": {},
		"dbsurveyor-core":    {},
		"project_plan":       {},
	}

	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if entry.IsDir() {
			if _, skip := skipDirs[entry.Name()]; skip {
				return filepath.SkipDir
			}

			return nil
		}

		if ext := filepath.Ext(entry.Name()); ext != ".go" && ext != ".md" {
			return nil
		}

		checkFileIsASCII(t, root, path)

		return nil
	})
	if err != nil {
		t.Fatalf("walk repository: %v", err)
	}
}

// checkFileIsASCII reports the first non-ASCII rune in path, with its line
// number, so a failure points at the character instead of just the file.
func checkFileIsASCII(t *testing.T, root, path string) {
	t.Helper()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Errorf("read %s: %v", path, err)

		return
	}

	line := 1

	for i := 0; i < len(data); {
		if data[i] == '\n' {
			line++
			i++

			continue
		}

		if data[i] < utf8.RuneSelf {
			i++

			continue
		}

		r, _ := utf8.DecodeRune(data[i:])

		rel, relErr := filepath.Rel(root, path)
		if relErr != nil {
			rel = path
		}

		t.Errorf("%s:%d contains non-ASCII character %q (%U); see R18 in AGENTS.md",
			filepath.ToSlash(rel), line, r, r)

		return
	}
}

// repoRoot walks up from the test's working directory to the module root.
func repoRoot(t *testing.T) string {
	t.Helper()

	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("working directory: %v", err)
	}

	for {
		if _, statErr := os.Stat(filepath.Join(dir, "go.mod")); statErr == nil {
			return dir
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatalf("no go.mod found above %s", dir)
		}

		dir = parent
	}
}
