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

// TestNoSecretIsConvertedToString asserts the half of R17 a type cannot
// enforce on its own.
//
// dbadapter.Secret redacts itself through every fmt, JSON, and slog path, so
// the ordinary ways a credential reaches a log line are closed. Reveal is the
// deliberate exit, and it returns []byte rather than string precisely so that
// turning a credential into a string takes a visible conversion. A string is
// immutable and cannot be zeroed, so once one exists the credential outlives
// every attempt to erase it, and it is the form that flows effortlessly into
// error messages and structured log fields.
//
// The check is syntactic: a string(...) conversion wrapping anything that calls
// Reveal, or a call to the RevealString method that exists for the one case
// where the conversion is unavoidable. Test files are included -- a test that
// stringifies a credential is how the pattern gets copied into non-test code.
//
// Every database driver in use takes its password as a string field, so the
// conversion has to happen once per engine. That handoff is reserved to the
// connect.go of an adapter package by the path check below, which is the same
// shape as the reservation of the atomic-write primitives to internal/artifact:
// a path exception a reviewer can see, rather than a lint suppression a future
// caller can copy.
func TestNoSecretIsConvertedToString(t *testing.T) {
	root := repoRoot(t)

	for _, tree := range []string{"cmd", "internal"} {
		walkAllGoFiles(t, filepath.Join(root, tree), func(path string, file *ast.File) {
			rel, err := filepath.Rel(root, path)
			if err != nil {
				rel = path
			}

			rel = filepath.ToSlash(rel)
			if isDriverHandoff(rel) {
				return
			}

			for _, line := range stringConversionsOfReveal(file) {
				t.Errorf("%s: converts a revealed secret to a string at offset %d; "+
					"a string cannot be zeroed and flows into logs and errors -- keep it []byte, "+
					"and hand it to a driver only from an adapter's connect.go",
					rel, line)
			}
		})
	}
}

// isDriverHandoff reports whether a repository-relative path is the one file per
// adapter package where a driver is handed a credential.
//
// The package itself is not enough: internal/dbadapter defines RevealString and
// must not be exempt, or the reservation would exempt the thing being reserved.
func isDriverHandoff(rel string) bool {
	return strings.HasPrefix(rel, "internal/") &&
		strings.HasSuffix(rel, "/connect.go") &&
		!strings.HasPrefix(rel, "internal/dbadapter/")
}

// stringConversionsOfReveal reports the position of every place a secret becomes
// a string: a string(...) conversion wrapping a Reveal call, or a RevealString
// call in its own right.
func stringConversionsOfReveal(file *ast.File) []token.Pos {
	var found []token.Pos

	ast.Inspect(file, func(node ast.Node) bool {
		call, ok := node.(*ast.CallExpr)
		if !ok {
			return true
		}

		if selector, isSelector := call.Fun.(*ast.SelectorExpr); isSelector {
			if selector.Sel.Name == revealStringMethod {
				found = append(found, call.Pos())
			}

			return true
		}

		if ident, isIdent := call.Fun.(*ast.Ident); !isIdent || ident.Name != "string" {
			return true
		}

		for _, arg := range call.Args {
			if callsReveal(arg) {
				found = append(found, call.Pos())
			}
		}

		return true
	})

	return found
}

// revealStringMethod is the named exit whose call sites this check reserves.
const revealStringMethod = "RevealString"

// callsReveal reports whether expression contains a call to a method named
// Reveal.
func callsReveal(expression ast.Expr) bool {
	reveals := false

	ast.Inspect(expression, func(node ast.Node) bool {
		call, ok := node.(*ast.CallExpr)
		if !ok {
			return true
		}

		if selector, isSelector := call.Fun.(*ast.SelectorExpr); isSelector && selector.Sel.Name == "Reveal" {
			reveals = true

			return false
		}

		return true
	})

	return reveals
}

// walkAllGoFiles is walkGoFiles including test files.
func walkAllGoFiles(t *testing.T, dir string, visit func(path string, file *ast.File)) {
	t.Helper()

	fset := token.NewFileSet()

	err := filepath.WalkDir(dir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if entry.IsDir() || !strings.HasSuffix(path, ".go") {
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
