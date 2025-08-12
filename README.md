# DBSurveyor

## Secure, Offline-First Database Analysis and Documentation Toolchain

**Author**: UncleSp1d3r

Toolchain for surveying database servers, extracting schema and sample data, and generating portable structured output.

## Security & Compliance Guarantees

### Offline-Only Operation

- **NO NETWORK CALLS**: Operates completely offline after initial installation
- **NO TELEMETRY**: Zero data collection, usage tracking, or external reporting
- **NO AUTO-UPDATES**: Manual control over all software updates
- **AIRGAP COMPATIBLE**: Full functionality in air-gapped environments

### Data Security

- **NO CREDENTIALS IN OUTPUTS**: Database credentials never stored in output files
- **AES-GCM ENCRYPTION**: Industry-standard authenticated encryption for sensitive outputs
  - Random nonce generation for each encryption operation
  - Embedded Key Derivation Function (KDF) parameters in encrypted files
  - Authenticated headers prevent tampering and ensure data integrity
  - 256-bit keys derived from user-provided passwords using PBKDF2/Argon2
- **SENSITIVE DATA WARNINGS**: Explicit warnings about sensitive data in sample outputs
- **CONFIGURABLE REDACTION**: Pattern-based redaction for PII, SSN, credit cards, etc.

### CI Security Controls (Per Pipeline Standard)

- **CodeQL**: Static analysis for security vulnerabilities
- **Syft**: Software Bill of Materials (SBOM) generation
- **Grype**: Vulnerability scanning of dependencies
- **FOSSA**: License compliance verification
- **Rust Quality Gate**: `cargo clippy -- -D warnings` enforced
- **Signed Releases**: All binaries cryptographically signed
- **Supply Chain Security**: SLSA attestation and provenance

### Development and Review Process

- **Code Review**: Primary tool is [CodeRabbit.ai](https://coderabbit.ai) for intelligent, conversational code analysis
- **GitHub Copilot**: Automatic reviews are disabled; CodeRabbit.ai provides superior review capabilities
- **Single Maintainer**: Streamlined development process with direct maintainer access
- **OpenAPI Generator**: Future HTTP client development will use OpenAPI Generator for Rust code generation

WARNING: Sample data may contain sensitive information. Use `--redact-samples` flag and review outputs before sharing.

## Exceptions

### FOSSA License Scanning Integration
- **Rule:** FOSSA GitHub App integration with PR enforcement requirement
- **Status:** Pending - requires GitHub App installation and configuration
- **Rationale:** FOSSA integration requires organization-level GitHub App setup
- **Duration:** Until FOSSA GitHub App is configured for the repository
- **Compensating Controls:** 
  - Manual license review via cargo-deny.toml configuration
  - Pre-commit license validation hooks
  - Regular dependency auditing with cargo-audit
  - License information included in generated SBOMs
- **Tracking:** Will be resolved once FOSSA GitHub App is installed

### Migration Status
- **Renovate:** ✅ Configured (replaced Dependabot)
- **Release Please:** ✅ Configured 
- **SLSA Provenance:** ✅ Configured
- **Cosign Signing:** ✅ Configured
- **CodeQL Analysis:** ✅ Configured
- **OSSF Scorecard:** ✅ Configured
- **MkDocs Documentation:** ✅ Configured
- **Local GitHub Actions Testing:** ✅ Configured with `act`

## Local Development and Testing

### GitHub Actions Testing with `act`

This project supports local testing of GitHub Actions workflows using [`act`](https://github.com/nektos/act):

```bash
# Setup act for local testing
just setup-act

# Test the entire CI workflow locally
just test-ci-local

# Test specific jobs
just test-lint-local
just test-security-local
just test-build-local

# Test release workflows (dry run)
just test-release-local
just test-release-please-local

# List all available workflows
just list-workflows

# Validate workflow syntax
just validate-workflows
```

### Key Development Commands

```bash
# Development workflow (format, lint, test, coverage)
just dev

# Security-focused development cycle
just security-full

# Local/CI parity - run the same checks as CI
just ci-check

# Pre-commit validation
just pre-commit
```
