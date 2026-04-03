# Core test infrastructure: security tests, CI database matrix, and coverage threshold

## Context

Testcontainers are already a workspace dependency and the nextest config exists. This ticket closes the remaining test gaps: security test suite (credential sanitization, offline operation, SQL injection resistance), CI database matrix (PostgreSQL + MySQL + MongoDB containers in GitHub Actions), and coverage threshold enforcement.

**Depends on**: T4 (MySQL, SQLite, MongoDB adapters complete), T7 (Markdown pipeline — so the full end-to-end flow is testable) **Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/f257e12b-711f-4d82-9249-e74749688e3c — Minimal capability checklist and Success criteria.

## Scope

### In scope

**Security tests** (`dbsurveyor-collect/tests/security_tests.rs` — already exists, extend it):

- Credential sanitization: verify database URLs with passwords never appear in any `DbSurveyorError` display string or log output
- Output file credential check: collected schema JSON never contains the password component of the connection string
- SQL injection resistance: table/column names containing `'; DROP TABLE users; --` and similar payloads produce correct `DbSurveyorError::collection_failed` (not a panic or successful injection)
- Collector output mode coverage: verify successful output generation for plain (`.dbsurveyor.json`), compressed (`.json.zst`), and encrypted (`.enc`) modes, and verify password mismatch produces canceled behavior (distinct non-zero path, no output file written)
- Offline operation: postprocessor binary path — loading a `.dbsurveyor.json`, `.json.zst`, and `.enc` file and generating Markdown makes zero network calls (verified by running with loopback-only network namespace, or by asserting no `reqwest`/`ureq`/`hyper` symbols are linked into the postprocessor binary)
- Postprocessor format handling: validate and load plain/compressed/encrypted schema files successfully in offline mode, including `validate` and `generate` command paths

**CI database matrix** (`.github/workflows/ci.yml`):

- Add a Linux-only job that starts PostgreSQL, MySQL, and MongoDB via `testcontainers-modules` (or Docker service containers) and runs all integration tests with `--features postgresql,mysql,mongodb,sqlite`
- Keep existing macOS/Windows jobs scoped to SQLite-only tests (no containers required)
- Matrix job must fail fast on test failure and emit clear per-database failure output

**Coverage threshold** (`.github/workflows/ci.yml`):

- Add `cargo llvm-cov` step for `dbsurveyor-core` on the Linux job
- Enforce `--fail-under-lines 70` for the core library (consistent with existing nextest config reference in `tasks.md`)
- Coverage report uploaded as a CI artifact

**`dbsurveyor-core/tests/`** — add any missing unit/integration tests to meet 70% line coverage for `dbsurveyor-core` (focus on untested paths in `quality/`, `security/`, and `validation/` modules identified by the coverage report)

### Out of scope

- Property-based testing with proptest (future, tasks.md task 19.4)
- CLI snapshot testing with `insta` (future, tasks.md task 23)
- Performance benchmarks with Criterion (future, tasks.md task 24)
- Pro-tier or WASM plugin testing

## Acceptance Criteria

- All security tests pass in CI on the Linux job
- No credential appears in any error message or output file in the sanitization tests
- The SQL injection test does not panic and produces a well-formed error
- Collector output generation is verified for plain/compressed/encrypted modes
- Encryption password mismatch path is verified to return canceled behavior and write no output file
- Postprocessor validate/generate flows successfully load plain/compressed/encrypted schema files offline
- The CI database matrix job runs and passes for all three container-backed databases
- `dbsurveyor-core` line coverage ≥ 70% as reported by `cargo llvm-cov`
- Coverage check failure causes the CI job to fail with a clear threshold message
- macOS and Windows CI jobs continue to pass (SQLite-only, no container dependency)
