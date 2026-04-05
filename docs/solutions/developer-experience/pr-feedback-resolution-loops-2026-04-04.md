---
title: Lessons from PR #126 Review Feedback Fixes
date: 2026-04-04
category: developer-experience
module: development-workflow
problem_type: developer_experience
component: development_workflow
severity: medium
applies_when:
  - Changing trait signatures that downstream code depends on
  - Adding validation to config structs with builder + direct construction paths
  - Tracking error counts from concurrent tasks
tags:
  - sampling-config
  - trait-signatures
  - validation
  - error-counting
  - tokio-join
---

# Lessons from PR #126 Review Feedback Fixes

## Context

PR review feedback surfaced several design issues introduced during the code review resolution work. These are the technical fixes and the reasoning behind them.

## Guidance

### Do not change trait signatures for internal refactoring

Changing `DatabaseAdapter::sample_table` from `&SamplingConfig` to `&mut SamplingConfig` to allow lazy regex recompilation in `validate()` was flagged as a semver-breaking public API change. The fix (lazy recompile on validate) was not worth the API break.

**Fix:** Reverted to `&SamplingConfig`. Pattern recompilation is handled by `recompile_patterns()` which callers invoke explicitly after deserialization. The `compiled_patterns` field is `pub(crate)` to prevent external code from depending on the cache.

### Builder and validate() must agree on invariants

`with_sample_size(0)` stored 0 unchanged, but `validate()` rejected 0. A deserialized config with `sample_size > MAX_SAMPLE_SIZE` bypassed the builder's clamp but validate() initially didn't catch it either.

**Fix:** Builder uses `.clamp(1, MAX_SAMPLE_SIZE)` -- zero is clamped to 1, oversized is clamped to max. `validate()` rejects both 0 and > MAX_SAMPLE_SIZE as a safety net for direct construction and deserialization paths.

### Count errors, not empty results

After `tokio::join!` on 4 concurrent metadata tasks (views, functions, procedures, triggers), the escalation logic checked `is_empty()` on result vectors. This incorrectly warned on databases that legitimately have no views or triggers.

**Fix:** Count `is_err()` on the original `Result` values before the match arms consume them. Empty-but-successful results are valid; only actual errors indicate a systemic issue.

### `decrypt_data_async` should take ownership, not clone

The async wrapper cloned the entire `EncryptedData` struct to satisfy `spawn_blocking`'s `'static` requirement. For large schemas this doubled ciphertext in memory.

**Fix:** Changed signature from `&EncryptedData` to `EncryptedData` (by value). The caller moves the value in rather than cloning.

### `cargo fmt --all --check` is wrong syntax

The release quality gate used `cargo fmt --all --check` but `--check` must be passed through to rustfmt: `cargo fmt --all -- --check`. Without the `--`, the flag is interpreted by cargo (which ignores it) and the check silently passes.

### `encryption_error()` should not duplicate context as source

The helper created a synthetic `io::Error` with the same message as the context string, causing the error chain to display the same text twice. Use a generic source message instead.

**Fix:** `source: Box::new(io::Error::new(InvalidData, "encryption operation failed"))` instead of cloning the context.

## Why This Matters

These issues were introduced by fixing other review comments -- the fix-verify-fix cycle can create new problems. Each fix above came from a reviewer catching a regression introduced by a prior fix.

## When to Apply

- Adding `validate()` methods to config structs -- ensure all construction paths (builder, struct literal, deserialization) are covered
- Counting failures from `tokio::join!` -- use `is_err()` before match, not `is_empty()` after
- Wrapping sync functions in `spawn_blocking` -- prefer ownership over clone for large data
- Adding quality gates to CI workflows -- verify exact `cargo fmt` syntax

## Related

- `docs/solutions/best-practices/systematic-code-review-batched-resolution-rust-2026-04-03.md`
- GitHub #126
