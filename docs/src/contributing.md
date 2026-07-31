# Contributing

The authoritative contributor guide lives at the repository root, in
[CONTRIBUTING.md](https://github.com/EvilBit-Labs/dbsurveyor/blob/main/CONTRIBUTING.md).
It is kept there rather than duplicated here so there is one copy to keep
current.

This page is a summary and a set of pointers.

## The short version

```bash
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor
just dev-setup
pre-commit install
```

```bash
just format           # run this BEFORE just check
just check            # format-check, lint, test, vuln -- the gate
just test-integration # container-backed adapter tests (needs Docker)
```

`just check` is the gate. Run `just format` first: the gate starts with a format
check, so skipping it turns a whitespace difference into a failure that reads
like a lint error.

## Before you write code

1. **Read [GOTCHAS.md](https://github.com/EvilBit-Labs/dbsurveyor/blob/main/GOTCHAS.md).**
   It records behaviors that have already cost somebody time: tests that
   reported success over nothing, the asymmetry between how artifacts are
   written and read, linter rules that contradict each other, and a per-engine
   list of database quirks re-validated against the current adapters.

2. **Open an issue first.** For anything beyond a typo, discuss it before writing
   code. This saves everyone time if the change does not fit the project's
   direction.

3. **One issue per pull request.** If a fix needs a refactor, that is a separate
   pull request, discussed first.

## Non-negotiables

Every change must maintain these. They are what the tool exists to promise:

- **Offline only.** No network call except to the target database. No telemetry.
- **Read only.** No schema modification, no DML, no temporary objects.
- **Credentials never reach output.** Not an artifact, not a log line, not an
  error message.
- **No cgo.** A repository test fails on any cgo dependency entering the graph.
- **ASCII only**, in source and in Markdown, checked byte by byte.

Several of these are enforced by tests rather than by review, because a reviewer
will eventually miss one. See the architecture section of the root guide.

## A test that has never failed is not evidence

Before trusting a new invariant test, break the invariant on purpose and watch it
fail. Two repository-level tests in this tree reported success over an empty set
for months. Both are recorded in GOTCHAS.md section 1, and the pattern is the
lesson rather than the two specific bugs.

## AI-assisted contributions

Accepted, under the terms in
[AI_POLICY.md](https://github.com/EvilBit-Labs/dbsurveyor/blob/main/AI_POLICY.md).
The short version: you own every line you submit and must be able to explain it,
disclose your tools in the pull request, and do not submit output you have not
read and run.

## Reporting a vulnerability

See [SECURITY.md](https://github.com/EvilBit-Labs/dbsurveyor/blob/main/SECURITY.md).
Do not open a public issue for a security vulnerability.
