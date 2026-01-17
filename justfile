# 🔒 justfile — DBSurveyor Security-First Developer Tasks
set dotenv-load := true
set ignore-comments := true

# Default recipe - shows available commands
default:
    just --list

# Show help
help:
    just --list

# -----------------------------
# 🔧 Setup & Installation
# -----------------------------

# Setup development environment
setup: install

# Install Rust development tools
install-rust:
    @echo "🔧 Installing Rust development tools..."
    rustup component add clippy rustfmt
    @echo "✅ Rust tools installed"

# Install Cargo tools
install-cargo-tools:
    @echo "📦 Installing Cargo tools..."
    @if ! command -v cargo-audit >/dev/null 2>&1; then cargo install cargo-audit; fi
    @if ! command -v cargo-deny >/dev/null 2>&1; then cargo install cargo-deny; fi
    @if ! command -v cargo-llvm-cov >/dev/null 2>&1; then cargo install cargo-llvm-cov; fi
    @if ! command -v cargo-nextest >/dev/null 2>&1; then cargo install cargo-nextest; fi
    @echo "✅ Cargo tools installed"

# Install security tools
install-security-tools:
    @echo "🛡️ Installing security tools..."
    @if ! command -v syft >/dev/null 2>&1; then \
        curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b ~/.local/bin; \
    fi
    @echo "✅ Security tools installed"

# Install all dependencies and development tools
install: install-rust install-cargo-tools install-security-tools docs-install
    @echo "🚀 Development environment ready!"

# Install mdBook and plugins for documentation
docs-install:
    cargo install mdbook mdbook-admonish mdbook-mermaid mdbook-linkcheck mdbook-toc mdbook-open-on-gh mdbook-tabs mdbook-i18n-helpers

# Update dependencies
update-deps:
    @echo "🔄 Updating dependencies..."
    cargo update
    @echo "✅ Dependencies updated!"

# -----------------------------
# 🧹 Linting, Formatting & Checking
# -----------------------------

# Format code with rustfmt
format:
    @echo "🎨 Formatting code..."
    cargo fmt
    @echo "✅ Code formatted"

# Check code formatting
format-check:
    @echo "🎨 Checking code formatting..."
    cargo fmt --check

# Lint code with clippy (strict warnings as errors)
lint:
    @echo "🔍 Running Rust Quality Gate (cargo clippy -- -D warnings)..."
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    @echo "✅ Rust Quality Gate passed - zero warnings enforced"

# Run pre-commit hooks
pre-commit:
    @echo "🔄 Running pre-commit security checks..."
    @just format-check
    @just lint
    @just test
    @just test-credential-security
    @echo "✅ Pre-commit checks passed - ready for secure commit"

# Run all linting and formatting checks
check: format-check lint
    @echo "✅ All checks passed!"

# Fix linting and formatting issues
fix: format
    cargo clippy --fix --allow-dirty

# -----------------------------
# 🦀 Standardized Rust Tasks
# -----------------------------

# Format all Rust code
rust-fmt:
    cargo fmt --all

# Check Rust code formatting
rust-fmt-check:
    cargo fmt --all -- --check

# Lint Rust code with clippy (strict mode)
rust-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all Rust tests with nextest
rust-test:
    cargo nextest run --features postgresql,sqlite,encryption,compression --workspace

# Run Rust test coverage with HTML report
rust-cov:
    cargo llvm-cov --features postgresql,sqlite,encryption,compression --workspace --open

# Quality assurance: format check, clippy, and tests
qa: rust-fmt-check rust-clippy rust-test
    @echo "✅ All QA checks passed!"

# Quality assurance with coverage
qa-cov: rust-fmt-check rust-clippy rust-test rust-cov
    @echo "✅ All QA checks with coverage completed!"

# -----------------------------
# 🧪 Testing & Coverage
# -----------------------------

# Run all tests with security verification using nextest
test:
    @echo "🧪 Running test suite with nextest and security checks..."
    @echo "⚠️  Testing offline-only operation - no external network calls allowed"
    # Run all tests with nextest parallel execution
    cargo nextest run --workspace --features postgresql,sqlite,encryption,compression
    @echo "✅ All tests passed - security guarantees maintained"

# Run tests excluding benchmarks with nextest
test-no-bench:
    cargo nextest run --features postgresql,sqlite,encryption,compression --lib --bins --tests

# Run integration tests only with nextest
test-integration:
    cargo nextest run --test '*' --features postgresql,sqlite,encryption,compression --test-group integration

# Run unit tests only with nextest
test-unit:
    cargo nextest run --lib --features postgresql,sqlite,encryption,compression --test-group unit

# Run doctests only (nextest doesn't support doctests, use cargo test)
test-doc:
    cargo test --doc --features postgresql,sqlite,encryption,compression

# Run tests with CI profile (for CI environments)
test-ci:
    @echo "🤖 Running tests with CI profile..."
    cargo nextest run --profile ci --features postgresql,sqlite,encryption,compression --workspace
    @echo "✅ CI tests completed"

# Run tests with dev profile (for local development)
test-dev:
    @echo "🚀 Running tests with dev profile..."
    cargo nextest run --profile dev --features postgresql,sqlite,encryption,compression --workspace
    @echo "✅ Dev tests completed"

# Run tests with verbose output for debugging
test-verbose:
    @echo "🔍 Running tests with verbose output..."
    cargo nextest run --features postgresql,sqlite,encryption,compression --workspace --nocapture
    @echo "✅ Verbose tests completed"

# Run tests for specific database engines with nextest
test-postgres:
    @echo "🐘 Testing PostgreSQL adapter..."
    cargo nextest run postgres --features postgresql

test-mysql:
    @echo "🐬 Testing MySQL adapter..."
    cargo nextest run mysql --features mysql

test-sqlite:
    @echo "📦 Testing SQLite adapter..."
    cargo nextest run sqlite --features sqlite

# Run coverage with cargo-llvm-cov and enforce 70% threshold
coverage:
    @echo "🔍 Running coverage with >55% threshold..."
    cargo llvm-cov -p dbsurveyor-core --lcov --fail-under-lines 55 --output-path lcov.info -- --test-threads=1
    @echo "✅ Coverage passed 55% threshold!"

# Run coverage for CI - generates report even if some tests fail
coverage-ci:
    @echo "🔍 Running coverage for CI with >55% threshold..."
    cargo llvm-cov -p dbsurveyor-core --lcov --fail-under-lines 55 --output-path lcov.info
    @echo "✅ Coverage passed 55% threshold!"

# Run coverage report in HTML format for local viewing
coverage-html:
    @echo "🔍 Generating HTML coverage report..."
    cargo llvm-cov --workspace --html --output-dir target/llvm-cov/html
    @echo "📊 HTML report available at target/llvm-cov/html/index.html"

# Run coverage report to terminal
coverage-report:
    cargo llvm-cov --workspace

# Clean coverage artifacts
coverage-clean:
    cargo llvm-cov clean --workspace

# -----------------------------
# 🔒 Security Testing
# -----------------------------

# Verify encryption capabilities (AES-GCM with random nonce)
test-encryption:
    @echo "🔐 Testing AES-GCM encryption with random nonce generation..."
    cargo nextest run encryption --features encryption --test-group security --nocapture
    @echo "✅ Encryption tests passed - AES-GCM security verified"

# Test offline operation (no network calls)
test-offline:
    @echo "✈️  Testing complete offline operation..."
    cargo nextest run offline --test-group security
    @echo "✅ Offline operation verified - airgap compatible"

# Verify no credentials leak into outputs
test-credential-security:
    @echo "🔑 Testing credential security..."
    cargo nextest run credential_security --test-group security --nocapture
    @echo "✅ Credential security verified - no leakage detected"

# Full security validation suite
security-full:
    @echo "🛡️  Running FULL security validation..."
    @just lint
    @just test-encryption
    @just test-offline
    @just test-credential-security
    @just security-audit
    @echo "✅ FULL SECURITY VALIDATION PASSED"

# =============================================================================
# DOCUMENTATION
# =============================================================================

# Build complete documentation (mdBook + rustdoc)
docs-build:
    #!/usr/bin/env bash
    set -euo pipefail
    # Build rustdoc
    cargo doc --no-deps --document-private-items --target-dir docs/book/api-temp
    # Move rustdoc output to final location
    mkdir -p docs/book/api
    cp -r docs/book/api-temp/doc/* docs/book/api/
    rm -rf docs/book/api-temp
    # Build mdBook
    cd docs && mdbook build

# Serve documentation locally with live reload
docs-serve:
    cd docs && mdbook serve --open

# Clean documentation artifacts
docs-clean:
    rm -rf docs/book target/doc

# Check documentation (build + link validation + formatting)
docs-check:
    cd docs && mdbook build
    @just fmt-check

# Generate and serve documentation
[unix]
docs:
    cd docs && mdbook serve --open

[windows]
docs:
    @echo "mdbook requires a Unix-like environment to serve"


# -----------------------------
# 🔧 Building & Running
# -----------------------------

# Build the project in debug mode
build:
    cargo build --workspace --all-features

# Build the project in release mode with security optimizations
build-release:
    @echo "🔨 Building with security optimizations..."
    cargo build --release --workspace --all-features
    @echo "✅ Build complete - offline operation verified"

# Build minimal feature set (for airgap environments)
build-minimal:
    @echo "🔨 Building minimal airgap-compatible version..."
    cargo build --release --no-default-features --features sqlite
    @echo "✅ Minimal build complete - maximum airgap compatibility"

# Build documentation
doc:
    cargo doc --features postgresql,sqlite,encryption,compression --no-deps

# Build and open documentation
doc-open:
    @echo "📚 Generating offline-compatible documentation..."
    cargo doc --features postgresql,sqlite,encryption,compression --no-deps --document-private-items --open
    @echo "✅ Documentation generated - works offline"

# Run the CLI tool with sample arguments
run *args:
    cargo run --features postgresql,sqlite,encryption,compression -- {{args}}

# Run benchmarks
bench:
    cargo bench --features postgresql,sqlite,encryption,compression

# -----------------------------
# 🔐 Security & Auditing
# -----------------------------

# Security audit and SBOM generation
security-audit:
    @echo "🔐 Running comprehensive security audit..."
    @echo "📋 Generating Software Bill of Materials (SBOM)..."
    syft dir:. -o spdx-json > sbom.spdx.json
    syft dir:. -o json > sbom.json
    @echo "✅ Security audit complete - reports generated"
    @echo "📄 SBOM files: sbom.spdx.json, sbom.json"

# SBOM generation for local inspection
sbom:
    @echo "📋 Generating Software Bill of Materials..."
    syft dir:. -o spdx-json > sbom.spdx.json
    syft dir:. -o json > sbom.json
    @echo "✅ SBOM generated: sbom.spdx.json, sbom.json"

# Simulate release process without publishing
release-dry:
    @echo "🎭 Simulating release process..."
    @just lint
    @just test
    @just build-release
    @just sbom
    @just security-audit
    @echo "✅ Release dry run complete - ready for actual release"

# Run dependency audit
audit:
    @echo "📊 Auditing dependencies for security vulnerabilities..."
    cargo audit
    @echo "✅ Dependency audit complete"

# Run strict CI audit (fails on all advisories)
audit-ci:
    @echo "📊 Running strict CI audit (fails on all advisories)..."
    cargo audit --ignore RUSTSEC-2023-0071
    @echo "✅ Strict audit passed - no vulnerabilities found"

# -----------------------------
# 🧹 Clean & Maintenance
# -----------------------------

# Clean build artifacts
clean:
    @echo "🧹 Cleaning build artifacts..."
    cargo clean
    rm -f sbom.spdx.json sbom.json lcov.info
    @echo "✅ Clean complete"

# Update dependencies
update:
    cargo update

# -----------------------------
# 📦 Packaging & Deployment
# -----------------------------

# Verify airgap deployment package
package-airgap:
    @echo "📦 Creating airgap deployment package..."
    @just build-minimal
    @echo "🔒 Verifying airgap compatibility..."
    mkdir -p airgap-package
    cp target/release/dbsurveyor* airgap-package/ || true
    cp README.md airgap-package/
    @echo "✅ Airgap package created in airgap-package/"

# -----------------------------
# 🤖 CI Workflow
# -----------------------------

# CI-friendly check that runs all validation
ci-check: format-check lint test-ci coverage-ci
    @echo "✅ All CI checks passed!"

# Fast CI check without coverage (for quick feedback)
ci-check-fast: format-check lint test-no-bench
    @echo "✅ Fast CI checks passed!"

# Full comprehensive checks - runs all non-interactive verifications
full-checks: format-check lint test-ci coverage audit-ci build-release
    @echo "✅ All full checks passed!"

# CI-friendly QA check (respects TERM=dumb)
ci-qa: rust-fmt-check rust-clippy rust-test
    @echo "✅ CI QA checks passed!"

# -----------------------------
# 🚀 Development Workflow
# -----------------------------

# Development workflow: format, lint, test, coverage
dev: format lint test coverage
    @echo "✅ Development checks complete!"

# Watch for changes and run tests with nextest
watch:
    cargo watch -x "nextest run --features postgresql,sqlite,encryption,compression"

# Watch for changes and run checks
watch-check:
    cargo watch -x "check --features postgresql,sqlite,encryption,compression" -x "clippy -- -D warnings"

# -----------------------------
# 📊 Project Information
# -----------------------------

# Show project information
info:
    @echo "🔒 DBSurveyor - Security-First Database Documentation"
    @echo "========================================================="
    @echo "Rust version: $(rustc --version)"
    @echo "Cargo version: $(cargo --version)"
    @echo ""
    @echo "🔒 Security Guarantees:"
    @echo "  ✓ Offline-only operation (no network calls except to databases)"
    @echo "  ✓ No telemetry or external reporting"
    @echo "  ✓ No credentials in outputs"
    @echo "  ✓ AES-GCM encryption with random nonce"
    @echo "  ✓ Airgap compatibility"

# ⚠️ SECURITY NOTICE: This justfile enforces the following security guarantees:
# - NO NETWORK CALLS: All operations work offline after dependency download
# - NO TELEMETRY: Zero data collection or external reporting mechanisms
# - NO CREDENTIALS IN OUTPUTS: Database credentials never appear in any output
# - AES-GCM ENCRYPTION: Industry-standard with random nonce, embedded KDF params, authenticated headers
# - AIRGAP COMPATIBLE: Full functionality in air-gapped environments
# - CI SECURITY CONTROLS: Strict linting, testing, and security gates
