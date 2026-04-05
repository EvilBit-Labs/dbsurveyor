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

- Rust (see `rust-toolchain.toml` for exact version)
- [just](https://github.com/casey/just) task runner
- [mise](https://mise.jdx.dev/) for tool management (optional but recommended)
- Docker (for integration tests with testcontainers)
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
just fmt             # Format code (run before ci-check)
just lint            # Clippy with strict warnings
just test            # Run test suite
just ci-check        # Full CI validation (~3 min)
just pre-commit      # Run all pre-commit checks
just security-audit  # Security audit and SBOM
just build           # Build release binaries
```

## Before You Start

1. **Read [GOTCHAS.md](GOTCHAS.md).** It documents pitfalls that have caught every contributor at least once -- adapter architecture, validation edge cases, SQLite escaping, CI quirks.

2. **Open an issue first.** For anything beyond a typo fix, open an issue or discussion before writing code. This saves everyone time if the change does not align with project direction.

3. **One issue per PR.** Do not bundle unrelated changes. If a fix requires refactoring, that is a separate PR discussed first.

## Code Standards

### Rust

- **Formatting:** `cargo fmt` -- standard Rust formatting
- **Linting:** `cargo clippy -- -D warnings` -- zero warnings policy, no exceptions
- **Safety:** `unsafe` code is denied at the workspace level
- **File size:** 600 lines preferred max. Break large files into focused modules
- **Error handling:** `Result<T, E>` with `?` operator. No `unwrap()` or `expect()` in production code (enforced by clippy deny)
- **No non-ASCII:** No emoji, checkmarks, or unicode bullets in source code

### Database Operations

- Parameterized queries only -- no string concatenation
- Read-only operations -- no schema modifications
- Connection pooling with configurable limits
- Credentials never logged or included in output

### Commits

Follow [Conventional Commits](https://www.conventionalcommits.org):

```text
feat(collector): add MySQL sampling support
fix(security): cap sample_size to prevent resource exhaustion
refactor(postgres): parallelize schema collection with tokio::join
```

Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` Scopes: `collector`, `processor`, `shared`, `security`, `cli`, `postgres`, `mysql`, `sqlite`, `mongodb`

### DCO Sign-Off

All commits require a Developer Certificate of Origin sign-off:

```bash
git commit -s -m "feat(collector): add MySQL sampling support"
```

The `-s` flag adds the `Signed-off-by` trailer from your git config. This is a legal attestation that you have the right to submit the contribution.

## Testing

- **Unit tests** co-located with code in `#[cfg(test)]` modules
- **Integration tests** in `tests/` using testcontainers for real database instances
- **Security tests** verify encryption, credential handling, offline operation
- Coverage threshold: 55% (target: 80%, being raised incrementally)

Run specific adapter tests:

```bash
cargo nextest run --features postgresql
cargo nextest run --features sqlite
cargo nextest run --all-features  # Everything
```

## Architecture Overview

DBSurveyor is a Rust workspace with three crates:

| Crate                | Purpose                                                      |
| -------------------- | ------------------------------------------------------------ |
| `dbsurveyor-core`    | Shared library: adapters, models, security, quality analysis |
| `dbsurveyor-collect` | CLI binary: collects database schemas                        |
| `dbsurveyor`         | CLI binary: processes collected schemas into documentation   |

### Adapter Pattern

Each database engine has an adapter module (`postgres/`, `mysql/`, `sqlite/`, `mongodb/`) implementing the `DatabaseAdapter` trait. Sub-modules are private with explicit `pub use` re-exports. See GOTCHAS.md section 1 for details.

### Security Guarantees

These are non-negotiable. Every change must maintain:

1. Offline-only operation (no network calls except to target databases)
2. Zero telemetry
3. Credential protection (never in output, zeroized in memory)
4. AES-GCM encryption for data at rest
5. Airgap compatibility

## AI-Assisted Contributions

We accept AI-assisted contributions. See [AI_POLICY.md](AI_POLICY.md) for the full policy. The short version:

- **You own every line you submit.** You must be able to explain it without asking your AI.
- **Disclose your tools.** Note what you used in the PR description.
- **No unreviewed output.** Hallucinated APIs, boilerplate that ignores conventions, or code you clearly did not run gets closed without review.

## Pull Request Process

1. Fork the repository and create a branch from `main`
2. Make your changes, following the standards above
3. Run `just ci-check` and ensure all checks pass
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
- `cargo clippy` warnings
- Hardcoded credentials or secrets
- Database write operations
- Non-ASCII characters in source code
- Work you cannot explain when asked

## Reporting Vulnerabilities

See [SECURITY.md](SECURITY.md) for the vulnerability reporting process. Do not open public issues for security vulnerabilities.

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

## Questions?

Open a discussion on GitHub. We are happy to help you through your first contribution.
