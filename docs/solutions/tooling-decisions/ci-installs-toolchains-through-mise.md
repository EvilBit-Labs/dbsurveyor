---
title: CI installs toolchains through mise, not setup-go
date: 2026-07-28
last_updated: 2026-08-02
category: tooling-decisions
module: ci
problem_type: tooling_decision
component: tooling
severity: high
applies_when:
  - A repository pins its toolchain in mise.toml
  - Adding or rewriting a GitHub Actions workflow that builds or tests Go
  - CI passes but the same command fails locally, or the reverse
  - A scanner reports standard-library advisories that do not reproduce locally
tags: [ci, mise, github-actions, setup-go, toolchain, goreleaser, govulncheck]
---

# CI installs toolchains through mise, not setup-go

## Context

A rewritten CI workflow used `actions/setup-go` with `go-version-file: go.mod`, while every other workflow in the repository used `jdx/mise-action` and `mise.toml` held the pins. It looked like a harmless stylistic difference. It was not:

| Source      | Go version |
| ----------- | ---------- |
| `mise.toml` | `1.26.5`   |
| `go.mod`    | `1.26.1`   |

CI was running a different toolchain from every contributor, silently.

The same workflow also installed `golangci-lint` at `latest` through `golangci-lint-action`, while `mise.toml` already pinned it -- two sources of version truth for one tool, which is how a lint rule appears in CI that nobody can reproduce.

## Guidance

**Install the toolchain through `mise-action`, and invoke `just` recipes rather than raw commands.**

```yaml
  - uses: actions/checkout@v5
  - uses: jdx/mise-action@<pinned-sha> # v4.2.2
    with:
      install: true
      cache: true
      github_token: ${{ secrets.GITHUB_TOKEN }}

  - name: Lint
    run: just lint
  - name: Test
    run: just test
```

`mise.toml` then pins Go, `golangci-lint`, `govulncheck`, `goreleaser`, and `just` in one place, and a contributor reproducing a CI failure runs the same commands CI ran.

`actions/setup-go` still has a legitimate use: layering a *different* Go on top of mise for a version matrix. Do that deliberately and say so in a comment; do not use it as the default toolchain source.

## Why This Matters

`go.mod`'s `go` directive is a **language level**, not a toolchain pin. It says "this module needs at least this much Go," which is a different question from "which Go should build it." Treating it as a pin means the two drift apart the moment somebody bumps one and not the other -- and nothing reports the drift, because both values are individually valid.

### What finally detects the drift: govulncheck

The drift stayed invisible for months and then surfaced as nine standard-library vulnerabilities on every branch (2026-07-31):

```text
Vulnerability #1: GO-2026-5856
    Found in: crypto/tls@go1.26.1
    Fixed in: crypto/tls@go1.26.5
...
Your code is affected by 9 vulnerabilities from the Go standard library.
```

None were real. `govulncheck` resolves advisories against the standard library's **patch** version, which makes it the one tool in the pipeline that can see the gap between the language level in `go.mod` and the toolchain in `mise.toml`. Everything else was happy, because a 1.26.1 compiler builds a 1.26.1-language-level module perfectly well.

Two things worth recognizing on sight:

- **Nine simultaneous stdlib advisories mean an old toolchain, not nine new bugs.** They are standard library, not module dependencies, so there is nothing in `go.mod` to bump.
- **`setup-go` also sets `GOTOOLCHAIN=local`**, so Go's own toolchain switching will not quietly correct the version later.

The diagnostic that settled it was internal to CI: `security.yml` ran a byte-identical `govulncheck ./...` and passed on the *same commit* the other workflow failed on. One command, one tree, two opposite results isolates the variable immediately.

**Do not fix this by bumping the `go` directive.** Raising `go.mod` to 1.26.5 turns the red green and is the wrong move: the directive states what the module requires, and overwriting that to silence a CI misconfiguration discards real information while leaving the two files free to drift again.

Two further consequences of the split that cost real time here:

**GoReleaser refuses a dirty tree.** `mise.toml` set `lockfile = true`, so installing tools rewrites `mise.lock`. A release job that installs through mise and then runs GoReleaser fails on a file the toolchain installer wrote, not on anything a human changed. Reset it first:

```yaml
  - name: Reset mise.lock to avoid a dirty-tree failure
    run: git checkout -- mise.lock || true
```

**A dropped step is invisible.** The same rewrite silently dropped the Codecov upload. The badge in the README stayed, pointing at coverage that had stopped being reported. A workflow rewrite should diff against what it replaced, not just against what looks complete.

## Enforce it with a test, not a comment

This guidance existed as a comment in `ci.yml` describing the hazard in detail, and a workflow sat next to it doing the warned-against thing anyway. A convention that lives only in prose is enforced by whoever happens to have read the prose.

The guard is a repository-level test that enumerates tracked workflows and fails on a forbidden `uses:` (`tools/workflow_test.go`):

```go
var usesKey = regexp.MustCompile(`^\s*(-\s*)?uses\s*:`)

for _, path := range trackedWorkflows(t, root) {
    // ... read the file, skip comment lines ...
    if !usesKey.MatchString(line) {
        continue
    }
    for _, forbidden := range forbiddenWorkflowUses {
        if strings.Contains(line, forbidden) {
            t.Errorf("%s installs a toolchain with %s; use jdx/mise-action", path, forbidden)
        }
    }
}
```

Three details that matter more than they look:

- **Match the key with a regex, not `strings.Contains(line, "uses:")`.** YAML permits `uses : value`, so a substring search is a one-character evasion of a guard whose entire job is to be unevadeable.
- **Skip comment lines.** The workflows explain *why* they avoid `setup-go`, so a naive scan trips on its own documentation.
- **Break the invariant and watch it fail before trusting it.** Reintroducing `setup-go`, confirming red, then reverting is the only evidence the check is wired to anything.

## Do not run one check in three places

The same `govulncheck` ran in three workflows at once. When one drifted, the other two supplied green checks beside the red one. Redundancy of this kind does not add confidence -- it gives a reviewer something else to look at. Two were removed, leaving a per-change scan and a scheduled one.

Naming compounds it: the failing job was called `govulncheck`, while a green check named `audit` came from an unrelated workflow. Anyone scanning for "did the dependency audit pass" found the green `audit` and stopped. Distinct, descriptive job names are what make a red check findable.

Related: CI runs on billed minutes, so it should be catching broken code. Style checks that a hook can catch before the commit exists (yamllint, actionlint) belong in pre-commit, not in a workflow.

## When to Apply

- Writing any new workflow in a repository that has a `mise.toml`
- Reviewing a workflow that installs a tool the repo already pins
- Debugging a CI failure that will not reproduce locally -- check the toolchain versions before the code

## Examples

The Codecov step, with the conditional that keeps Dependabot PRs from failing on a token they cannot see:

```yaml
  - name: Upload to Codecov
    uses: codecov/codecov-action@<pinned-sha> # v7.0.0
    with:
      token: ${{ secrets.CODECOV_TOKEN }}
      files: ./coverage.out
      # Dependabot pull requests run without access to CODECOV_TOKEN, so a
      # tokenless upload to a protected branch is rejected.
      fail_ci_if_error: ${{ github.actor != 'dependabot[bot]' }}
```

A cross-compile check is worth adding to any repo that releases multiple platforms, so a break on a platform nobody develops on surfaces in CI rather than during a release:

```yaml
  - name: Cross-compile check
    run: |
      for target in linux/amd64 linux/arm64 darwin/amd64 darwin/arm64 windows/amd64; do
          GOOS=${target%/*} GOARCH=${target#*/} go build -o /dev/null ./cmd/... || exit 1
      done
```

## Related

- `AGENTS.md` names `EvilBit-Labs/opnDossier` as the reference for tooling questions. Reading its live workflows surfaced both the `mise.lock` reset and the Codecov conditional -- neither was guessable.
- `mise.toml` is the single source of pinned versions; `go.mod` is not.
