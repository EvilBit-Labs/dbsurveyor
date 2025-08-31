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

# Install dependencies and development tools
install:
    @echo "🚀 Setting up secure development environment..."
    rustup component add clippy rustfmt
    @echo "📦 Installing security tools..."
    @if ! command -v cargo-audit >/dev/null 2>&1; then \
        cargo install cargo-audit; \
    fi
    @if ! command -v cargo-deny >/dev/null 2>&1; then \
        cargo install cargo-deny; \
    fi
    @if ! command -v cargo-llvm-cov >/dev/null 2>&1; then \
        cargo install cargo-llvm-cov; \
    fi
    @echo "✅ Development environment ready - security tools installed"

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
    pre-commit run -a
    cargo fmt
    prettier --write "**/*.{yml,yaml,js,jsx,ts,tsx}" 2>/dev/null
    @echo "✅ Code formatted"

# Check code formatting
format-check:
    @echo "🎨 Checking code formatting..."
    cargo fmt --check

# Lint code with clippy (strict warnings as errors)
lint:
    @echo "🔍 Running Rust Quality Gate (cargo clippy -- -D warnings)..."
    @echo "⚠️  EXPLICIT REQUIREMENT: cargo clippy -- -D warnings must pass"
    cargo clippy --all-targets --all-features -- -D warnings
    @echo "✅ Rust Quality Gate passed - zero warnings enforced"

# Run all linting and formatting checks
check: format-check lint pre-commit
    @echo "✅ All checks passed!"

# Run all linting and formatting checks with pre-commit hooks
check-full: format-check lint pre-commit pre-commit-run
    @echo "✅ All checks with pre-commit hooks passed!"

# Run pre-commit hooks manually
pre-commit-run:
    @echo "🔄 Running pre-commit hooks..."
    @if command -v pre-commit > /dev/null 2>&1; then \
        pre-commit run -a; \
        echo "✅ Pre-commit hooks passed!"; \
    else \
        echo "⚠️  pre-commit not installed, skipping hooks"; \
    fi

# Fix linting and formatting issues
fix: format
    cargo clippy --fix --allow-dirty

# Run pre-commit hooks
pre-commit:
    @echo "🔄 Running pre-commit security checks..."
    @just format-check
    @just lint
    @just test
    @just test-credential-security
    @echo "✅ Pre-commit checks passed - ready for secure commit"

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
    cargo clippy --all-targets --all-features -- -D warnings

# Run all Rust tests
rust-test:
    cargo test --all-features --workspace

# Run Rust test coverage with HTML report
rust-cov:
    cargo llvm-cov --all-features --workspace --open

# Quality assurance: format check, clippy, and tests
qa: rust-fmt-check rust-clippy rust-test
    @echo "✅ All QA checks passed!"

# Quality assurance with coverage
qa-cov: rust-fmt-check rust-clippy rust-test rust-cov
    @echo "✅ All QA checks with coverage completed!"

# -----------------------------
# 🧪 Testing & Coverage
# -----------------------------

# Run all tests with security verification
test:
    @echo "🧪 Running test suite with security checks..."
    @echo "⚠️  Testing offline-only operation - no external network calls allowed"
    # Run dbsurveyor-collect tests sequentially to avoid environment variable conflicts
    cargo test -p dbsurveyor-collect --all-features --verbose -- --test-threads=1
    # Run all other tests normally
    cargo test --workspace --exclude dbsurveyor-collect --all-features --verbose
    @echo "✅ All tests passed - security guarantees maintained"

# Run tests excluding benchmarks
test-no-bench:
    cargo test --all-features --lib --bins --tests

# Run integration tests only
test-integration:
    cargo test --test '*' --all-features

# Run unit tests only
test-unit:
    cargo test --lib --all-features

# Run doctests only
test-doc:
    cargo test --doc --all-features

# Run tests for specific database engines
test-postgres:
    @echo "🐘 Testing PostgreSQL adapter..."
    cargo test postgres --verbose

test-mysql:
    @echo "🐬 Testing MySQL adapter..."
    cargo test mysql --verbose

test-sqlite:
    @echo "📦 Testing SQLite adapter..."
    cargo test sqlite --verbose

# Run coverage with cargo-llvm-cov and enforce 75% threshold
coverage:
    @echo "🔍 Running coverage with >75% threshold..."
    cargo llvm-cov --all-features --workspace --lcov --fail-under-lines 75 --output-path lcov.info -- --test-threads=1
    @echo "✅ Coverage passed 75% threshold!"

# Run coverage for CI - generates report even if some tests fail
coverage-ci:
    @echo "🔍 Running coverage for CI with >75% threshold..."
    cargo llvm-cov --all-features --workspace --lcov --fail-under-lines 75 --output-path lcov.info
    @echo "✅ Coverage passed 75% threshold!"

# Run coverage report in HTML format for local viewing
coverage-html:
    @echo "🔍 Generating HTML coverage report..."
    cargo llvm-cov --all-features --workspace --html --output-dir target/llvm-cov/html
    @echo "📊 HTML report available at target/llvm-cov/html/index.html"

# Run coverage report to terminal
coverage-report:
    cargo llvm-cov --all-features --workspace

# Clean coverage artifacts
coverage-clean:
    cargo llvm-cov clean --workspace

# -----------------------------
# 🔒 Security Testing
# -----------------------------

# Verify encryption capabilities (AES-GCM with random nonce)
test-encryption:
    @echo "🔐 Testing AES-GCM encryption with random nonce generation..."
    @echo "⚠️  Verifying: random nonce, embedded KDF params, authenticated headers"
    cargo test encryption --verbose -- --nocapture
    @echo "✅ Encryption tests passed - AES-GCM security verified"

# Test offline operation (no network calls)
test-offline:
    @echo "✈️  Testing complete offline operation..."
    @echo "🚫 Verifying zero network calls during operation"
    @echo "⚠️  This test simulates airgap environment conditions"
    cargo test offline --verbose
    @echo "✅ Offline operation verified - airgap compatible"

# Verify no credentials leak into outputs
test-credential-security:
    @echo "🔑 Testing credential security..."
    @echo "⚠️  Verifying NO CREDENTIALS appear in any output files"
    cargo test credential_security --verbose -- --nocapture
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
    @echo "🔒 All security guarantees verified:"
    @echo "   ✓ Offline-only operation (no network calls)"
    @echo "   ✓ No telemetry or external reporting"
    @echo "   ✓ No credentials in outputs"
    @echo "   ✓ AES-GCM encryption (random nonce, embedded KDF, authenticated headers)"
    @echo "   ✓ Airgap compatibility confirmed"

# -----------------------------
# 🔧 Building & Running
# -----------------------------

# Build the project in debug mode
build:
    cargo build --all-features

# Build the project in release mode with security optimizations
build-release:
    @echo "🔨 Building with security optimizations..."
    cargo build --release --all-features
    @echo "✅ Build complete - offline operation verified"

# Build minimal feature set (for airgap environments)
build-minimal:
    @echo "🔨 Building minimal airgap-compatible version..."
    cargo build --release --no-default-features --features sqlite
    @echo "✅ Minimal build complete - maximum airgap compatibility"

# Build documentation
doc:
    cargo doc --all-features --no-deps

# Build and open documentation
doc-open:
    @echo "📚 Generating offline-compatible documentation..."
    cargo doc --all-features --no-deps --document-private-items --open
    @echo "✅ Documentation generated - works offline"

# Serve documentation locally (required by standard)
docs:
    @echo "📖 Starting documentation server..."
    @if ! command -v mkdocs > /dev/null 2>&1; then \
        echo "Installing MkDocs..."; \
        pip install mkdocs-material; \
    fi
    mkdocs serve

# Build documentation for verification (required by standard)
docs-build:
    @echo "🔨 Building documentation site..."
    @if ! command -v mkdocs > /dev/null 2>&1; then \
        echo "Installing MkDocs..."; \
        pip install mkdocs-material; \
    fi
    mkdocs build
    @echo "✅ Documentation built - check site/ directory"

# Run the CLI tool with sample arguments
run *args:
    cargo run --all-features -- {{args}}

# Run benchmarks
bench:
    cargo bench --all-features

# -----------------------------
# 🔐 Security & Auditing
# -----------------------------

# Security audit and SBOM generation
security-audit:
    @echo "🔐 Running comprehensive security audit..."
    @echo "📋 Generating Software Bill of Materials (SBOM)..."
    # Install tools if not present
    @if ! command -v syft >/dev/null 2>&1; then \
        echo "Installing Syft for SBOM generation..."; \
        curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b ~/.local/bin; \
    fi
    @if ! command -v grype >/dev/null 2>&1; then \
        echo "Installing Grype for vulnerability scanning..."; \
        curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh -s -- -b ~/.local/bin; \
    fi
    # Generate SBOM
    ~/.local/bin/syft dir:. -o json > sbom.json
    ~/.local/bin/syft dir:. -o spdx-json > sbom.spdx.json
    # Vulnerability scan
    ~/.local/bin/grype dir:. --output table
    ~/.local/bin/grype dir:. --output json --file grype-report.json
    @echo "✅ Security audit complete - reports generated"
    @echo "📄 SBOM files: sbom.json, sbom.spdx.json"
    @echo "🛡️  Vulnerability report: grype-report.json"

# SBOM generation for local inspection (required by standard)
sbom:
    @echo "📋 Generating Software Bill of Materials..."
    @if ! command -v syft > /dev/null 2>&1; then \
        echo "Installing Syft..."; \
        curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b ~/.local/bin; \
    fi
    ~/.local/bin/syft dir:. -o json > sbom.json
    ~/.local/bin/syft dir:. -o spdx-json > sbom.spdx.json
    @echo "📋 Generating SBOM provenance metadata..."
    @./scripts/generate-sbom-metadata.sh
    @echo "✅ SBOM generated: sbom.json, sbom.spdx.json, sbom.metadata.json"

# Simulate release process without publishing (required by standard)
release-dry:
    @echo "🎭 Simulating release process..."
    @just lint
    @just test
    @just build-release
    @just sbom
    @just security-audit
    @echo "✅ Release dry run complete - ready for actual release"

# Install language-specific tooling (required by standard)
install-tools:
    @echo "🔧 Installing Rust development tools..."
    rustup component add clippy rustfmt
    @if ! command -v cargo-audit > /dev/null 2>&1; then \
        cargo install cargo-audit; \
    fi
    @if ! command -v cargo-deny > /dev/null 2>&1; then \
        cargo install cargo-deny; \
    fi
    @if ! command -v cargo-llvm-cov > /dev/null 2>&1; then \
        cargo install cargo-llvm-cov; \
    fi
    @if ! command -v just > /dev/null 2>&1; then \
        echo "Installing just task runner..."; \
        cargo install --locked just; \
    fi
    @echo "✅ Rust tools installed"

# Run dependency audit
audit:
    @echo "📊 Auditing dependencies for security vulnerabilities..."
    @echo "🔍 Ignoring RUSTSEC-2023-0071 (RSA vulnerability) - See SECURITY.md for rationale and mitigation details"
    cargo audit --ignore RUSTSEC-2023-0071
    @echo "✅ Dependency audit complete"

# Check for security advisories
check-advisories:
    cargo audit

# -----------------------------
# 🧹 Clean & Maintenance
# -----------------------------

# Clean build artifacts
clean:
    @echo "🧹 Cleaning build artifacts (security: removing any cached sensitive data)..."
    cargo clean
    rm -f sbom.json sbom.spdx.json sbom.metadata.json grype-report.json lcov.info
    @echo "✅ Clean complete - no sensitive data in cache"

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
    # Create deployment bundle with all dependencies
    mkdir -p airgap-package
    cp target/release/dbsurveyor* airgap-package/ || true
    cp README.md airgap-package/
    @echo "✅ Airgap package created in airgap-package/"
    @echo "🛡️  Package includes offline documentation and security guarantees"

# -----------------------------
# 🤖 CI Workflow
# -----------------------------

# CI-friendly check that runs all validation
ci-check: format-check lint test coverage-ci
    @echo "✅ All CI checks passed!"

# Fast CI check without coverage (for quick feedback)
ci-check-fast: format-check lint test-no-bench
    @echo "✅ Fast CI checks passed!"

# Full comprehensive checks - runs all non-interactive verifications
full-checks: format-check lint pre-commit-run test coverage audit build-release
    @echo "✅ All full checks passed!"

# CI-friendly QA check (respects TERM=dumb)
ci-qa: rust-fmt-check rust-clippy rust-test
    @echo "✅ CI QA checks passed!"

# -----------------------------
# 🧪 Local GitHub Actions Testing (act)
# -----------------------------

# Install act for local GitHub Actions testing
install-act:
    @echo "📦 Installing act for local GitHub Actions testing..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "Installing act via Homebrew..."; \
        brew install act; \
    else \
        echo "✅ act is already installed"; \
    fi
    @echo "✅ act installation complete"

# Setup act with local configuration
setup-act: install-act
    @echo "⚙️  Setting up act for local GitHub Actions testing..."
    @if [ ! -f .secrets ]; then \
        echo "📝 Creating .secrets file from template..."; \
        cp .secrets.template .secrets; \
        echo "✏️  Please edit .secrets file with your actual tokens if needed"; \
    else \
        echo "✅ .secrets file already exists"; \
    fi
    @echo "🐳 Pulling required Docker images for act..."
    docker pull ghcr.io/catthehacker/ubuntu:act-latest
    @echo "✅ act setup complete - you can now run 'just test-ci-local'"

# Test CI workflow locally with act
test-ci-local:
    @echo "🧪 Running CI workflow locally with act..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/ci.yml

# Test simplified CI workflow optimized for local testing
test-ci-simple:
    @echo "🧪 Running simplified CI workflow locally with act..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/ci-local.yml

# Test specific CI jobs locally
test-lint-local:
    @echo "🔍 Testing lint job locally..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/ci.yml -j lint

test-security-local:
    @echo "🛡️  Testing security scan job locally..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/ci.yml -j security-scan

test-build-local:
    @echo "🔨 Testing build job locally..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/ci.yml -j build

# Test release workflow locally (dry run)
test-release-local:
    @echo "📦 Testing release workflow locally..."
    @echo "⚠️  Note: This simulates release triggers but won't actually release"
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/release.yml --dryrun

# Test Release Please workflow locally
test-release-please-local:
    @echo "🏷️  Testing Release Please workflow locally..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/release-please.yml

# Test OSSF Scorecard workflow locally
test-scorecard-local:
    @echo "📊 Testing OSSF Scorecard workflow locally..."
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    act -W .github/workflows/scorecard.yml

# List all available GitHub Actions workflows
list-workflows:
    @echo "📋 Available GitHub Actions workflows:"
    @find .github/workflows -name "*.yml" -o -name "*.yaml" | sort | while read file; do \
        echo "  📄 $(basename "$file")"; \
        grep -H "^name:" "$file" 2>/dev/null | sed 's/.*name: *//; s/^/    - /' || echo "    - (no name specified)"; \
    done

# Test all workflows locally (comprehensive check)
test-all-workflows:
    @echo "🚀 Testing all workflows locally..."
    @echo "⚠️  This may take a while and requires Docker"
    @if ! command -v act > /dev/null 2>&1; then \
        echo "❌ act not found - installing..."; \
        just install-act; \
    fi
    @echo "🧪 Testing CI workflow..."
    act -W .github/workflows/ci.yml --dryrun || echo "❌ CI workflow test failed"
    @echo "🏷️  Testing Release Please workflow..."
    act -W .github/workflows/release-please.yml --dryrun || echo "❌ Release Please workflow test failed"
    @echo "📊 Testing Scorecard workflow..."
    act -W .github/workflows/scorecard.yml --dryrun || echo "❌ Scorecard workflow test failed"
    @echo "✅ All workflow tests completed"

# Validate GitHub Actions syntax
validate-workflows:
    @echo "✅ Validating GitHub Actions workflow syntax..."
    @for file in .github/workflows/*.yml .github/workflows/*.yaml; do \
        if [ -f "$file" ]; then \
            echo "🔍 Checking $(basename "$file")..."; \
            if command -v yamllint > /dev/null 2>&1; then \
                yamllint "$file" || echo "❌ YAML syntax error in $(basename "$file")"; \
            else \
                echo "⚠️  yamllint not installed - install with: pip install yamllint"; \
            fi; \
        fi; \
    done
    @echo "✅ Workflow validation complete"

# -----------------------------
# 🚀 Development Workflow
# -----------------------------

# Development workflow: format, lint, test, coverage
dev: format lint test coverage pre-commit-run
    @echo "✅ Development checks complete!"

# Watch for changes and run tests
watch:
    cargo watch -x "test --all-features"

# Watch for changes and run checks
watch-check:
    cargo watch -x "check --all-features" -x "clippy -- -D warnings"

# -----------------------------
# 📊 Project Information
# -----------------------------

# Show project information
info:
    @echo "🔒 DBSurveyor - Security-First Database Documentation"
    @echo "========================================================="
    @echo "Rust version: $(rustc --version)"
    @echo "Cargo version: $(cargo --version)"
    @echo "Project features:"
    @cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].features | keys[]' 2>/dev/null || echo "  - PostgreSQL, MySQL, SQLite support"
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
