# justfile - DBSurveyor Security-First Developer Tasks
# Cross-platform justfile using OS annotations
# Windows uses PowerShell, Unix uses bash

set shell := ["bash", "-cu"]
set windows-shell := ["powershell", "-NoProfile", "-Command"]
set dotenv-load := true
set ignore-comments := true

# Use mise to manage all dev tools
# See mise.toml for tool versions

mise_exec := "mise exec --"
root := justfile_dir()

# =============================================================================
# GENERAL COMMANDS
# =============================================================================

default:
    @just --list

# =============================================================================
# CROSS-PLATFORM HELPERS (private)
# =============================================================================

[private]
[windows]
ensure-dir dir:
    New-Item -ItemType Directory -Force -Path "{{ dir }}" | Out-Null

[private]
[unix]
ensure-dir dir:
    /bin/mkdir -p "{{ dir }}"

[private]
[windows]
rmrf path:
    if (Test-Path "{{ path }}") { Remove-Item "{{ path }}" -Recurse -Force }

[private]
[unix]
rmrf path:
    /bin/rm -rf "{{ path }}"

# =============================================================================
# SETUP AND INITIALIZATION
# =============================================================================

# Development setup - mise handles all tool installation via mise.toml
setup:
    mise install

# =============================================================================
# FORMATTING AND LINTING
# =============================================================================

alias format-rust := fmt
alias lint-rust := clippy

# Main format recipe - calls all formatters
format: fmt fmt-justfile

# Format Rust code
fmt:
    @{{ mise_exec }} cargo fmt --all

# Check Rust code formatting
fmt-check:
    @{{ mise_exec }} cargo fmt --all --check

# Format justfile
fmt-justfile:
    @{{ mise_exec }} just --fmt --unstable

# Lint justfile formatting
lint-justfile:
    @{{ mise_exec }} just --fmt --check --unstable

# Lint Rust code with clippy (strict zero-warning policy)
clippy:
    @{{ mise_exec }} cargo clippy --workspace --all-targets --all-features -- -D warnings

# Lint with minimal features
clippy-min:
    @{{ mise_exec }} cargo clippy --workspace --all-targets --no-default-features -- -D warnings

# Main lint recipe
lint: clippy lint-justfile

# Run clippy with fixes
fix:
    @{{ mise_exec }} cargo clippy --fix --allow-dirty --allow-staged

# Quick development check
check: fmt-check lint

# =============================================================================
# BUILDING
# =============================================================================

# Build in debug mode
build:
    @{{ mise_exec }} cargo build --workspace --all-features

# Build in release mode
build-release:
    @{{ mise_exec }} cargo build --workspace --release --all-features

# Build minimal feature set (for airgap environments)
build-minimal:
    @{{ mise_exec }} cargo build --release --no-default-features --features sqlite

# =============================================================================
# TESTING
# =============================================================================

# Run all tests with nextest
test:
    @{{ mise_exec }} cargo nextest run --workspace --features postgresql,sqlite,encryption,compression

# Run tests excluding benchmarks
test-no-bench:
    @{{ mise_exec }} cargo nextest run --features postgresql,sqlite,encryption,compression --lib --bins --tests

# Run integration tests only
test-integration:
    @{{ mise_exec }} cargo nextest run --test '*' --features postgresql,sqlite,encryption,compression

# Run unit tests only
test-unit:
    @{{ mise_exec }} cargo nextest run --lib --features postgresql,sqlite,encryption,compression

# Run doctests (nextest doesn't support doctests)
test-doc:
    @{{ mise_exec }} cargo test --doc --features postgresql,sqlite,encryption,compression

# Run tests with CI profile
test-ci:
    @{{ mise_exec }} cargo nextest run --profile ci --features postgresql,sqlite,encryption,compression --workspace

# Run tests with verbose output
test-verbose:
    @{{ mise_exec }} cargo nextest run --features postgresql,sqlite,encryption,compression --workspace --no-capture

# Test PostgreSQL adapter
test-postgres:
    @{{ mise_exec }} cargo nextest run postgres --features postgresql

# Test comprehensive PostgreSQL adapter functionality
test-postgres-comprehensive:
    cd dbsurveyor-core && {{ mise_exec }} cargo nextest run --test postgres_comprehensive --features postgresql --no-capture

# Test PostgreSQL connection pooling
test-postgres-pooling:
    cd dbsurveyor-core && {{ mise_exec }} cargo nextest run --test postgres_connection_pooling --features postgresql --no-capture

# Test PostgreSQL versions and configurations
test-postgres-versions:
    cd dbsurveyor-core && {{ mise_exec }} cargo nextest run --test postgres_versions_and_configs --features postgresql --no-capture

# Test all PostgreSQL comprehensive tests
test-postgres-all:
    cd dbsurveyor-core && {{ mise_exec }} cargo nextest run --test postgres_comprehensive --test postgres_connection_pooling --test postgres_versions_and_configs --features postgresql --no-capture

# Test MySQL adapter
test-mysql:
    @{{ mise_exec }} cargo nextest run mysql --features mysql

# Test SQLite adapter
test-sqlite:
    @{{ mise_exec }} cargo nextest run sqlite --features sqlite

# =============================================================================
# COVERAGE
# =============================================================================

# Private helper: run cargo llvm-cov with proper setup
[private]
[unix]
_coverage +args:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf target/llvm-cov-target
    RUSTFLAGS="--cfg coverage" {{ mise_exec }} cargo llvm-cov -p dbsurveyor-core --lcov --output-path lcov.info {{ args }}

[private]
[windows]
_coverage +args:
    Remove-Item -Recurse -Force target/llvm-cov-target -ErrorAction SilentlyContinue
    $env:RUSTFLAGS = "--cfg coverage"; {{ mise_exec }} cargo llvm-cov -p dbsurveyor-core --lcov --output-path lcov.info {{ args }}

# Run coverage with threshold check
coverage:
    @just _coverage --fail-under-lines 55

# Run coverage for CI
coverage-ci:
    @just _coverage --fail-under-lines 55

# Run coverage with HTML report
coverage-html:
    @{{ mise_exec }} cargo llvm-cov --workspace --html --output-dir target/llvm-cov/html

# Run coverage report to terminal
coverage-report:
    @{{ mise_exec }} cargo llvm-cov --workspace

# Clean coverage artifacts
coverage-clean:
    @{{ mise_exec }} cargo llvm-cov clean --workspace

# =============================================================================
# SECURITY TESTING
# =============================================================================

# Test encryption capabilities (AES-GCM)
test-encryption:
    @{{ mise_exec }} cargo nextest run encryption --features encryption --no-capture

# Test offline operation (no network calls)
test-offline:
    @{{ mise_exec }} cargo nextest run offline

# Verify no credentials leak into outputs
test-credential-security:
    @{{ mise_exec }} cargo nextest run credential_security --no-capture

# Full security validation suite
security-full: lint test-encryption test-offline test-credential-security audit deny

# =============================================================================
# SECURITY AND AUDITING
# =============================================================================

# Run dependency audit
audit:
    @{{ mise_exec }} cargo audit

# Run cargo-deny checks
deny:
    @{{ mise_exec }} cargo deny check

# Run strict CI audit
audit-ci:
    @{{ mise_exec }} cargo audit --ignore RUSTSEC-2023-0071

# =============================================================================
# DOCUMENTATION
# =============================================================================

# Build rustdoc
doc:
    @{{ mise_exec }} cargo doc --features postgresql,sqlite,encryption,compression --no-deps

# Build and open documentation
doc-open:
    @{{ mise_exec }} cargo doc --features postgresql,sqlite,encryption,compression --no-deps --document-private-items --open

# Build complete documentation (mdBook + rustdoc)
[unix]
docs-build:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ mise_exec }} cargo doc --no-deps --document-private-items --target-dir docs/book/api-temp
    mkdir -p docs/book/api
    cp -r docs/book/api-temp/doc/* docs/book/api/
    rm -rf docs/book/api-temp
    cd docs && {{ mise_exec }} mdbook build

# Serve documentation locally with live reload
[unix]
docs-serve:
    cd docs && {{ mise_exec }} mdbook serve --open

# Clean documentation artifacts
[unix]
docs-clean:
    rm -rf docs/book target/doc

# Check documentation build
[unix]
docs-check:
    cd docs && {{ mise_exec }} mdbook build

# Generate and serve documentation
[unix]
docs: docs-build docs-serve

[windows]
docs:
    @echo "mdbook requires a Unix-like environment to serve"

# =============================================================================
# CI AND QUALITY ASSURANCE
# =============================================================================

# Full local CI parity check
ci-check: fmt-check lint test-ci coverage-ci audit-ci deny

# Fast CI check without coverage
ci-check-fast: fmt-check lint test-no-bench

# Full comprehensive checks
full-checks: fmt-check lint test-ci coverage audit-ci build-release

# Run benchmarks
bench:
    @{{ mise_exec }} cargo bench --features postgresql,sqlite,encryption,compression

# =============================================================================
# RELEASE
# =============================================================================

# Validate GoReleaser config and lint release workflow
release-check:
    @{{ mise_exec }} goreleaser check
    @{{ mise_exec }} actionlint .github/workflows/release.yml

# Local release dry-run (builds all targets, creates archives, skips publishing)
release-snapshot:
    @{{ mise_exec }} goreleaser release --snapshot --clean

# =============================================================================
# PACKAGING AND DEPLOYMENT
# =============================================================================

# Create airgap deployment package
[unix]
package-airgap: build-minimal
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p airgap-package
    cp target/release/dbsurveyor* airgap-package/ 2>/dev/null || true
    cp README.md airgap-package/

[windows]
package-airgap: build-minimal
    New-Item -ItemType Directory -Force -Path "airgap-package" | Out-Null
    Copy-Item "target/release/dbsurveyor*" "airgap-package/" -ErrorAction SilentlyContinue
    Copy-Item "README.md" "airgap-package/"

# =============================================================================
# MAINTENANCE
# =============================================================================

# Update dependencies
update:
    @{{ mise_exec }} cargo update

# Check for outdated dependencies
outdated:
    @{{ mise_exec }} cargo outdated

# Clean build artifacts
[unix]
clean:
    @{{ mise_exec }} cargo clean
    rm -f sbom.spdx.json sbom.json lcov.info

[windows]
clean:
    @{{ mise_exec }} cargo clean
    Remove-Item -Force sbom.spdx.json, sbom.json, lcov.info -ErrorAction SilentlyContinue

# =============================================================================
# DEVELOPMENT WORKFLOW
# =============================================================================

# Development workflow: format, lint, test
dev: format lint test

# Run the CLI tool
run *args:
    @{{ mise_exec }} cargo run --features postgresql,sqlite,encryption,compression -- {{ args }}

# Show project information
info:
    @echo "DBSurveyor - Security-First Database Documentation"
    @echo "==================================================="
    @{{ mise_exec }} rustc --version
    @{{ mise_exec }} cargo --version
    @echo ""
    @echo "Security Guarantees:"
    @echo "  - Offline-only operation (no network calls except to databases)"
    @echo "  - No telemetry or external reporting"
    @echo "  - No credentials in outputs"
    @echo "  - AES-GCM encryption with random nonce"
    @echo "  - Airgap compatibility"

# SECURITY NOTICE: This justfile enforces the following security guarantees:
# - NO NETWORK CALLS: All operations work offline after dependency download
# - NO TELEMETRY: Zero data collection or external reporting mechanisms
# - NO CREDENTIALS IN OUTPUTS: Database credentials never appear in any output
# - AES-GCM ENCRYPTION: Industry-standard with random nonce, embedded KDF params
# - AIRGAP COMPATIBLE: Full functionality in air-gapped environments
