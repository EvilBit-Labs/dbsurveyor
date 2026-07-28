# Contributing to DBSurveyor

Thank you for your interest in contributing. DBSurveyor is a database schema documentation and analysis tool built for security professionals and database administrators. We welcome contributions that align with the project's goals.

This guide covers what you need to know as a human contributor. For AI coding assistant rules, see [AGENTS.md](AGENTS.md). For non-obvious pitfalls and hard-earned lessons, see [GOTCHAS.md](GOTCHAS.md).

## Core Philosophy

- **Security first.** No network calls except to target databases. No telemetry. Credentials never appear in output.
- **Operator-centric.** Built for people who run infrastructure, not people who demo it. Offline-first, airgap-compatible.
- **Read-only.** All database operations are strictly read-only. We collect metadata, never modify it.
- **Polish over scale.** Quality over feature count. Sane defaults. CLI help that is actually helpful.

## Getting Started

### Prerequisites

- Go 1.26 or later (see `mise.toml` for the pinned version)
- [just](https://github.com/casey/just) task runner
- [mise](https://mise.jdx.dev/) for tool management (optional but recommended)
- [golangci-lint](https://golangci-lint.run/) v2 -- it owns the formatters too
- Docker or another Testcontainers-compatible runtime, for the integration tests
- [pre-commit](https://pre-commit.com/) for git hooks

### Setup

```bash
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor
just dev-setup       # Install tools and dependencies
pre-commit install   # Set up git hooks
```

### Common Commands

```bash
just format           # Format. Run this BEFORE just check
just lint             # golangci-lint run
just test             # go test ./...
just test-integration # container-backed adapter tests (needs Docker)
just test-race        # race detector; the one place CGO_ENABLED=1 is allowed
just vuln             # govulncheck over the dependency graph
just check            # format-check, lint, test, vuln -- the gate
just build            # both binaries into ./dist
```

**Run `just format` before `just check`.** The gate starts with a format check,
so skipping the format step turns a whitespace difference into a failed gate that
reads like a lint error.

## Before You Start

1. **Read [GOTCHAS.md](GOTCHAS.md).** It records behaviors that have already
   cost somebody time: tests that reported success over nothing, the asymmetry
   between how artifacts are written and read, linter rules that contradict each
   other, and a per-engine list of database quirks that were re-validated against
   the Go adapters rather than assumed to carry over.

2. **Open an issue first.** For anything beyond a typo fix, open an issue or discussion before writing code. This saves everyone time if the change does not align with project direction.

3. **One issue per PR.** Do not bundle unrelated changes. If a fix requires refactoring, that is a separate PR discussed first.

## Code Standards

### Go

- **Formatting:** `golangci-lint fmt` -- it owns gofumpt, goimports, gci, and
  golines, so there is one formatter rather than four to keep in agreement.
- **Linting:** `golangci-lint run` -- zero issues, no exceptions. A suppression
  needs a specific linter and a written reason.
- **No cgo, anywhere.** `CGO_ENABLED=0` in build and CI, and a repository test
  fails on any cgo dependency entering the graph. This is what makes a single
  binary work on an airgapped host, and it is why `mattn/go-sqlite3` is rejected
  outright in favor of `modernc.org/sqlite`.
- **Error handling:** wrap with context, do not discard. Where an error genuinely
  must be dropped, pass it to a named function that documents the reason rather
  than assigning to the blank identifier -- `errcheck` runs with
  `check-blank: true`, so a blank assignment is flagged like an unchecked call.
- **ASCII only.** No emoji, no curly quotes, no unicode bullets, in source *or*
  in Markdown. `tools/ascii_test.go` checks whole files byte by byte, which is
  what catches a smart quote a copy-paste introduced into a doc comment.
- **Comments explain why.** The codebase favors flat, explicit control flow and
  few abstractions. A comment that restates the code is noise; a comment naming
  the trap the code avoids is the reason the next person does not reintroduce
  it.

### Database Operations

- Parameterized queries only -- no string concatenation
- Read-only operations -- no schema modifications
- Connection pooling with configurable limits
- Credentials never logged or included in output

### Commits

Follow [Conventional Commits](https://www.conventionalcommits.org):

```text
feat(postgres): batch the per-table metadata queries
fix(envelope): bound the Argon2id cost parameters from above as well as below
docs(formats): publish a worked envelope example a test reproduces
```

Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`.

Scopes name a package: `dbschema`, `dbadapter`, `artifact`, `envelope`,
`survey`, `report`, `progress`, `postgres`, `mysql`, `sqlite`, `mongodb`,
`mssql`, `oracle`, `adapters`, `formats`, `deps`.

A breaking change takes `!` in the header or `BREAKING CHANGE:` in the footer.
Versioning is semver.

### DCO Sign-Off

All commits require a Developer Certificate of Origin sign-off:

```bash
git commit -s -m "feat(mysql): collect index sort direction"
```

The `-s` flag adds the `Signed-off-by` trailer from your git config. This is a legal attestation that you have the right to submit the contribution.

## Testing

- **Unit tests** live in the package they test, so they can exercise unexported
  functions. `go test ./...` needs no container runtime.
- **Integration tests** drive real database servers through
  [Testcontainers](https://golang.testcontainers.org/) and sit behind the
  `integration` build tag. They are linted and type-checked anyway: an untagged
  file is a file nothing compiles, and a stale integration test is discovered at
  the worst possible moment.
- **Golden files** use `sebdah/goldie/v2`. They are Go-authored -- they record
  what this implementation produces so a change is a visible diff. They are not a
  compatibility corpus, and nothing here claims byte compatibility with the
  retired Rust implementation.

```bash
go test ./...                              # no containers needed
go test -tags integration ./...            # needs Docker
go test -tags integration ./internal/postgres/
go test ./internal/report/ -update         # regenerate golden files
```

### A test that has never failed is not evidence

Before trusting a new invariant test, break the invariant on purpose and watch it
fail. This is not a stylistic preference: two repository-level tests in this tree
reported success over an empty set for months. One had a skip list that matched
the repository root, so the walk ended before reading a file; the other used a
package pattern that resolved against the test binary's own directory. Both are
recorded in [GOTCHAS.md](GOTCHAS.md) section 1.

## Architecture

One Go module at the repository root. Application packages live under
`internal/`; only `cmd/` sits outside it, so no package becomes an API this
project owes compatibility to.

| Package                  | Purpose                                                     |
| ------------------------ | ----------------------------------------------------------- |
| `internal/dbschema`      | The schema document, plus quality, redaction, validation, and credential scanning |
| `internal/dbadapter`     | The adapter contract and its parameter types. A leaf        |
| `internal/postgres` etc. | One package per engine, each importing `dbadapter`          |
| `internal/envelope`      | AES-256-GCM and Argon2id, as a byte format                  |
| `internal/artifact`      | Atomic write, zstd framing, extension dispatch, and load    |
| `internal/survey`        | Collection orchestration. Imports **no** adapter            |
| `internal/report`        | Markdown rendering                                          |
| `internal/progress`      | Progress reporting that knows when to say nothing           |
| `cmd/`                   | Flag parsing and wiring only                                |

### Adapters are wired, not registered

`internal/survey` reaches an engine through a map it is handed, and
`cmd/dbsurveyor-collect/wire.go` is the only file in the tree that imports an
adapter package. Two repository tests enforce it, one over the transitive
dependency graph and one over the import blocks.

This is not architectural taste. It means the survey is testable with a fake in
the map rather than six databases, and a binary that orchestrates nothing does
not link six drivers.

### Some rules are enforced by tests rather than by review

Four properties are checked mechanically, because a reviewer will eventually miss
one:

- No cgo in the dependency graph.
- `os.Create` and `os.WriteFile` are refused repository-wide by `forbidigo`, and
  `os.Rename`, `os.CreateTemp`, and `os.OpenFile` are reserved to
  `internal/artifact` -- so the atomic-write contract and the credential scan
  cannot be routed around.
- No conversion of a revealed credential to a `string`, outside the one file per
  adapter where a driver has to be handed one.
- Source and Markdown files are ASCII, byte by byte.

### Security guarantees

Non-negotiable. Every change must maintain offline-only operation, zero
telemetry, read-only database access, credentials absent from every output, and
airgap compatibility. See [SECURITY.md](SECURITY.md) for what each of those means
in practice -- and for what is deliberately *not* claimed.

When a change touches authentication, credential handling, an output path, or a
dependency, say so in the PR description.

## AI-Assisted Contributions

We accept AI-assisted contributions. See [AI_POLICY.md](AI_POLICY.md) for the full policy. The short version:

- **You own every line you submit.** You must be able to explain it without asking your AI.
- **Disclose your tools.** Note what you used in the PR description.
- **No unreviewed output.** Hallucinated APIs, boilerplate that ignores conventions, or code you clearly did not run gets closed without review.

## Pull Request Process

1. Fork the repository and create a branch from `main`
2. Make your changes, following the standards above
3. Run `just format`, then `just check`, and ensure everything passes
4. Commit with conventional commit messages and DCO sign-off
5. Open a PR with a clear description of what changed and why
6. Wait for review -- this is a single-maintainer project, so please be patient

### PR Description

Include:

- Summary of changes (what and why)
- Test plan (how you verified the changes work)
- AI tooling disclosure (if applicable)

### What Gets Your PR Closed

- Bundled unrelated changes
- Missing tests for new functionality
- Any `golangci-lint` issue
- A `//nolint` without a specific linter and a written reason
- Hardcoded credentials or secrets
- A database write operation of any kind
- A new cgo dependency
- Non-ASCII characters in source or Markdown
- Work you cannot explain when asked

## Reporting Vulnerabilities

See [SECURITY.md](SECURITY.md) for the vulnerability reporting process. Do not open public issues for security vulnerabilities.

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

## Questions?

Open a discussion on GitHub. We are happy to help you through your first contribution.
