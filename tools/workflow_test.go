package tools

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// forbiddenWorkflowUses are actions no workflow may install a toolchain with.
//
// The version a Go toolchain action installs comes from the `go` directive in
// go.mod, which states a minimum language level rather than the toolchain this
// project runs. mise.toml is where the toolchain is pinned, and the two are
// allowed to differ: go.mod said 1.26.1 while mise.toml said 1.26.5.
//
// That gap is invisible until something reads the standard library's patch
// version, and govulncheck does. See GOTCHAS 4.5 for the incident.
var forbiddenWorkflowUses = []string{
	"actions/setup-go",
}

// usesKey matches a step's `uses` key, with or without the list marker and
// with or without space before the colon. YAML permits `uses : value`, so a
// plain search for "uses:" is an evasion this check should not have.
var usesKey = regexp.MustCompile(`^\s*(-\s*)?uses\s*:`)

// TestWorkflowsInstallTheToolchainThroughMise asserts that no workflow installs
// Go by any route other than mise.
//
// This is a test rather than a line in AGENTS.md because the documented version
// was already there and did not work: ci.yml has carried a comment explaining
// this exact hazard since the Go rewrite landed, and audit.yml sat beside it
// using actions/setup-go the whole time. A convention that only exists in prose
// is enforced by whoever happens to have read the prose.
func TestWorkflowsInstallTheToolchainThroughMise(t *testing.T) {
	root := repoRoot(t)

	for _, path := range trackedWorkflows(t, root) {
		data, err := os.ReadFile(filepath.Join(root, path))
		if err != nil {
			t.Errorf("read %s: %v", path, err)

			continue
		}

		for _, line := range strings.Split(string(data), "\n") {
			// A comment naming the action is how the workflows explain why they
			// do not use it, so match the `uses:` key rather than the name.
			if strings.HasPrefix(strings.TrimSpace(line), "#") {
				continue
			}

			if !usesKey.MatchString(line) {
				continue
			}

			for _, forbidden := range forbiddenWorkflowUses {
				if strings.Contains(line, forbidden) {
					t.Errorf(
						"%s installs a toolchain with %s; use jdx/mise-action so the version comes "+
							"from mise.toml rather than the go directive in go.mod (GOTCHAS 4.5)",
						path, forbidden,
					)
				}
			}
		}
	}
}

// trackedWorkflows returns the workflow files git is tracking.
//
// It asks git rather than walking the directory for the reason GOTCHAS 1.3
// records: a walk sees whatever is on disk, including scratch files nobody
// committed. The empty-set guard is GOTCHAS 1.1 -- this check is worthless if
// the pattern ever stops matching, and silence is how that would present.
func trackedWorkflows(t *testing.T, root string) []string {
	t.Helper()

	out, err := exec.CommandContext(t.Context(),
		"git", "-C", root, "ls-files", "-z", "--", ".github/workflows/*.yml", ".github/workflows/*.yaml").Output()
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
		t.Fatal("git ls-files matched no workflow files; the check would pass over nothing")
	}

	return files
}
