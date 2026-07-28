# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - Unreleased

The first published release. Nothing was released before it, so the entries below
describe the whole tool rather than a set of changes to one.

DBSurveyor was previously written in Rust and never shipped a release. That
implementation is preserved on the
[`rust-final`](https://github.com/EvilBit-Labs/dbsurveyor/tree/rust-final)
branch. This one is a clean-slate Go rewrite with the formats specified fresh,
and it reads no Rust-era artifact. See
[ADR 0002](docs/adr/0002-go-clean-slate-rewrite.md).

### Added

#### Database support

- Six engines, each with a pure-Go driver and no client library to install:
  PostgreSQL (`pgx/v5`), MySQL and MariaDB (`go-sql-driver/mysql`), SQLite
  (`modernc.org/sqlite`), MongoDB (`mongo-driver/v2`), SQL Server
  (`microsoft/go-mssqldb`), and Oracle (`sijms/go-ora`).
- Oracle needs no Instant Client. `go-ora` speaks the wire protocol directly,
  which is what makes the engine reachable from an airgapped host at all.
- PostgreSQL batch collection: five metadata queries covering every table, run
  concurrently, with a per-table fallback that produces an identical document
  because both paths run the same query text through the same scan function.
- MongoDB schema inference from a sample of documents, reporting each field's
  observed type frequency so a field two writers disagreed about stays visible
  rather than being flattened to one type.
- Multi-database enumeration for PostgreSQL and MySQL, with one small pool per
  database, closed as soon as that database is done.

#### Collection

- Read-only collection of tables, columns, primary and foreign keys, indexes,
  constraints, views, routines, triggers, and user-defined types.
- Optional row sampling, off by default, with per-engine ordering strategies and
  a throttle that is interruptible rather than a plain sleep.
- Four redaction modes applied before values reach disk, and again at report
  time. Redaction is idempotent, so applying it twice cannot un-redact anything.
- Progress reporting that emits no escape sequences when `TERM=dumb` or when
  standard output is not a terminal.

#### Artifacts

- A JSON schema document specified in `docs/formats/schema-document.md`, with
  published JSON Schemas generated from the Go types rather than maintained
  beside them.
- Zstandard compression and AES-256-GCM encryption with Argon2id key derivation.
  The byte layout is specified in `docs/formats/encrypted-envelope.md`, and a
  test reproduces that document's worked example from injected salt and nonce
  bytes.
- Atomic writes throughout: a failed write leaves the previous artifact intact.
- A recursive credential scan on every load and save path, running on the bytes
  before decoding, so a credential sitting in a field the types do not declare is
  caught rather than silently dropped by the decoder.

#### Reporting

- Markdown reports, with optional quality analysis over the sampled rows scoring
  completeness, consistency, and uniqueness.
- Terminal rendering when standard output is a terminal, raw Markdown when it is
  a file or a pipe.

#### Release

- One archive per platform holding both binaries: Linux and macOS on amd64 and
  arm64, Windows on amd64. There are no per-driver variants, because the drivers
  are pure Go and there is nothing to gate.
- Cosign signatures, SBOM, checksums, deb/rpm/apk packages, and a Homebrew cask.
- `CGO_ENABLED=0` and `-trimpath` asserted on the produced binaries, not only on
  the dependency graph.

### Security

- Offline-only operation, with no telemetry and no update checks.
- Every database operation is a read. Where an engine has a session-level
  read-only mode it is requested as well; `SECURITY.md` names the three engines
  that have none rather than implying a guarantee they cannot make.
- Credentials are held as `[]byte` in a type that redacts itself through every
  `fmt`, JSON, and `slog` path, and a repository test refuses to let one become a
  `string` outside the single driver-handoff file per adapter.
- Deterministic zeroization is explicitly **not** claimed. `SECURITY.md` explains
  why, and states what is guaranteed instead.

[Unreleased]: https://github.com/EvilBit-Labs/dbsurveyor/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EvilBit-Labs/dbsurveyor/releases/tag/v0.1.0
