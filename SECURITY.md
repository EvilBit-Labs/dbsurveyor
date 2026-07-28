# Security Policy

## Supported Versions

| Version | Implementation | Supported |
| ------- | -------------- | --------- |
| 0.2.x   | Go             | yes       |
| 0.1.x   | Rust (retired) | no        |
| < 0.1   | -              | no        |

The 0.1.x line was the Rust implementation, which is preserved on the
`rust-final` branch and no longer receives fixes. See
[ADR 0002](docs/adr/0002-go-clean-slate-rewrite.md).

## Reporting a Vulnerability

**CRITICAL**: Security vulnerabilities must be reported privately. Do NOT create public GitHub issues for security reports.

### Private Reporting Methods

#### 1. GitHub Security Advisories (Recommended)

- Go to this repository's **Security** tab
- Click **"Report a vulnerability"**
- Fill out the security advisory form
- This creates a private issue visible only to maintainers

#### 2. Email (Alternative)

- **Security Email**: <security@evilbitlabs.io>
- **Subject Format**: `[SECURITY] DBSurveyor - [Brief Description]`
- **PGP Key**: [Available upon request for sensitive reports]

#### 3. Private Vulnerability Reporting (PVR)

- Use GitHub's built-in PVR system
- Ensures complete confidentiality during investigation

### Required Information

When reporting a vulnerability, please include:

- **Description**: Clear explanation of the vulnerability
- **Steps to Reproduce**: Detailed reproduction steps (sanitized)
- **Impact Assessment**: Severity and potential consequences
- **Affected Versions**: Specific versions or commit ranges
- **Proposed Fix**: Any mitigation suggestions (if available)

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 1 week
- **Resolution**: Depends on severity and complexity
- **Public Disclosure**: Coordinated after fix is available

## What is guaranteed

These are properties the implementation enforces, each with the mechanism that
enforces it. A guarantee without a mechanism is a hope.

### Credentials never reach an output file

This is the hard guarantee, and the one everything else is arranged around.

- A credential is held as `[]byte` inside a `Secret` type that redacts itself
  through every `fmt` verb, `encoding/json`, and `log/slog` path. It marshals to
  a redaction marker and refuses to unmarshal at all.
- The bytes leave the type through one method, and a repository test rejects any
  conversion of a revealed secret to a `string` outside the single file per
  adapter where a driver has to be handed one.
- Every path that writes or reads a document runs a recursive credential scan.
  The scan runs on the **bytes before decoding**, so a credential sitting in a
  field this version's types do not declare is caught rather than silently
  dropped by `json.Unmarshal` and pronounced clean.
- A document that carries a credential is not writable and not loadable. That
  failure is not suppressible.

### Every database operation is a read

No schema modification, no DML, no temporary objects, no `ANALYZE`. An operator
may be pointed at production with credentials they are not supposed to write
with, and this tool must not be the reason that becomes a problem.

Where an engine has a session-level read-only mode it is requested as a second
line of defense: `default_transaction_read_only` on PostgreSQL,
`transaction_read_only` on MySQL, `PRAGMA query_only` on SQLite.

**Three engines have no such mode.** SQL Server's `ApplicationIntent` only routes
within an availability group, Oracle's read-only transactions are per-transaction
rather than a session state, and MongoDB has nothing equivalent. On a standalone
server of those three the guarantee rests entirely on the implementation issuing
only reads, and their integration tests assert it against the data -- table lists
and row counts before and after a full survey -- rather than by expecting the
server to reject a write.

### Offline only, and airgap compatible

No network call except to the target database. No telemetry, no external
reporting, no update checks. No cgo anywhere in the dependency graph, so the
binary is genuinely self-contained and needs no vendor client library on the
host.

### Encryption at rest

AES-256-GCM with a key derived by Argon2id from an operator-supplied password.
The byte layout is specified in
[`docs/formats/encrypted-envelope.md`](docs/formats/encrypted-envelope.md), and a
test reproduces the worked example in that specification from injected salt and
nonce bytes -- so a layout change that is not also a documentation change is a
test failure rather than a review catch.

The Argon2id cost parameters arrive from the file and are consumed *before*
anything is authenticated, because the key must be derived before the GCM tag can
be checked. They therefore have an upper bound as well as a lower one: an
artifact naming a multi-terabyte memory cost is rejected rather than allowed to
crash the process on open.

## What is not guaranteed

### Deterministic zeroization is not claimed

Credentials are held as `[]byte` rather than `string` and are overwritten after
use on a best-effort basis. That is the whole of the claim.

Go's garbage collector may have copied those bytes during a heap move, and the
runtime offers no way to find or erase the copies. A password that arrived
through the process environment cannot be erased at all: Go hands back a string
from the environment block, and neither the string nor the block is something
this program can overwrite.

Anyone whose threat model includes an attacker reading this process's memory
should assume the credential is recoverable from it. The guarantee this project
does make is narrower and enforceable: the credential does not reach a file, a
log, or an error message.

### A schema is not the data

An artifact holds structure by default. Sampling is opt-in because it is the only
part of a survey that reads user data, and sampled values are masked before they
reach disk. Redaction is pattern-based and errs toward masking -- but it is a
heuristic over column names and value shapes, not a classifier. An operator
sharing an artifact that contains samples should read it first.

### Quality metrics describe a sample

Scores are computed over the sampled rows only. A score of 1.0 over three rows is
a much weaker statement than the same score over a thousand, which is why the row
count is reported beside every score. Anomaly metrics carry a mean and a standard
deviation, which on a column such as salary or transaction amount still describe
the distribution -- weigh that before sharing a quality report.

### Responsible Disclosure

- Keep vulnerability details confidential until resolved
- Allow reasonable time for investigation and fix development
- Coordinate public disclosure with maintainers
- Do not exploit vulnerabilities beyond what's necessary for reporting

### Security Contacts

- **Primary**: <security@evilbitlabs.io>
- **Maintainer**: @unclesp1d3r
- **Response Time**: 48 hours acknowledgment

### Security Updates

Security updates are released as patch versions (for example 0.2.1) and should be
applied promptly. Critical vulnerabilities may result in an immediate patch
release.

`govulncheck` runs in CI and as part of the local `just check` gate, over the
whole dependency graph. It is run per change rather than per release, because a
vulnerability that is *reachable* from this code is a different thing from one
that merely exists somewhere in the graph, and the distinction is only visible
while the change that introduced it is still in hand.

---

**Note**: This security policy is designed to protect users while ensuring vulnerabilities are addressed promptly and responsibly.
