# GitHub Copilot Instructions

The conventions for this repository live in
[AGENTS.md](../AGENTS.md), which is the single source for coding standards,
layout rules, and the security guarantees every change must maintain. This file
used to duplicate them and drifted; it now points at the one copy.

Read, in order:

| Document                             | What it covers                                        |
| ------------------------------------ | ----------------------------------------------------- |
| [AGENTS.md](../AGENTS.md)            | Layout, stack, conventions, requirements R1-R18        |
| [GOTCHAS.md](../GOTCHAS.md)          | Behaviors that have already cost somebody time         |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Workflow, commit format, what gets a PR closed        |
| [SECURITY.md](../SECURITY.md)        | What is guaranteed, and what is deliberately not       |

## The short version

- **Go 1.26**, one module at the repository root. Application packages under
  `internal/`; only `cmd/` sits outside it.
- **No cgo, anywhere.** A repository test fails on any cgo dependency entering
  the graph. This is what makes one static binary work on an airgapped host.
- **Every database operation is a read.** No DML, no schema modification, no
  temporary objects.
- **Credentials never reach output** -- not an artifact, not a log line, not an
  error message. Enforced by a recursive scan on every load path and by a
  repository test that refuses to let a credential become a `string`.
- **ASCII only**, in source and in Markdown, checked byte by byte.
- **`just format` before `just check`.** The gate starts with a format check.
- **Before trusting a new invariant test, break the invariant and watch it
  fail.** Two tests in this tree reported success over an empty set for months.
