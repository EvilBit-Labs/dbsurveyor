# Development Gotchas and Pitfalls

Non-obvious behaviors and hard-earned lessons. Referenced from `AGENTS.md` and
`CONTRIBUTING.md`. Read it before changing anything in the areas it covers.

Entries marked **[carried]** were learned against the retired Rust
implementation and describe the *database*, not the language. They are believed
to hold for the Go tree but have not all been re-validated against it yet; the
adapter unit that lands each engine is responsible for confirming or correcting
its entries (R3).

## 1. Tests that report success over nothing

Two repository-level tests in this tree passed for months while checking an
empty set. Both are fixed; the pattern is the lesson.

### 1.1 A skip list that matched the repository root

`tools/ascii_test.go` walks the tree and skips directories by base name. Its skip
list named the retired Rust crate directories, one of which was `dbsurveyor` --
which is also the name of the repository root. `filepath.WalkDir` visits the root
first, matched it, returned `SkipDir`, and the entire walk ended before reading a
file. R18 had never actually been enforced.

The root is now exempt from the name match explicitly. When adding a skip entry,
consider that a checkout can be named anything, including the thing you are
trying to skip.

### 1.2 A package pattern relative to the test binary

`tools/nocgo_test.go` ran `go list -deps ./...` to assert R13 over the whole
dependency graph. A test binary's working directory is its own package
directory, so `./...` expanded to `tools` and `tools/genschema` and nothing else.
Both checks now name the module path explicitly.

Any test that shells out to `go list`, `go build`, or `git` should use absolute
or module-qualified arguments. Relative ones resolve against a directory that is
rarely the one you pictured.

### 1.3 The general rule

Before trusting a new invariant test, break the invariant on purpose and watch it
fail. A test that has never failed is not evidence that the property holds.

## 2. Artifacts and framing

### 2.1 Extension dispatch is asymmetric

Writing appends the extension the requested options imply. Reading dispatches on
the *final* extension only, and then, inside a decrypted payload, on the zstd
frame magic (`28 B5 2F FD`) rather than on the filename. A
compressed-and-encrypted artifact is named `.enc` and its name says nothing about
the zstd frame inside it.

The `.zst` path deliberately does **not** sniff: a file named `.zst` that is not
a zstd frame is an error, not a JSON file to fall back on.

Full rules in `docs/formats/compression.md`.

### 2.2 Compression happens before encryption

Never the reverse. Ciphertext is indistinguishable from random and does not
compress, so encrypting first pays for the compression pass and saves nothing.

### 2.3 The credential scan runs on bytes, before decoding

`json.Unmarshal` silently drops any field the Go types do not declare. A scan
that ran on the decoded document would pronounce a file clean while a credential
sat in a field this version does not know about. `internal/artifact` scans the
JSON bytes first, then decodes, then validates -- and validation scans again,
because it is the enforcement point for documents that never touched disk.

### 2.4 `os.Create` and `os.WriteFile` are forbidden outside `internal/artifact`

`forbidigo` refuses them repository-wide, and
`tools/architecture_test.go` additionally reserves `os.Rename`, `os.CreateTemp`,
and `os.OpenFile` to that package. The rule exists so the atomic-write contract
and the credential scan cannot be routed around. If you need to write a file, you
need `internal/artifact`.

### 2.5 Unauthenticated input needs a ceiling, not just a floor

Both the Argon2id cost parameters and the zstd decompressed size arrive from a
file and are consumed *before* anything is authenticated -- the key must be
derived before the GCM tag can be checked, and a plain `.zst` has no tag at all.
Each therefore has an upper bound as well as a lower one. The Rust
implementation validated only minimums, which left an artifact naming a
multi-terabyte memory cost able to crash the process on open.

## 3. Linters

### 3.1 `nonamedreturns` versus gocritic `unnamedResult`

Both are enabled. `unnamedResult` wants multiple return values named;
`nonamedreturns` forbids naming them. When they collide -- which happens for
functions returning two values of the same type -- the resolution is to return a
small struct instead of two values. This has come up twice; do not resolve it
with a suppression.

### 3.2 `errcheck` has `check-blank: true`

`_ = f()` is flagged just as an unchecked call is. Where an error genuinely must
be dropped, pass it to a named function that documents the reason (see
`discardError` in `internal/artifact/write.go`) rather than suppressing the
linter.

### 3.3 `nolintlint` requires a specific linter and an explanation

`//nolint` alone is rejected. Write `//nolint:gosec // G101: this is the name of
a variable, not a credential.` The formatter will move a trailing directive if
the line wraps, so put it on its own line above the statement.

### 3.4 `gosec` G101 fires on constants that merely *name* a secret

`const PasswordEnvVar = "DBSURVEYOR_ENCRYPTION_PASSWORD"` is the name of an
environment variable to read, not a credential. It needs a documented
suppression.

## 4. Build and CI

### 4.1 Run `just format` before `just check`

`just check` starts with `format-check`. Skipping the format step turns a
whitespace difference into a failed gate that reads like a lint error.

### 4.2 `-race` is the one place CGO is allowed

The race detector requires cgo, so `just test-race` overrides the
repository-wide `CGO_ENABLED=0`. It is a test-only exception and never applies to
a shipped build.

### 4.3 `govulncheck` may not be on `PATH`

`mise` installs Go tools into the Go toolchain's `bin`, not `$GOPATH/bin`. If
`just vuln` reports "command not found", the binary is likely under
`$(go env GOBIN)`.

### 4.4 rust-analyzer's descendant: trust the compiler over the IDE

Editor diagnostics go stale after multi-file edits and do not always enable the
same build tags. `go build ./...` and `golangci-lint run` are authoritative.

## 5. Schema documents

### 5.1 Published artifacts are generated

`docs/formats/*.schema.json` and the example documents are derived from the Go
types by `just gen-schema`. `TestPublishedArtifactsAreCurrent` fails when a
committed file is stale. Change a schema type, regenerate.

### 5.2 The envelope specification is held to the code by a test

`docs/formats/encrypted-envelope.md` publishes a complete envelope in hex, and
`TestWorkedExampleMatchesTheSpecification` reproduces it from injected salt and
nonce bytes. A layout change that is not also a documentation change is a test
failure, not a review catch.

### 5.3 Model types have `PartialEq` semantics but float fields

Several types carry `float64` quality scores. Compare them with tolerance where
it matters; do not assume exact equality survives a serialization round trip on
every platform.

## 6. Database behavior [carried]

These describe engine quirks, not language mechanics. They cost real time in the
Rust tree and the same traps exist in Go.

### 6.1 MySQL `INFORMATION_SCHEMA` mixes signed and unsigned

`ORDINAL_POSITION` in `COLUMNS` is `INT UNSIGNED`. `NON_UNIQUE` in `STATISTICS`
is signed `INT`. A driver that is strict about integer width will fail to decode
one if you assume the other. Check the actual column type before choosing the Go
type.

### 6.2 Row-count estimates are nullable

MySQL `INFORMATION_SCHEMA.TABLES.TABLE_ROWS` and SQLite `MAX(rowid)` both return
NULL for newly created tables, empty tables, and some storage engines. Scan into
a nullable type and default to zero. PostgreSQL `pg_class.reltuples` carries the
same risk.

### 6.3 SQLite PRAGMA arguments and DML identifiers escape differently

PRAGMA arguments use single-quote escaping (`'` doubled). DML identifiers use
double-quote escaping. These are different quoting contexts in the same engine;
use the per-context helper, not one shared function.

### 6.4 SQLite `PRAGMA table_info` is zero-based

`cid` starts at 0. The schema document requires `ordinal_position` to start at 1.
Convert.

### 6.5 Never build a quoted identifier without escaping the quote character

An identifier containing a double quote closes the quoting early. Escape by
doubling, always, even for identifiers you believe are system-generated.

### 6.6 Multi-database mode exhausts connections easily

Scanning many databases on one server means many pools. Cap each pool small
(2 connections, no idle minimum) and close it explicitly when that database is
done, rather than relying on scope exit.

### 6.7 PostgreSQL batch collection is worth the complexity

Per-table metadata queries make schema collection O(5N+1). Running the five
queries once for *all* tables and grouping in memory makes it O(6). Keep a
per-table fallback for the case where the batch query fails.

### 6.8 Every operation is read-only

No temporary tables, no session variables that persist, no `ANALYZE`. An operator
may be pointed at production with credentials they are not supposed to write
with, and the tool must not be the reason that becomes a problem.
