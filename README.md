# DBSurveyor

[![License][license-badge]][license] [![Sponsors][sponsors-badge]][sponsors]

[![CI][ci-badge]][ci]

[![codecov][codecov-badge]][codecov] [![Issues][issues-badge]][issues] [![Last Commit][commits-badge]][commits] [![OpenSSF Scorecard][scorecard-badge]][scorecard] [![OpenSSF Best Practices][bestpractices-badge]][bestpractices]

## Overview

DBSurveyor surveys a database server, extracts its schema and optionally a sample
of its rows, and writes a portable structured artifact. It makes no network call
except to the database it was pointed at, sends no telemetry, and never writes a
credential into its output.

It is built for operators who need auditable database documentation in airgapped
or contested environments -- which is why it ships as one static binary per
platform with no client libraries to install.

> **This tree is a Go implementation.** It replaces the retired Rust
> implementation, which is preserved in full on the
> [`rust-final`](https://github.com/EvilBit-Labs/dbsurveyor/tree/rust-final)
> branch. The formats are specified fresh with the Go implementation as the
> reference, and nothing here reads a Rust-era artifact. See
> [ADR 0002](docs/adr/0002-go-clean-slate-rewrite.md) for why.

## Quick start

### Installation

**Pre-built binaries** are on the [Releases] page for Linux, macOS, and Windows.

**Homebrew** (macOS/Linux):

```bash
brew install EvilBit-Labs/tap/dbsurveyor
```

**From source** (Go 1.26 or later):

```bash
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor
go build -o dist/ ./cmd/...
```

### Basic usage

```bash
# Survey a PostgreSQL database
dbsurveyor-collect postgres://user:pass@localhost:5432/mydb

# Render the artifact as Markdown
dbsurveyor schema.json
```

## The two binaries

- **`dbsurveyor-collect`** connects to a database and writes an artifact. It is
  the only half that touches a database.
- **`dbsurveyor`** reads an artifact and renders a report. It makes no network
  call and reads no connection string.

The split is deliberate. Collection needs credentials and network access to a
production database; reporting needs neither, so an artifact can be carried out
of a restricted environment and read anywhere.

## Database support

All six engines are implemented and use pure-Go drivers, so no vendor client
library is needed on the host.

| Engine     | Connection string                     | Driver                     |
| ---------- | ------------------------------------- | -------------------------- |
| PostgreSQL | `postgres://user:pass@host:5432/db`   | `jackc/pgx/v5`             |
| MySQL      | `mysql://user:pass@host:3306/db`      | `go-sql-driver/mysql`      |
| SQLite     | `sqlite:///path/to/database.db`       | `modernc.org/sqlite`       |
| MongoDB    | `mongodb://user:pass@host:27017/db`   | `mongo-driver/v2`          |
| SQL Server | `sqlserver://user:pass@host:1433?database=db` | `microsoft/go-mssqldb` |
| Oracle     | `oracle://user:pass@host:1521/SERVICE` | `sijms/go-ora`            |

MariaDB works through the MySQL adapter. Oracle needs **no Instant Client**:
`go-ora` speaks the wire protocol directly, which is the property that makes the
engine reachable from an airgapped host at all.

CockroachDB speaks the PostgreSQL wire protocol, so the PostgreSQL adapter may
work against it. That is untested and unclaimed.

MongoDB stores no schema, so its output is **inferred from a sample of
documents**. Every field, type, and nullability in a MongoDB report is a claim
about the sample rather than about the collection, and the report says so.

## Usage

### Collecting

```bash
# Compress the artifact (.json.zst)
dbsurveyor-collect --compress postgres://localhost/db

# Encrypt it (.enc). The password comes from DBSURVEYOR_ENCRYPTION_PASSWORD
# when set, and from a prompt otherwise.
dbsurveyor-collect --encrypt postgres://localhost/db

# Read rows as well as structure. Off by default: sampling is the only part of
# a survey that touches user data.
dbsurveyor-collect --sample --sample-size 50 postgres://localhost/db

# Keep the survey from monopolizing a production database
dbsurveyor-collect --sample --throttle 250ms postgres://localhost/db

# The connection string may come from the environment instead
export DATABASE_URL="postgres://user:pass@localhost/db"
dbsurveyor-collect
```

### Reporting

```bash
# Render a report to standard output
dbsurveyor schema.json

# An encrypted artifact reads its password the same way the collector wrote it
dbsurveyor schema.enc

# Include the sampled rows, and score them
dbsurveyor --samples --analyze schema.json

# Write the report to a file, as raw Markdown
dbsurveyor --output report.md schema.json
```

### Redaction

Sampled values are masked before they reach an artifact and again before they
reach a report. Four modes, each masking a superset of the one before it:

| Mode           | Masks                                                          |
| -------------- | -------------------------------------------------------------- |
| `none`         | nothing                                                         |
| `minimal`      | columns whose names denote a secret                             |
| `balanced`     | and columns whose names denote personal data (**the default**)  |
| `conservative` | every string except identifiers, timestamps, and date-like values |

Redaction is idempotent, so applying it at collection time and again at report
time cannot un-redact anything.

## What this tool guarantees

- **Offline only.** No network call except to the target database. No telemetry,
  no external reporting, no update checks.
- **Read only.** Every database operation is a read. No schema modification, no
  DML, no temporary objects. Where an engine has a read-only session mode, it is
  requested as well; where one does not, [`GOTCHAS.md`](GOTCHAS.md) says so
  plainly rather than implying a guarantee the engine cannot make.
- **Credentials never reach output.** Not an artifact, not a log line, not an
  error message. Every load path terminates in a recursive credential scan that
  runs on the bytes before decoding, so a credential in a field the types do not
  declare is caught too.
- **Airgap compatible.** Full functionality with no internet access. No cgo
  anywhere in the dependency graph, so the binary is genuinely self-contained.

Deterministic zeroization of credentials in memory is **not** claimed. See
[SECURITY.md](SECURITY.md) for what is and is not guaranteed and why.

## Formats

The on-disk formats are specified independently of the code, and each
specification is held to the implementation by a test rather than by review:

- [`docs/formats/schema-document.md`](docs/formats/schema-document.md) -- the JSON
  schema document
- [`docs/formats/encrypted-envelope.md`](docs/formats/encrypted-envelope.md) --
  the AES-256-GCM byte layout, with a worked example a test reproduces
- [`docs/formats/compression.md`](docs/formats/compression.md) -- zstd framing,
  extension dispatch, and the atomic-write contract

## Documentation

Full documentation is at **[evilbitlabs.io/dbsurveyor](https://evilbitlabs.io/dbsurveyor)**.

Quick links: [Installation](docs/src/installation.md) | [Quick Start](docs/src/quick-start.md) | [CLI Reference](docs/src/cli-reference.md) | [Database Support](docs/src/database-support.md) | [Security](docs/src/security.md) | [Troubleshooting](docs/src/troubleshooting.md)

## Development

```bash
just build            # both binaries into ./dist
just test             # go test ./...
just test-integration # container-backed adapter tests (needs Docker)
just format           # run BEFORE just check
just check            # format-check, lint, test, vuln -- the gate
```

`just check` is the gate. Run `just format` first: a formatting-only failure is
otherwise indistinguishable from a lint failure in the log.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow, [AGENTS.md](AGENTS.md)
for the conventions, and [GOTCHAS.md](GOTCHAS.md) for the behaviors that have
already cost somebody time.

## Contributing

Contributions are welcome. Please open an issue to discuss proposed changes
before submitting a pull request.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

<!-- Badge links -->

[bestpractices]: https://www.bestpractices.dev/projects/9872
[bestpractices-badge]: https://www.bestpractices.dev/projects/9872/badge
[ci]: https://github.com/EvilBit-Labs/dbsurveyor/actions/workflows/ci.yml
[ci-badge]: https://img.shields.io/github/actions/workflow/status/EvilBit-Labs/dbsurveyor/ci.yml?style=flat-square&label=CI
[codecov]: https://codecov.io/gh/EvilBit-Labs/dbsurveyor
[codecov-badge]: https://img.shields.io/codecov/c/github/EvilBit-Labs/dbsurveyor?style=flat-square
[commits]: https://github.com/EvilBit-Labs/dbsurveyor/commits/main
[commits-badge]: https://img.shields.io/github/last-commit/EvilBit-Labs/dbsurveyor?style=flat-square
[issues]: https://github.com/EvilBit-Labs/dbsurveyor/issues
[issues-badge]: https://img.shields.io/github/issues/EvilBit-Labs/dbsurveyor?style=flat-square
[license]: https://github.com/EvilBit-Labs/dbsurveyor/blob/main/LICENSE
[license-badge]: https://img.shields.io/github/license/EvilBit-Labs/dbsurveyor?style=flat-square
[releases]: https://github.com/EvilBit-Labs/dbsurveyor/releases
[scorecard]: https://scorecard.dev/viewer/?uri=github.com/EvilBit-Labs/dbsurveyor
[scorecard-badge]: https://img.shields.io/ossf-scorecard/github.com/EvilBit-Labs/dbsurveyor?style=flat-square
[sponsors]: https://github.com/sponsors/EvilBit-Labs
[sponsors-badge]: https://img.shields.io/github/sponsors/EvilBit-Labs?style=flat-square
