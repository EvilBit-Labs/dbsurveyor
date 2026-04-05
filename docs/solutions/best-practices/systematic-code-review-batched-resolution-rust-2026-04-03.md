---
title: Systematic Code Review to Batched Resolution Pattern for Single-Maintainer Rust Projects
date: 2026-04-03
category: best-practices
module: development-workflow
problem_type: best_practice
component: development_workflow
severity: medium
applies_when:
  - Code review produces more than 10 actionable findings
  - Findings span multiple crates, modules, or concern domains
  - The codebase has a working CI pipeline that can serve as a gate
  - Multiple independent changes can be made without touching overlapping files
tags:
  - code-review
  - batch-processing
  - parallel-agents
  - ci-gates
  - rust
  - single-maintainer
  - refactoring
  - security
---

# Systematic Code Review to Batched Resolution Pattern for Single-Maintainer Rust Projects

## Context

A ~35K-line Rust workspace (3 crates: `dbsurveyor-core`, `dbsurveyor-collect`, `dbsurveyor`) needed a comprehensive code review covering security, performance, architecture, code quality, testing, documentation, best practices, and CI/CD. The review produced 82 findings across 8 categories, with 29 requiring immediate action (7 Critical, 22 High). The challenge was resolving these efficiently without introducing regressions from concurrent changes across the codebase, as a single maintainer.

## Guidance

Use a **batch-and-parallel** resolution workflow for large-scale code review findings:

### 1. Multi-Phase Review with Specialized Agents

Run specialized agents in parallel for each review dimension, with checkpoint gates between phases to synthesize overlapping findings before proceeding:

- Phase 1: Code Quality + Architecture (parallel)
- Phase 2: Security + Performance (parallel)
- Checkpoint: User reviews findings before continuing
- Phase 3: Testing + Documentation (parallel)
- Phase 4: Best Practices + CI/CD (parallel)
- Phase 5: Consolidated report

### 2. Structured Todo Files with Frontmatter

File each finding as a standalone todo with machine-readable frontmatter:

```yaml
---
id: "008"
status: ready
priority: p1
category: security
source: full-review/02-security-performance.md
---

# Add SamplingConfig validation with sample_size upper bound

## Problem
SamplingConfig.sample_size accepts u32::MAX, producing LIMIT 4294967295.

## Fix
Add MAX_SAMPLE_SIZE = 10_000 constant and validate() method.
```

Mark P0/P1 items `ready` (actionable now), P2/P3 as `pending` (need triage).

### 3. Dependency-Ordered Batches

Group ready items into batches where items within a batch are independent but batches depend on prior batches completing:

1. **CI/Config fixes first** -- unblock the pipeline
2. **Security fixes** -- cap dangerous inputs, restrict visibility
3. **Error handling & CLI** -- fix error variants, hide stubs
4. **Core refactors** -- dead code removal, module moves (these change structure that later batches depend on)
5. **Performance & docs** -- parallelization, pool tuning, doc alignment

### 4. Parallel Execution Within Batches

Run up to 4 agents simultaneously on independent items within the same batch. Each agent owns a distinct set of files to avoid merge conflicts.

### 5. CI Gate Between Every Batch

Run the full validation suite after each batch:

```
just fmt && just ci-check
# Pre-commit hooks: actionlint, clippy, fmt, cargo-audit
# Full test suite: 536 tests via nextest
# Dependency check: cargo deny (advisories, bans, licenses, sources)
```

Fix any failures before starting the next batch.

## Why This Matters

- **Regression prevention**: CI gates between batches catch cross-agent conflicts immediately. Without gates, a security fix in batch 2 could silently break a refactor in batch 4.
- **Throughput**: Parallel agents within a batch provide 3-4x speedup over sequential resolution. 19 fixes completed in 5 batches rather than 19 sequential passes.
- **Traceability**: Structured todo files enable picking up work across sessions. A new session can filter by `status: ready` and `priority: p0` without re-analyzing the codebase.
- **Conflict avoidance**: Dependency ordering ensures foundational changes (dead code removal, module moves) land before dependent changes (refactors referencing moved code).
- **Safety net**: Pre-commit hooks (actionlint, clippy -D warnings, cargo fmt, cargo-audit) catch issues before they enter the commit history.

## When to Apply

- Code review produces more than 10 actionable findings
- Findings span multiple crates, modules, or concern domains (security, performance, architecture)
- The codebase has a working CI pipeline with pre-commit hooks
- Multiple independent changes can be made without touching overlapping files
- The project uses a strict linting policy (e.g., `clippy -D warnings`, `deny(unsafe_code)`)

Do not use for fewer than 5 findings or when all findings touch the same file -- sequential resolution is simpler.

## Examples

### Batch Ordering Prevents Conflicts

**Before (ad hoc)**: Pre-compiling regex in `sampling.rs`, then removing the dead adapter layer that included an old copy of `sampling.rs`, then discovering the removal created a conflict with the regex fix because both touched overlapping module declarations.

**After (dependency-ordered)**:

- Batch 4: Remove 2,895 lines of dead duplicate adapter code
- Batch 4 (same batch, parallel): Pre-compile regex at `SamplingConfig` construction
- No conflict because both agents touch different files

### Parallel Agent Assignment Within a Batch

```
Batch 2 (Security, 4 parallel agents):
  Agent A: Cap sample_size with MAX_SAMPLE_SIZE [config/sampling.rs]
  Agent B: Quote SQLite SystemRowId [sqlite/sampling.rs]
  Agent C: Make connection_string pub(crate) [sqlite/mod.rs]
  Agent D: Create rust-toolchain.toml [rust-toolchain.toml]
```

Each agent owns distinct files. CI gate after batch verifies no conflicts.

### Results From This Session

| Metric                    | Value                             |
| ------------------------- | --------------------------------- |
| Total findings            | 82                                |
| Todos filed               | 74                                |
| Resolved this session     | 19                                |
| Commits                   | 5                                 |
| Lines removed (dead code) | 2,895                             |
| Tests passing             | 536/536                           |
| Batches                   | 5                                 |
| Agents per batch          | 3-4                               |
| CWEs addressed            | CWE-400, CWE-89, CWE-200, CWE-755 |

## Related

- `.full-review/05-final-report.md` -- Consolidated review report with all 82 findings
- `.context/compound-engineering/todos/` -- 74 structured todo files (19 complete, 10 ready, 44 ready after triage, 1 deleted as duplicate)
- GitHub #22 -- Comprehensive Security Hardening
- GitHub #23 -- Enhanced Collector Performance Optimization
- GitHub #39 -- CI job parity with local dev environment
