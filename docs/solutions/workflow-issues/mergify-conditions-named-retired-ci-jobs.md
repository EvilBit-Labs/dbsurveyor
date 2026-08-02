---
title: A merge condition naming a check that does not exist waits forever instead of failing
date: 2026-08-02
category: workflow-issues
module: ci
problem_type: workflow_issue
component: development_workflow
severity: high
applies_when:
  - Renaming, splitting, or removing a CI job that something else names by string
  - Adopting or editing merge automation that gates on named checks
  - A pull request sits on "waiting on CI" with every visible check already green
  - Auditing a repository after a rewrite or migration that renamed jobs wholesale
root_cause: config_error
resolution_type: config_change
tags: [ci, mergify, github-actions, merge-queue, config-drift, branch-protection, rename]
---

# A merge condition naming a check that does not exist waits forever instead of failing

## Context

`.mergify.yml` gated merges on ten named checks:

```yaml
  - check-success = quality
  - check-success = Tests (postgresql)
  - check-success = Tests (sqlite)
  - check-success = Tests (mysql)
  - check-success = Tests (mongodb)
  - check-success = Tests (mssql)
  - check-success = Tests (encryption)
  - check-success = Tests (compression)
  - check-success = Test Coverage
  - check-success = Build Binaries
```

Those are the Rust workspace's job names. The Go rewrite renamed every CI job and this file was not part of that change, so none of the ten had existed for some time. The jobs that actually run are `Lint`, `Build and vet`, `Test (ubuntu-latest)`, `Test (macos-latest)`, `Test (windows-latest)`, `Integration`, `Vulnerabilities`, and `Coverage`.

The reason this survived is the whole lesson. **A condition on a check that never reports does not fail. It waits.** Mergify has no way to distinguish "this check has not finished yet" from "nothing will ever produce this check," so the protection sat pending indefinitely, and the auto-queue rule gated on it never fired. Nothing anywhere said a name was wrong. The pull request simply showed "waiting on CI" while every check visible in the interface was green.

It surfaced only because a bot comment enumerated the checks being waited on, and the names in that list did not match the names in the checks list directly above it.

## Guidance

**Verify check names against what the platform publishes, not against the workflow files.**

The temptation is to grep the workflow YAML for `name:` and diff it against the config. That is the wrong reference, because it is not what the config is compared to at runtime -- a job name is a template, and what appears as a check is the expanded result. `Test (${{ matrix.os }})` in the workflow becomes three checks named `Test (ubuntu-latest)`, `Test (macos-latest)`, and `Test (windows-latest)`. Comparing against source would have reported a mismatch that was not real, and could equally miss one that was.

Ask the platform for the names it actually published on a live pull request, then assert every required name appears in that set:

```bash
gh pr checks <PR> | awk -F'\t' '{print $1}' | sort -u > /tmp/actual.txt
sed -n 's/.*check-success = //p' .mergify.yml | sort -u > /tmp/required.txt

while read -r c; do
    grep -Fxq "$c" /tmp/actual.txt || echo "MISSING: $c"
done < /tmp/required.txt
```

An empty result is the pass condition. Run it against a pull request whose checks have finished: the comparison is only as good as the published set, and a job that has not reported yet cannot be distinguished from one that never will -- which is the same ambiguity that caused the original problem.

**Update every condition list, not the one that was reported.** The same names were repeated across four blocks: a queue rule per bot author, the default queue rule, and the merge protection itself. Fixing only the protection leaves the queues silently broken in exactly the same way.

**Say in the file that the names are load-bearing.** Nothing validates them, so the only durable protection is that the next person renaming a job knows this file exists.

## Why This Matters

Failures that announce themselves get fixed. This class does not announce itself, and the absence of a signal is easy to read as absence of a problem.

The specific harm is that a merge gate which can never pass is indistinguishable, from the outside, from a merge gate that is working and merely slow. Both look like "waiting on CI." The gate had stopped gating -- it was not enforcing the ten checks, it was enforcing nothing and blocking everything -- and the visible symptom of a broken safety mechanism was the same as the visible symptom of a working one.

This is the same family as `GOTCHAS.md` section 1, where two repository tests passed for months while checking an empty set. A check over nothing reports success; a condition over nothing reports pending. Neither reports a problem, and in both cases the fix is to make the emptiness visible rather than to trust that silence means health.

It is also a sibling of the toolchain drift in [ci-installs-toolchains-through-mise.md](../tooling-decisions/ci-installs-toolchains-through-mise.md): a config naming an identifier that has moved, with nothing comparing the two. The shape is shared, but that one ends in an enforced test and this one does not, for the reason below.

## When to Apply

- **Renaming a CI job.** Grep the repository for the old name before considering the rename finished. Merge automation, branch protection rules, status-check requirements, dashboards, and documentation all name jobs by string.
- **After any rewrite or migration.** A change that replaces an implementation wholesale will rename jobs wholesale, and downstream consumers of those names are easy to miss because they are not code and nothing compiles them.
- **When a pull request will not merge and nothing looks wrong.** Read the list of checks being waited on and compare it, name by name, against the list of checks that reported. Do not skim -- the failure is that two lists look similar.

## Examples

The mapping that was applied, and the reasoning per line:

| Retired name                                | Replacement                                          | Note                                                             |
| ------------------------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------- |
| `quality`                                   | `Lint`                                               | Direct rename                                                    |
| `Build Binaries`                            | `Build and vet`                                      | Direct rename                                                    |
| `Test Coverage`                             | `Coverage`                                           | Direct rename                                                    |
| `Tests (postgresql)` and four siblings      | `Integration`                                        | Five per-engine jobs became one job running all container suites |
| `Tests (encryption)`, `Tests (compression)` | `Test (ubuntu-latest)` and the other two matrix legs | Both are unit suites now, covered by the OS matrix               |
| --                                          | `Vulnerabilities`                                    | Added; it runs per change and was not previously gated on        |

Two of those rows are judgment, not translation. Collapsing five engine-specific requirements onto one `Integration` job means the gate no longer distinguishes which engine broke -- and in fact widens it, since that job runs every container suite including Oracle, which was never one of the five. Folding two feature-specific suites into the OS matrix asserts that the new unit suites cover what the old ones did. Both are defensible and both are choices; a rename table that looks purely mechanical can quietly change what a gate enforces, so the non-obvious rows are worth stating rather than burying.

### Why there is no automated guard here

The sibling learning about toolchain drift ends with a repository test that fails when a workflow uses a forbidden action. The equivalent here would parse `check-success` values out of the config and assert each names a real job.

That was considered and not written, because the comparison is against published check names rather than source, and reproducing the expansion mechanically means resolving matrix strategies -- `Test (${{ matrix.os }})` expands to three names that appear nowhere in the file as literals. A test that compared against literal `name:` values would produce false failures for every matrix job, which is worse than no test: a check that cries wolf gets deleted.

The alternative -- querying a live pull request from a test -- makes the suite depend on network access and on a pull request existing, which the repository's offline-first constraints rule out.

So the guard is a comment in the file stating that renaming a CI job means editing it in the same change, and the shell check above as a manual verification step. That is weaker than a test and is recorded as such rather than dressed up.

## Related

- [ci-installs-toolchains-through-mise.md](../tooling-decisions/ci-installs-toolchains-through-mise.md) -- the sibling drift: `go.mod`'s language level against `mise.toml`'s toolchain pin, surfaced by `govulncheck` reporting phantom standard-library advisories.
- `GOTCHAS.md` section 1 -- tests that report success over an empty set, and section 1.4's rule that an invariant check is not evidence until it has been watched failing.
