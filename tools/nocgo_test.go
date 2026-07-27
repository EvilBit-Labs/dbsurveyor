// Package tools holds repository-level architecture tests that assert
// properties of the build itself rather than of any single package.
package tools

import (
	"os/exec"
	"strings"
	"testing"
)

// TestNoCGODependency asserts R13: no package in the dependency graph imports
// "C". This is checked rather than assumed because the no-CGO rule is what
// keeps the six database drivers pure Go, which is in turn what makes airgapped
// installs work without vendor client libraries. A single CGO-requiring
// dependency entering go.sum would silently break static linking and the
// cross-compilation story in .goreleaser.yaml.
//
// CGO_ENABLED=0 in CI would make such an import fail to build, but only for the
// platforms CI actually builds. This test fails on the dependency graph itself,
// so a CGO import is caught even when it sits behind a build tag for a platform
// CI does not cover.
func TestNoCGODependency(t *testing.T) {
	out, err := exec.Command("go", "list", "-deps", "./...").Output()
	if err != nil {
		t.Fatalf("go list -deps ./...: %v", err)
	}

	for _, pkg := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if strings.TrimSpace(pkg) == "C" {
			t.Error(`dependency graph imports "C": CGO is forbidden (R13). ` +
				`Find the offending module with: go list -deps -f '{{.ImportPath}} {{.CgoFiles}}' ./...`)
		}
	}
}

// TestNoCgoFiles asserts the same property from the other direction: no package
// in the graph carries .cgo files. A package can pull in CGO through cgo source
// files without "C" appearing as a distinct entry in the dependency list, so
// both checks are needed to cover R13.
func TestNoCgoFiles(t *testing.T) {
	out, err := exec.Command("go", "list", "-deps",
		"-f", "{{if .CgoFiles}}{{.ImportPath}}{{end}}", "./...").Output()
	if err != nil {
		t.Fatalf("go list -deps ./...: %v", err)
	}

	if offenders := strings.TrimSpace(string(out)); offenders != "" {
		t.Errorf("packages with cgo files (R13 forbids CGO):\n%s", offenders)
	}
}
