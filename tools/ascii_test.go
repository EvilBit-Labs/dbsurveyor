package tools

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
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

	for _, rel := range trackedSources(t, root) {
		checkFileIsASCII(t, root, filepath.Join(root, rel))
	}
}

// trackedSources lists the .go and .md files git is tracking, as paths relative
// to root.
//
// R18 is a rule about repository source, and asking git what that is rather than
// walking the filesystem is what makes the two agree. A walk sees whatever
// happens to be in the working tree -- an editor's scratch file, a vendored
// dependency, an agent's notes -- and fails the build over a file no contributor
// ever committed, which is how a gate stops being trusted. It also silently
// depends on a skip list; see the repository-root hazard in GOTCHAS section 1.1.
//
// The count is asserted because this check has already reported success over an
// empty set once. A git invocation that returns nothing must fail loudly rather
// than pass quietly.
func trackedSources(t *testing.T, root string) []string {
	t.Helper()

	out, err := exec.CommandContext(t.Context(),
		"git", "-C", root, "ls-files", "-z", "--", "*.go", "*.md").Output()
	if err != nil {
		t.Fatalf("git ls-files: %v", err)
	}

	var files []string

	for _, name := range strings.Split(string(out), "\x00") {
		if name != "" {
			files = append(files, name)
		}
	}

	if len(files) == 0 {
		t.Fatal("git ls-files matched no .go or .md files; the check would pass over nothing")
	}

	return files
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
