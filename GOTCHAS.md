# Development Gotchas and Pitfalls

Non-obvious behaviors and hard-earned lessons. Referenced from `AGENTS.md` and `CONTRIBUTING.md`. Read it before changing anything in the areas it covers.

Entries marked **[carried]** were learned against the retired Rust implementation and describe the *database*, not the language. They are believed to hold for the Go tree but have not all been re-validated against it yet; the adapter unit that lands each engine is responsible for confirming or correcting its entries (R3).

## 1. Tests that report success over nothing

Two repository-level tests in this tree passed for months while checking an empty set. Both are fixed; the pattern is the lesson.

### 1.1 A skip list that matched the repository root

`tools/ascii_test.go` walks the tree and skips directories by base name. Its skip list named the retired Rust crate directories, one of which was `dbsurveyor` -- which is also the name of the repository root. `filepath.WalkDir` visits the root first, matched it, returned `SkipDir`, and the entire walk ended before reading a file. R18 had never actually been enforced.

The root is now exempt from the name match explicitly. When adding a skip entry, consider that a checkout can be named anything, including the thing you are trying to skip.

### 1.2 A package pattern relative to the test binary

`tools/nocgo_test.go` ran `go list -deps ./...` to assert R13 over the whole dependency graph. A test binary's working directory is its own package directory, so `./...` expanded to `tools` and `tools/genschema` and nothing else. Both checks now name the module path explicitly.

Any test that shells out to `go list`, `go build`, or `git` should use absolute or module-qualified arguments. Relative ones resolve against a directory that is rarely the one you pictured.

### 1.3 A working-tree walk is not a repository

`tools/ascii_test.go` originally enumerated files by walking the filesystem from the module root. A walk sees whatever happens to be on disk -- an editor's scratch file, an agent's notes, a downloaded fixture -- none of which any contributor committed, and R18 is a rule about repository source. The result was a gate that failed on every machine with local scratch in the tree and passed in CI, which is the fastest way to teach people to stop running it.

It now asks `git ls-files` and checks what git is tracking. That also removes the skip list, and with it the 1.1 hazard: there is no directory name left to match the repository root by accident.

The same reasoning applies to any repository-level check. Ask git what the repository contains; do not infer it from what is sitting in the directory.

### 1.4 The general rule

Before trusting a new invariant test, break the invariant on purpose and watch it fail. A test that has never failed is not evidence that the property holds.

## 2. Artifacts and framing

### 2.1 Extension dispatch is asymmetric

Writing appends the extension the requested options imply. Reading dispatches on the *final* extension only, and then, inside a decrypted payload, on the zstd frame magic (`28 B5 2F FD`) rather than on the filename. A compressed-and-encrypted artifact is named `.enc` and its name says nothing about the zstd frame inside it.

The `.zst` path deliberately does **not** sniff: a file named `.zst` that is not a zstd frame is an error, not a JSON file to fall back on.

Full rules in `docs/formats/compression.md`.

### 2.2 Compression happens before encryption

Never the reverse. Ciphertext is indistinguishable from random and does not compress, so encrypting first pays for the compression pass and saves nothing.

### 2.3 The credential scan runs on bytes, before decoding

`json.Unmarshal` silently drops any field the Go types do not declare. A scan that ran on the decoded document would pronounce a file clean while a credential sat in a field this version does not know about. `internal/artifact` scans the JSON bytes first, then decodes, then validates -- and validation scans again, because it is the enforcement point for documents that never touched disk.

### 2.4 `os.Create` and `os.WriteFile` are forbidden outside `internal/artifact`

`forbidigo` refuses them repository-wide, and `tools/architecture_test.go` additionally reserves `os.Rename`, `os.CreateTemp`, and `os.OpenFile` to that package. The rule exists so the atomic-write contract and the credential scan cannot be routed around. If you need to write a file, you need `internal/artifact`.

### 2.5 Unauthenticated input needs a ceiling, not just a floor

Both the Argon2id cost parameters and the zstd decompressed size arrive from a file and are consumed *before* anything is authenticated -- the key must be derived before the GCM tag can be checked, and a plain `.zst` has no tag at all. Each therefore has an upper bound as well as a lower one. The Rust implementation validated only minimums, which left an artifact naming a multi-terabyte memory cost able to crash the process on open.

### 2.6 A file's own size is a ceiling too

2.5 covers the lengths an artifact *declares*. The length it simply *has* needs a bound as well: `os.ReadFile` allocates the whole file before a single magic byte has been looked at, so every declared-length ceiling downstream is reached only after the process has committed to the allocation. `internal/artifact` stats the path and refuses anything past `maxArtifactSize` before reading.

## 3. Linters

### 3.1 `nonamedreturns` versus gocritic `unnamedResult`

Both are enabled. `unnamedResult` wants multiple return values named; `nonamedreturns` forbids naming them. When they collide -- which happens for functions returning two values of the same type -- the resolution is to return a small struct instead of two values. This has come up twice; do not resolve it with a suppression.

### 3.2 `errcheck` has `check-blank: true`

`_ = f()` is flagged just as an unchecked call is. Where an error genuinely must be dropped, pass it to a named function that documents the reason (see `discardError` in `internal/artifact/write.go`) rather than suppressing the linter.

### 3.3 `nolintlint` requires a specific linter and an explanation

`//nolint` alone is rejected. Write `//nolint:gosec // G101: this is the name of a variable, not a credential.` The formatter will move a trailing directive if the line wraps, so put it on its own line above the statement.

### 3.4 `gosec` G101 fires on constants that merely *name* a secret

`const PasswordEnvVar = "DBSURVEYOR_ENCRYPTION_PASSWORD"` is the name of an environment variable to read, not a credential. It needs a documented suppression.

## 4. Build and CI

### 4.1 Run `just format` before `just check`

`just check` starts with `format-check`. Skipping the format step turns a whitespace difference into a failed gate that reads like a lint error.

### 4.2 `-race` is the one place CGO is allowed

The race detector requires cgo, so `just test-race` overrides the repository-wide `CGO_ENABLED=0`. It is a test-only exception and never applies to a shipped build.

### 4.3 A bare tool name in the justfile runs whatever is on `PATH`

Every recipe invokes its tool through `mise exec --`, the Go toolchain included. That is not decoration. `mise` puts each pinned tool in its own directory under `~/.local/share/mise/installs`, and `$GOPATH/bin` sits ahead of those on `PATH`, so a bare `govulncheck` in a recipe resolves to whatever a past `go install ...@latest` left in `$GOPATH/bin` -- unpinned, never refreshed, and carrying the Go toolchain of the day it was installed.

This is how it actually failed: `just dev-setup` ran `mise install` and then `go install golang.org/x/vuln/cmd/govulncheck@latest`, so the pinned copy was installed and immediately shadowed. Five months later `just vuln` died with "Loading packages failed, possibly due to a mismatch between the Go version used to build govulncheck and the Go version on PATH" -- a v1.1.4 binary built with go1.25.7 being asked to parse a go1.26 tree, while the mise-pinned v1.6.0 built with go1.26.5 sat unused. CI was unaffected, because a fresh runner has no `$GOPATH/bin` to shadow anything, which is what let it rot for so long.

`dev-setup` is now `mise install` alone. If a tool is needed, pin it in `mise.toml`; do not `go install` it.

### 4.4 rust-analyzer's descendant: trust the compiler over the IDE

Editor diagnostics go stale after multi-file edits and do not always enable the same build tags. `go build ./...` and `golangci-lint run` are authoritative.

### 4.5 `go.mod` states a language level, not a toolchain

`actions/setup-go` with `go-version-file: go.mod` reads the `go` directive and installs *that* version, then sets `GOTOOLCHAIN=local` so nothing upgrades it later. The `go` directive is a minimum language level, though, and this repository pins its actual toolchain in `mise.toml` -- `go.mod` says 1.26.1 while `mise.toml` says 1.26.5.

That gap is invisible until something reads the standard library's *patch* version. `govulncheck` does. `audit.yml` was the last workflow still using `setup-go`, and it reported nine stdlib advisories -- fixed across 1.26.2 through 1.26.5 -- against a tree that had none, while the identical `govulncheck ./...` in `security.yml` passed throughout because that workflow installs its toolchain through mise.

Two things made it survive. The failure looked like real vulnerability findings rather than a configuration error. And the check names hid it: the failing job is called `govulncheck`, while the check that reads `audit` in the flat PR list comes from `security.yml`, an unrelated workflow, and was green. Anyone scanning for "did the dependency audit pass" found a passing `audit` and stopped.

Install the toolchain with `jdx/mise-action` in every workflow. `tools/workflow_test.go` now enforces that, because documentation did not: `ci.yml` already carried this warning in a comment while `audit.yml` sat next to it doing the thing the comment warned against.

This is a different failure from 4.3, despite both ending in "the pinned version is not the one that ran". 4.3 is a PATH race between two binaries on one machine. This is one tool reading a different file than the other -- no race, nothing unpinned, and `setup-go` behaving exactly as configured. A fix for either does nothing for the other.

## 5. Schema documents

### 5.1 Published artifacts are generated

`docs/formats/*.schema.json` and the example documents are derived from the Go types by `just gen-schema`. `TestPublishedArtifactsAreCurrent` fails when a committed file is stale. Change a schema type, regenerate.

### 5.2 The envelope specification is held to the code by a test

`docs/formats/encrypted-envelope.md` publishes a complete envelope in hex, and `TestWorkedExampleMatchesTheSpecification` reproduces it from injected salt and nonce bytes. A layout change that is not also a documentation change is a test failure, not a review catch.

### 5.3 Model types have `PartialEq` semantics but float fields

Several types carry `float64` quality scores. Compare them with tolerance where it matters; do not assume exact equality survives a serialization round trip on every platform.

## 6. Database behavior

Engine quirks, not language mechanics. Every entry below was re-validated against the Go adapters in U8; where the carried entry was wrong, the correction is stated rather than the original left standing (R3).

### 6.1 MySQL `INFORMATION_SCHEMA` mixes signed and unsigned [confirmed]

`ORDINAL_POSITION` in `COLUMNS` is `INT UNSIGNED`. `NON_UNIQUE` in `STATISTICS` is signed `INT`. Under the binary protocol a prepared statement uses, the driver reports each with its declared signedness, so scanning one into the other's Go type is a decode error rather than a silent conversion. Check the actual column type before choosing the Go type.

A second, separate MySQL signedness trap: a column's own signedness appears only in `COLUMN_TYPE`. `DATA_TYPE` reports `int` for a signed and an unsigned column alike, so a mapping that reads only `DATA_TYPE` silently reports every unsigned column as signed.

### 6.2 Row-count estimates are absent in three different ways [corrected]

The carried entry said all three engines return NULL. Two do and one does not:

- MySQL `INFORMATION_SCHEMA.TABLES.TABLE_ROWS` is **NULL** for a table the storage engine has no statistics for.
- SQLite `MAX(rowid)` is **NULL** for an empty table, and **errors** on a `WITHOUT ROWID` table, which has no rowid to take a maximum of.
- PostgreSQL `pg_class.reltuples` is **-1**, not NULL, on a table that has never been analyzed. A nullable scan does not catch it; a `> 0` guard does.

All three are reported as an estimate of zero. Scanning any of them into a plain unsigned integer fails or wraps.

### 6.3 SQLite: bind the PRAGMA argument instead of escaping it [corrected]

The carried entry said PRAGMA arguments use single-quote escaping while DML identifiers use double-quote escaping, and to keep a helper per context. The better answer is to remove the second context: every PRAGMA this tool needs has a `pragma_*` table-valued function, and those take the table name as a **bound parameter**.

`internal/sqlite` therefore has one quoting helper, for DML, and no single-quote escaping anywhere. The same name needing two escapings in one file is how the wrong one gets used.

### 6.4 Derive the ordinal position by counting, not by converting [corrected]

The carried entry said SQLite `PRAGMA table_info` reports a zero-based `cid` and to convert it. True, but converting is the wrong fix, because PostgreSQL has the opposite problem: `pg_attribute.attnum` is already one-based and has **gaps**, since a dropped column's attribute number is never reused. A converted position reproduces the gap, and the document requires consecutive positions.

Both adapters order rows by the engine's own column order and number them by counting. That is correct for a zero-based source and for a gapped one.

### 6.5 Never build a quoted identifier without escaping the quote character

An identifier containing the quote character closes the quoting early. Escape by doubling, always, even for identifiers you believe the catalog generated.

The character differs per engine -- backtick on MySQL, double quote on PostgreSQL and SQLite -- which is why each adapter carries its own helper. A shared helper would need the engine as an argument, and the argument is what gets passed wrong.

### 6.6 Multi-database mode exhausts connections easily [confirmed]

Scanning many databases on one server means many pools. Cap each pool small (2 connections, no idle minimum) and close it explicitly when that database is done, rather than relying on scope exit.

### 6.7 PostgreSQL batch collection is worth the complexity [confirmed]

Per-table metadata queries make schema collection O(5N+1). Running the five queries once for *all* tables and grouping in memory makes it O(6).

Two things make the fallback trustworthy rather than merely present:

- Each query is written **once**, as a template with a placeholder where its table predicate goes, and completed with either `n.nspname = ANY($1)` or `n.nspname = $1 AND c.relname = $2`. The select list, joins, and ordering are literally the same text in both paths, so the fallback cannot drift into reporting something subtly different.
- Only an **error** triggers the fallback. An empty result is an answer -- a database with no foreign keys anywhere -- and treating it as a failure would send every simple schema down the slow path.

### 6.8 Every operation is read-only, and each engine has a different lever

No temporary tables, no session variables that persist, no `ANALYZE`. An operator may be pointed at production with credentials they are not supposed to write with, and the tool must not be the reason that becomes a problem.

Beyond issuing only reads, each adapter asks the engine to enforce it:

| Engine     | Lever                                                     |
| ---------- | --------------------------------------------------------- |
| PostgreSQL | `default_transaction_read_only=on` as a startup parameter |
| MySQL      | `transaction_read_only=1` as a session system variable    |
| SQLite     | `PRAGMA query_only(1)` on every connection                |

PostgreSQL's is a startup parameter rather than a `SET` on purpose: it applies to every connection the pool opens later to grow itself, which a one-off `SET` would miss.

### 6.9 SQLite read-only mode does not stop the file being created

`PRAGMA query_only` refuses writes to a database's *contents*. It does not stop the driver creating a database file that was not there, so a mistyped path produces a valid, empty, entirely fictional schema document rather than an error. `internal/sqlite` stats the path before opening it.

### 6.10 An empty catalog result is not a missing table

`PRAGMA table_info` on a table that does not exist returns **zero rows**, not an error, and `INFORMATION_SCHEMA.COLUMNS` behaves the same way for a table the credential cannot see. A sampler that read that as success would go on to select from a table that is not there. Each adapter turns the empty result back into an error.

### 6.11 SQL Server escapes the closing bracket, and only that one

SQL Server brackets identifiers. Only `]` can end the quoting, so it is the only character that needs doubling -- an embedded `[` is harmless. A helper written by analogy with the double-quote engines escapes the wrong character and leaves the right one able to close the quoting early.

### 6.12 SQL Server reports a character length in bytes

`sys.columns.max_length` is a byte count, so an `NVARCHAR(50)` reports 100. Passing it through verbatim says the column holds twice what it does. A value of -1 marks the MAX types, which have no declared limit at all.

### 6.13 Oracle folds unquoted identifiers *up*

Every other engine here either folds down (PostgreSQL) or preserves case (MySQL, SQL Server). Oracle folds up, so a table created as `orders` is stored as `ORDERS` and a lookup by the name an operator typed finds nothing. Names reaching the catalog queries are normalized first, and a name that already carries an upper-case letter is left alone -- it is either the stored form already or was created quoted, and folding it would break the second case.

### 6.14 Oracle's NUMBER is three types wearing one name

`NUMBER(p,0)` is an integer, `NUMBER(p,s)` with a scale is a fixed-point decimal, and a bare `NUMBER` is a floating value of unspecified precision. Precision and scale are both NULL for the bare form, so defaulting either collapses the distinction. The width an integer maps to comes from the decimal precision: `NUMBER(9,0)` fits in 32 bits and `NUMBER(10,0)` does not.

Two further Oracle facts the document has to accommodate: a `DATE` carries a time component, unlike every other engine's `DATE`; and there is no `ON UPDATE` for a foreign key at all, so a nil update action is accurate rather than incomplete.

### 6.15 Oracle records every NOT NULL as a check constraint

`ALL_CONSTRAINTS` reports a NOT NULL declaration as a check whose condition is `"COL" IS NOT NULL`. The column's own nullability already carries that, so recording it again fills the document with one constraint per non-nullable column and buries the checks an operator actually wrote. They are filtered out.

### 6.16 A MongoDB schema is a claim about the sample, not about the collection

There is no declaration to read, so every field, type, and nullability in a MongoDB document comes from a sample. A field that happens to be present in every sampled document is reported as non-nullable, which is a statement about the sample. The type frequency is the substance of the result -- a field that is a string in most documents and a number in the rest is a data-quality finding -- and since the schema document has nowhere structured to put it, it goes in the column comment rather than being discarded.

`$sample` is used rather than a `find` with a limit: a find returns documents in storage order, and the first N documents of a long-lived collection are the oldest, which is the sample least likely to show the fields an application added recently.

### 6.17 A view's columns are resolved, not declared

SQLite reports a view's columns by planning its SELECT, so `PRAGMA table_info` on a view whose underlying table was dropped returns an error -- `no such table` -- while `sqlite_master` still lists the view. This is not the empty-result case of 6.10; it is a genuine driver error, and the two need different handling.

`collectViews` records it as a warning and keeps the view without columns, the same demotion `collectTable` applies to a missing row estimate. Aborting meant one dangling view denied the operator the entire schema.

### 6.18 Oracle's generated NOT NULL check is the whole condition, not a suffix

6.15 filters out the check constraints Oracle generates for NOT NULL. The filter has to match the *whole* condition. An operator-written check can legitimately end in the same three words -- `status = 'X' AND notes IS NOT NULL` -- and a suffix test silently discards it, which is the opposite of the problem 6.15 was solving.

### 6.19 Which Go type a BSON subdocument arrives as is the driver's choice

A nested document decodes as `bson.M`, `bson.D`, or a plain `map[string]any` depending on the registry in play, and the caller does not get to pick. Every type switch that handles one of them has to handle all three, and there are three such switches in `internal/mongodb`: field inference, the type mapper, and sample value normalization. Fixing only the one that was reported leaves a subdocument typed as `unknown` while inference walks its fields, or a nested ObjectID handed to `encoding/json` raw.

The shared helper is `asDocument`. Route through it rather than adding a fourth type switch.

### 6.20 Three engines have no server-side read-only switch

The lever table in 6.8 covers PostgreSQL, MySQL, and SQLite. The other three have nothing equivalent:

| Engine     | Nearest lever                | What it actually does                                                         |
| ---------- | ---------------------------- | ----------------------------------------------------------------------------- |
| SQL Server | `ApplicationIntent=ReadOnly` | Routes to a readable secondary in an availability group; no effect standalone |
| Oracle     | none                         | Read-only transactions are per-transaction, not a session mode                |
| MongoDB    | secondary read preference    | Reaches a member that rejects writes; no effect standalone                    |

On a standalone server the read-only guarantee for these three rests entirely on the adapter issuing only reads. Their integration suites therefore assert it against the data -- the table list and row counts before and after a full survey -- rather than by expecting the server to reject a write.

## 7. Orchestration and the command layer

### 7.1 An adapter is reached through a map, never an import

`internal/survey` orchestrates a collection through a `Registry` the command layer hands it, and `cmd/dbsurveyor-collect/wire.go` is the only file in the tree that imports an adapter package. Two tests in `tools/` enforce both halves: one over the transitive dependency graph, one over the import blocks under `internal/`.

The payoff is concrete rather than architectural. The survey is testable with a fake in the map instead of six databases, and a binary that orchestrates nothing does not link six drivers.

### 7.2 `os.Exit` skips every pending defer in its own function

`main` installs a signal handler with `defer stop()`, so the exit has to happen in a caller: `os.Exit(runCommand())`, with the work and the defer inside `runCommand`. gocritic's `exitAfterDefer` catches the mistake, which is worth knowing about before it looks like a false positive.

### 7.3 A connection string is a credential from the moment it is read

`survey.ParseTarget` moves the password into a `Secret` and the raw string is never stored -- not on the options struct, not in a log line, and not in an error. The parse errors are deliberately terse and do **not** wrap `net/url`'s: that error quotes the URL it failed on, which for a connection string is the password.

## 8. Credentials at the driver boundary

Every driver in use takes its password as a `string` field, so the conversion `Secret` exists to prevent has to happen exactly once per engine. `dbadapter.Secret.RevealString` is that exit, and `TestNoSecretIsConvertedToString` in `tools/` reserves both it and the equivalent hand-rolled `string(secret.Reveal())` to the `connect.go` of an adapter package.

This is the same shape as the reservation of the atomic-write primitives to `internal/artifact` (2.4): a path exception a reviewer can see, rather than a lint suppression a future caller can copy. `internal/dbadapter` itself is not exempt, or the reservation would exempt the thing being reserved.

### 8.1 Zeroing has exactly one correct place, and it is not connect.go

R17 asks that a credential be zeroed best effort after connection setup, and the obvious reading -- zero it where the driver takes it -- is wrong here. `NewSecret` deliberately does not copy the caller's bytes, so every copy of a `ConnectionConfig` shares one backing array, and a multi-database run hands that same credential to a fresh pool per database. Zeroing at the handoff erases the credential out from under every database after the first.

`survey.Run` zeroes it once, immediately after `collect` returns, which is the point past which no further connection is opened. Both the success and the failure path go through it.
