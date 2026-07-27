package tools

import (
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// modulePath is the import prefix of every package in this tree.
const modulePath = "github.com/EvilBit-Labs/dbsurveyor"

// artifactPackage owns the atomic-write primitives the check below reserves.
const artifactPackage = "internal/artifact"

// TestEnvelopeDoesNotImportArtifact asserts the layering the plan states:
// artifact imports envelope, never the reverse.
//
// The direction matters because the envelope package is the security-review
// surface. Keeping it free of file handling means a reviewer auditing the
// cryptography does not also have to reason about paths, temporary files, and
// extension dispatch, and it keeps the byte format usable by anything that is
// not an artifact on disk.
func TestEnvelopeDoesNotImportArtifact(t *testing.T) {
	target := modulePath + "/internal/envelope"

	out, err := exec.CommandContext(t.Context(), "go", "list", "-deps", target).Output()
	if err != nil {
		t.Fatalf("go list -deps %s: %v", target, err)
	}

	forbidden := modulePath + "/" + artifactPackage

	for _, pkg := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if strings.TrimSpace(pkg) == forbidden {
			t.Errorf("internal/envelope depends on %s; the dependency runs the other way", forbidden)
		}
	}
}

// reservedCalls are the primitives that make a write atomic. They belong to
// internal/artifact alone.
//
// golangci-lint's forbidigo rule already refuses os.Create and os.WriteFile
// everywhere, which stops the obvious ways to write a file directly. It cannot
// express "except in one package", so the primitives that package legitimately
// needs are reserved here instead, where the exception is a path check rather
// than a lint suppression a future caller could copy.
var reservedCalls = map[string]string{
	"os.Rename":     "the rename that publishes an artifact",
	"os.CreateTemp": "the temporary file an atomic write goes through",
	"os.OpenFile":   "opening a destination for writing",
}

// TestAtomicWritePrimitivesAreReservedToArtifact walks the Go sources under
// cmd/ and internal/ and fails if any package other than internal/artifact
// calls one of the reserved primitives.
//
// Test files are exempt: a test that stages a fixture on disk is not a code
// path an operator's artifact travels.
func TestAtomicWritePrimitivesAreReservedToArtifact(t *testing.T) {
	root := repoRoot(t)

	for _, tree := range []string{"cmd", "internal"} {
		walkGoFiles(t, filepath.Join(root, tree), func(path string, file *ast.File) {
			rel, err := filepath.Rel(root, path)
			if err != nil {
				rel = path
			}

			rel = filepath.ToSlash(rel)
			if strings.HasPrefix(rel, artifactPackage+"/") {
				return
			}

			for _, call := range selectorCalls(file) {
				if reason, reserved := reservedCalls[call]; reserved {
					t.Errorf("%s calls %s (%s); route schema output through %s",
						rel, call, reason, artifactPackage)
				}
			}
		})
	}
}

// walkGoFiles parses every non-test Go file under dir and hands it to visit. A
// missing directory is not a failure: the tree grows a package at a time.
func walkGoFiles(t *testing.T, dir string, visit func(path string, file *ast.File)) {
	t.Helper()

	fset := token.NewFileSet()

	err := filepath.WalkDir(dir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if entry.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}

		file, parseErr := parser.ParseFile(fset, path, nil, parser.SkipObjectResolution)
		if parseErr != nil {
			t.Errorf("parse %s: %v", path, parseErr)

			return nil
		}

		visit(path, file)

		return nil
	})
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("walk %s: %v", dir, err)
	}
}

// selectorCalls reports every call of the form pkg.Func in file, as "pkg.Func".
//
// The match is on the source text rather than on resolved types, so a package
// imported under an alias would slip past. That is acceptable: the check exists
// to catch a caller reaching for the obvious primitive, not to defeat someone
// deliberately hiding it, and a deliberate alias would be visible in review.
func selectorCalls(file *ast.File) []string {
	var calls []string

	ast.Inspect(file, func(node ast.Node) bool {
		call, ok := node.(*ast.CallExpr)
		if !ok {
			return true
		}

		selector, ok := call.Fun.(*ast.SelectorExpr)
		if !ok {
			return true
		}

		pkg, ok := selector.X.(*ast.Ident)
		if !ok {
			return true
		}

		calls = append(calls, pkg.Name+"."+selector.Sel.Name)

		return true
	})

	return calls
}
