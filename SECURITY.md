# Security Guarantees & Threat Model

## Core Security Guarantees

This document provides comprehensive security guarantees for the dbsurveyor project. These guarantees are **contractual commitments** enforced through code, CI/CD pipelines, and testing.

### 1. Offline-Only Operation

**GUARANTEE**: Zero external network communication after installation.

- [x] **NO NETWORK CALLS**: No HTTP requests, DNS queries, or external API calls during runtime
- [x] **NO TELEMETRY**: Absolutely no data collection, usage tracking, or external reporting
- [x] **NO AUTO-UPDATES**: Manual control over all software updates and dependency changes
- [x] **AIRGAP COMPATIBLE**: Full functionality in air-gapped environments

**Verification**:

- Automated tests verify no network calls during operation
- CI pipeline validates offline operation in isolated environments
- Manual verification: `just test-offline` confirms airgap compatibility

### 2. Data Security & Privacy

**GUARANTEE**: Sensitive information is never exposed in outputs.

- [x] **NO CREDENTIALS IN OUTPUTS**: Database credentials never appear in output files, logs, or debug information
- [x] **AES-GCM ENCRYPTION**: Industry-standard authenticated encryption for sensitive outputs
  - Random nonce generation for each encryption operation
  - Embedded Key Derivation Function (KDF) parameters using Argon2
  - Authenticated headers prevent tampering and ensure data integrity
  - 256-bit keys with configurable iteration counts for future-proofing
- [x] **SENSITIVE DATA WARNINGS**: Explicit warnings about sensitive data in sample outputs
- [x] **CONFIGURABLE REDACTION**: Pattern-based redaction for PII, SSN, credit cards, etc.

**Verification**:

- Credential security tests: `just test-credential-security`
- Encryption tests: `just test-encryption`
- Sample data security validated through automated testing

### 3. Supply Chain Security (Pipeline Standard Compliance)

**GUARANTEE**: Comprehensive security controls in CI/CD pipeline.

- [x] **CodeQL**: Static analysis for security vulnerabilities and code quality
- [x] **Syft**: Software Bill of Materials (SBOM) generation for all dependencies
- [x] **Grype**: Vulnerability scanning of all dependencies and container images
- [x] **FOSSA**: License compliance verification and legal risk assessment
- [x] **SIGNED RELEASES**: All binary releases are cryptographically signed
- [x] **SLSA ATTESTATION**: Supply chain provenance and integrity verification

**Verification**:

- Full security audit: `just security-full`
- CI pipeline runs all security controls on every commit
- SBOM and vulnerability reports generated for every release

### 4. Secure Development Practices

**GUARANTEE**: Security-first development methodology.

- [x] **STRICT LINTING**: `cargo clippy -- -D warnings` enforces zero warnings
- [x] **MEMORY SAFETY**: No `unsafe` code blocks allowed (Rust memory safety)
- [x] **INPUT VALIDATION**: All inputs validated and sanitized
- [x] **ERROR HANDLING**: No panic-based error handling in production code
- [x] **MINIMAL DEPENDENCIES**: Carefully audited, minimal dependency set

**Verification**:

- Pre-commit hooks enforce security standards: `just pre-commit`
- Automated dependency auditing: `just audit`
- Memory safety guaranteed by Rust compiler and `unsafe` code prohibition

## Threat Model

### Threats We Mitigate

#### 1. Data Exfiltration

**Threat**: Sensitive database information leaked through outputs or network calls.
**Mitigation**:

- No network calls after installation
- Credential exclusion from all outputs
- Optional encryption of output files
- Configurable data redaction patterns

#### 2. Supply Chain Attacks

**Threat**: Compromised dependencies or build process.
**Mitigation**:

- SBOM generation for all dependencies
- Vulnerability scanning with Grype
- Signed releases with provenance attestation
- Minimal, audited dependency set

#### 3. Air-Gap Compromise

**Threat**: Tools that require network connectivity compromise air-gapped environments.
**Mitigation**:

- Complete offline operation capability
- Self-contained binaries with no runtime dependencies
- Offline documentation and help systems
- Airgap deployment packages

#### 4. Credential Exposure

**Threat**: Database credentials appear in logs, outputs, or temporary files.
**Mitigation**:

- Environment variable credential sourcing
- Secure memory handling with automatic cleanup
- No credential storage in any output format
- Comprehensive credential security testing

### Threats Outside Scope

#### 1. Physical Security

- **Not Covered**: Physical access to systems running dbsurveyor
- **Recommendation**: Follow organizational physical security policies

#### 2. Database Server Security

- **Not Covered**: Security of target database servers
- **Recommendation**: Follow database security best practices independently

#### 3. Host System Compromise

- **Not Covered**: Compromise of the system running dbsurveyor
- **Recommendation**: Standard endpoint security measures apply

## Cryptographic Details

### AES-GCM Encryption Implementation

When the `--encrypt` flag is used, dbsurveyor implements authenticated encryption using AES-256-GCM:

```text
Encryption Process:
1. Generate cryptographically secure random 96-bit nonce
2. Derive 256-bit key using Argon2id with embedded parameters:
   - Memory cost: 65536 KiB (configurable)
   - Time cost: 3 iterations (configurable)
   - Parallelism: 4 threads (configurable)
   - Salt: 32-byte random salt per operation
3. Encrypt plaintext using AES-256-GCM with nonce and derived key
4. Embed KDF parameters, salt, and nonce in output file header
5. Append authentication tag to ensure integrity

Output Format:
[KDF_PARAMS][SALT][NONCE][ENCRYPTED_DATA][AUTH_TAG]
```

**Security Properties**:

- **Confidentiality**: AES-256 provides strong encryption
- **Integrity**: GCM authentication tag prevents tampering
- **Authenticity**: Embedded parameters prevent substitution attacks
- **Uniqueness**: Random nonce ensures unique encryption per operation
- **Forward Secrecy**: Keys derived per-operation, not reused

### Key Derivation Parameters

Default Argon2id parameters (configurable via CLI):

```text
Memory Cost: 65536 KiB (64 MiB)
Time Cost: 3 iterations
Parallelism: 4 threads
Salt Length: 32 bytes (256 bits)
Output Length: 32 bytes (256 bits)
```

These parameters provide strong resistance against:

- Brute force attacks
- Dictionary attacks
- Rainbow table attacks
- GPU-based attacks
- ASIC-based attacks

## Security Testing

### Automated Security Tests

1. **Credential Security Tests**

   ```bash
   just test-credential-security
   ```

   Verifies no credentials appear in any output files or logs.

2. **Encryption Security Tests**

   ```bash
   just test-encryption
   ```

   Validates AES-GCM implementation with random nonce generation.

3. **Offline Operation Tests**

   ```bash
   just test-offline
   ```

   Confirms zero network calls during operation.

4. **Full Security Suite**

   ```bash
   just security-full
   ```

   Runs complete security validation including external tools.

### Manual Security Verification

1. **Network Isolation Test**

   - Disconnect network interface
   - Run dbsurveyor operations
   - Verify successful operation without connectivity

2. **Output Analysis**

   - Generate various output formats
   - Search for credential patterns using regex
   - Verify redaction is working correctly

3. **Encryption Verification**

   - Encrypt output with known password
   - Decrypt and verify integrity
   - Confirm nonce uniqueness across operations

## Security Incident Response

### Vulnerability Disclosure

If you discover a security vulnerability in dbsurveyor:

1. **DO NOT** open a public GitHub issue
2. **Email**: security@[project-domain] with details
3. **Include**: Steps to reproduce, impact assessment, proposed fix (if available)
4. **Response Time**: We commit to acknowledging within 48 hours

### Security Update Process

1. **Assessment**: Evaluate severity and impact
2. **Fix Development**: Develop and test security fix
3. **Advisory**: Prepare security advisory with CVE if applicable
4. **Release**: Expedited release process for critical issues
5. **Disclosure**: Coordinate public disclosure with reporter

## Compliance & Auditing

### Audit Trail

dbsurveyor maintains comprehensive audit trails:

- **Build Provenance**: SLSA attestation for all releases
- **Dependency Tracking**: SBOM for all third-party components
- **Vulnerability History**: Historical Grype reports
- **Code Changes**: Git commit history with signed commits

### Compliance Frameworks

This security model supports compliance with:

- **SOX**: Audit trail and data integrity requirements
- **GDPR**: Data privacy and protection mechanisms
- **HIPAA**: Technical safeguards for protected health information
- **PCI DSS**: Data protection for payment card information
- **FedRAMP**: Federal security requirements for cloud services

### Documentation Requirements

All security guarantees are:

- [x] **Documented**: Comprehensive documentation in this file
- [x] **Tested**: Automated tests verify guarantees
- [x] **Enforced**: CI/CD pipeline enforces compliance
- [x] **Auditable**: Full audit trail maintained

## Known Vulnerabilities

### RUSTSEC-2023-0071: RSA Marvin Attack Vulnerability

**Affected Component**: MySQL support in `dbsurveyor-collect`
**Status**: Mitigated by default configuration
**Severity**: Medium (5.9)

#### Issue Description

The RSA crate v0.9.8 used by SQLx for MySQL connections contains a vulnerability that may allow key recovery through timing side-channels (Marvin Attack).

#### Mitigation

**Default Configuration**: MySQL support is **disabled by default** to prevent this vulnerability.

**For users who need MySQL support**:

1. Explicitly enable MySQL feature: `cargo build --features mysql`
2. **Security Recommendation**: Use PostgreSQL or SQLite instead when possible
3. If MySQL is required, ensure connections use:
   - Strong network security (VPN, private networks)
   - Regular key rotation
   - Monitoring for timing attacks

#### Alternative Solutions

1. **PostgreSQL**: Full support, no security vulnerabilities
2. **SQLite**: Full support, no network dependencies
3. **MongoDB**: Full support with secure authentication

#### Status Updates

This vulnerability will be resolved when:

- SQLx updates to use a patched RSA crate version
- Alternative TLS implementations are available
- The RSA crate maintainers release a security fix

For the latest security status, run:

```bash
cargo audit
```

## Security Warnings

### Critical Warnings

**WARNING - SAMPLE DATA**: Sample data may contain sensitive information. Always review outputs before sharing and use `--redact-samples` flag when appropriate.

**WARNING - CREDENTIAL MANAGEMENT**: Never pass credentials via command-line arguments. Use environment variables or secure configuration files with appropriate permissions.

**WARNING - OUTPUT SHARING**: Even with redaction, carefully review all outputs before sharing outside your organization.

### Best Practices

1. **Use Encryption**: Always use `--encrypt` flag for sensitive environments
2. **Limit Samples**: Use `--sample 0` or `--no-data` for maximum security
3. **Regular Updates**: Keep dbsurveyor updated for latest security fixes
4. **Audit Outputs**: Regularly audit output files for unintended data exposure
5. **Secure Storage**: Store output files in secure, access-controlled locations

## Security Resources

- **Security Policy**: [SECURITY.md](SECURITY.md) (this file)
- **CI Security Pipeline**: [.github/workflows/ci.yml](.github/workflows/ci.yml)
- **Security-First Development**: [justfile](justfile)
- **Dependency Security**: [Cargo.toml](Cargo.toml) with security lints
- **Pipeline Standard Compliance**: All security controls documented in CI workflow

---

**Last Updated**: 2024-12-19
**Security Review**: Required before each major release
**Next Review**: 2025-01-19
