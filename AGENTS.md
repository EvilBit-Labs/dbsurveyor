# dbsurveyor - AI Agent Instructions

## Project Overview

**Description**: Toolchain for surveying database servers, extracting schema and sample data, and generating portable structured output.

**Architecture pattern**: Modular monolith -- organized into packages, shipped as two binaries.

**Visibility**: Public repository.

**Development OS**: WSL, macOS. Windows is a supported build target, not the primary development environment.

**Repository**: GitHub, `EvilBit-Labs/dbsurveyor`.

**Reference repository**: <https://github.com/EvilBit-Labs/opnDossier>. When a tooling or layout question has no answer here, read opnDossier's live configuration rather than picking a default.

> This tree is a Go rewrite in progress. The Rust implementation it replaces was removed from the default branch and is preserved in full on the `rust-final` branch. Nothing here is obliged to read a Rust-era artifact.

@GOTCHAS.md

## Technology Stack

| Layer            | Choice                                                                                   |
| ---------------- | ---------------------------------------------------------------------------------------- |
| Language         | Go 1.26, module `github.com/EvilBit-Labs/dbsurveyor` at the repository root              |
| CLI              | `spf13/cobra` wrapped by `charmbracelet/fang`                                            |
| Logging          | `charmbracelet/log`                                                                      |
| Terminal styling | `charmbracelet/lipgloss`; `charmbracelet/glamour` for rendered Markdown                  |
| Testing          | `stretchr/testify`; `sebdah/goldie/v2` golden files; `testcontainers-go` for adapters    |
| Database drivers | `pgx/v5`, `go-sql-driver/mysql`, `modernc.org/sqlite`, `mongo-driver/v2`,                |
|                  | `microsoft/go-mssqldb`, `sijms/go-ora` -- all pure Go                                    |
| Compression      | `klauspost/compress/zstd`                                                                |
| Cryptography     | `crypto/aes` and `crypto/cipher` from the standard library, `golang.org/x/crypto/argon2` |
| Task runner      | `just`                                                                                   |
| Lint and format  | `golangci-lint` v2 (it owns the formatters: gofumpt, goimports, gci, golines)            |
| Release          | GoReleaser v2, native Go builder                                                         |
| CI               | GitHub Actions                                                                           |

Every dependency must be pure Go. See R13.

### Databases

In scope, one adapter package each: PostgreSQL, MySQL, SQLite, MongoDB, MSSQL, Oracle.

Cassandra, ClickHouse, and CockroachDB are of interest but are **not** in scope for the current rewrite. CockroachDB speaks the PostgreSQL wire protocol, so the PostgreSQL adapter may work against it; that is untested and unclaimed.

### Choosing a technology not listed

Analyze the codebase and propose one. Check the actual documentation -- via Context7 or the project's own docs -- before assuming anything about an API or a library version. Do not assert a library's behavior from memory.

## Requirements

The rewrite plan numbers its requirements R1-R18 and the code refers to them by number. The ones that constrain day-to-day work:

**Layout**

- **R1.** The Go module is the repository root.
- **R5.** Application packages live under `internal/`; only `cmd/` sits outside it, so no package becomes an API the project owes compatibility to.
- **R7.** No package is named for a container concept. `models`, `util`, `common`, and `output` are disallowed as package names. A package name names a concept.
- **R8.** Atomic writes, compression, encryption, and output-extension dispatch live in a package separate from the schema core.
- **R9.** `cmd/` packages hold flag parsing and wiring only. Orchestration lives under `internal/`.

**Adapters**

- **R10.** The adapter interface and its parameter types live in a leaf package that adapter implementations import.
- **R11.** Concrete adapters are constructed by explicit wiring at the command layer. No `init()`-based registration, no global registry.
- **R13.** No CGO anywhere in the dependency graph. `CGO_ENABLED=0` in build and CI, and no CGO-requiring dependency may enter `go.sum`. This is why `mattn/go-sqlite3` is rejected outright: pure-Go drivers are what let an operator drop a single binary onto an airgapped host with no vendor client libraries present.

**Formats and security**

- **R15.** On-disk formats are specified fresh under `docs/formats/`, with the Go implementation as the reference.
- **R16.** Offline-only operation, no telemetry, read-only database access, recursive credential scanning on every load path, atomic writes, airgap compatibility.
- **R17.** Credentials are held as `[]byte` rather than `string`, zeroed best effort after connection setup, and never passed through `fmt`, logging, or error paths. Documentation states plainly that deterministic zeroization is **not** claimed -- the garbage collector may have copied the bytes and the runtime offers no way to find those copies.
- **R18.** Source files are ASCII-only. `asciicheck` covers identifiers; `tools/ascii_test.go` covers whole `.go` and `.md` files byte by byte, which is what catches a curly quote in a doc comment or a checkmark in a table.

## Layout

```text
go.mod                     module github.com/EvilBit-Labs/dbsurveyor
justfile                   task runner
CONCEPTS.md                shared domain vocabulary (survey, artifact, sample,
                           redaction mode) with project-specific meaning
mise.toml                  toolchain pinning
.golangci.yml              strict v2 linter set
.goreleaser.yaml           release configuration
cmd/
  dbsurveyor-collect/      collector CLI: flags, adapter wiring
  dbsurveyor/              postprocessor CLI: flags, wiring
internal/
  dbschema/                schema types, quality, redaction, validation,
                           credential scanning
  dbadapter/               adapter interface and parameter types (leaf)
  postgres/ mysql/ sqlite/ relational adapters (pgx/v5, go-sql-driver, modernc)
  mongodb/ mssql/ oracle/  document and enterprise adapters (go-ora needs no Instant Client)
  envelope/                AES-256-GCM + Argon2id byte format
  artifact/                atomic write, zstd, extension dispatch, load
  survey/                  collection orchestration; imports no adapter (R11)
  report/                  Markdown report orchestration
  progress/                TERM=dumb-aware progress reporting
docs/
  formats/                 on-disk format specifications
  adr/                     architecture decision records
  solutions/               documented solutions to past problems (bugs, best
                           practices, workflow patterns), by category with YAML
                           frontmatter (module, tags, problem_type)
tools/                     repository-level architecture tests
```

Directories not listed as present have not landed yet; the tree is a scope declaration, not an inventory.

## Commands

`just` recipes are grouped; `just --list` shows all of them and `just group <name>` shows one group. Every recipe runs its tool through `mise exec`, so the pinned version is the one that executes -- see GOTCHAS 4.3 for what happened when they did not.

```bash
just build            # both binaries into ./dist
just collect ARGS     # go run the collector
just postprocess ARGS # go run the postprocessor
just test             # go test ./...
just test-integration # container-backed adapter tests (needs Docker)
just test-race        # race detector (the one place CGO_ENABLED=1 is allowed)
just coverage         # coverage report
just coverage-ci      # fail below the coverage threshold
just lint             # golangci-lint run
just lint-yaml        # yamllint --strict over every tracked YAML file
just format           # golangci-lint fmt -- run this BEFORE just check
just check            # format-check, lint, lint-yaml, test, vuln
just ci-check         # MANDATORY COMMIT GATE: check plus test-race
just ci-smoke         # fast build and short tests
just ci-full          # everything CI gates, container suites included
just pre-commit       # every pre-commit hook over the whole tree
just gen-schema       # regenerate docs/formats artifacts from the Go types
just docs-build       # build the mdBook site into docs/book
just docs-serve       # serve the mdBook site with live reload
just changelog        # regenerate CHANGELOG.md with git-cliff
just release-check    # validate .goreleaser.yaml
just release-snapshot # local release dry run, publishes nothing
just install          # install the pinned toolchain and pre-commit hooks
just update-deps      # update toolchain, Go modules, and hooks
```

### The commit gate

**`just ci-check` is mandatory before every commit.** It is `format-check lint lint-yaml test test-race vuln`. Run `just format` first; a formatting-only failure is otherwise indistinguishable from a lint failure in the log.

`just check` is the everyday loop and is not sufficient on its own: it omits `test-race`, and CI does not run the race detector either. `ci-check` is the only gate in this project that looks for a data race, so skipping it is the one way a race reaches `main` with nothing having tried to catch it.

Neither gate runs the container suites. Run `just test-integration` when a change touches an adapter, or `just ci-full` for everything CI gates.

## Working Agreements

### Communication

Be concise and direct. The maintainer is a senior fullstack developer -- do not explain what a mutex is. State the decision and the reason for it.

### Before changing anything

Read `README.md` and `CONTRIBUTING.md`. Read `GOTCHAS.md`, included above; it records behaviors that have already cost time.

### While changing it

- Match the surrounding style. This codebase favors flat, explicit control flow and few abstractions, and comments that explain *why* rather than restate the code.
- Handle errors. Wrap with context; do not discard. Where an error genuinely must be dropped, say so in a comment and make the drop visible rather than assigning to the blank identifier.
- Confirm before a significant change. Do not commit on the maintainer's behalf without being asked to.

### Before returning

Run `just format`, then `just ci-check`. It is the mandatory commit gate and it is not optional because the change looks small. Confirm the change does what it claims. When a check fails, say so and show the output; do not report partial work as done.

A test that passes is not evidence until you have checked it can fail. Two repository-level tests in this tree reported success over an empty set for months -- see `GOTCHAS.md` section 1.

### Commits

Conventional Commits, with a scope naming the package: `feat(artifact):`, `fix(postgres):`, `docs(formats):`, `chore(deps):`. Breaking changes take `!` in the header or `BREAKING CHANGE:` in the footer.

Versioning is semver.

### When you learn something

A non-obvious behavior that cost time belongs in `GOTCHAS.md`. A convention that should bind future work belongs here. Propose the edit.

## Security

Do not commit secrets. Use environment variables or a secret manager. The credential scan in `internal/dbschema` will fail a document that carries one, and that failure is not to be suppressed.

The guarantees this tool makes to an operator:

- **Offline only.** No network calls except to the target database. No telemetry, no external reporting, no update checks.
- **Read only.** Every database operation is a read. No schema modification, no DML, no temporary objects.
- **Credentials never reach output.** Not in an artifact, not in a log line, not in an error message. Every load path terminates in the recursive scan.
- **Airgap compatible.** Full functionality with no internet access.

Parameterized queries only. Identifiers that must be interpolated are escaped by the per-engine helper, never by string concatenation.

When a change touches authentication, credential handling, an output path, or a dependency, offer a security review of the affected code before moving on.

## References

- `docs/formats/schema-document.md` -- the JSON schema document
- `docs/formats/encrypted-envelope.md` -- the AES-256-GCM byte layout
- `docs/formats/compression.md` -- zstd framing, extension dispatch, atomic write
- `docs/adr/` -- architecture decision records
