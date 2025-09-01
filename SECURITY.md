# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

**CRITICAL**: Security vulnerabilities must be reported privately. Do NOT create public GitHub issues for security reports.

### Private Reporting Methods

#### 1. GitHub Security Advisories (Recommended)

- Go to this repository's **Security** tab
- Click **"Report a vulnerability"**
- Fill out the security advisory form
- This creates a private issue visible only to maintainers

#### 2. Email (Alternative)

- **Security Email**: <security@evilbitlabs.io>
- **Subject Format**: `[SECURITY] DBSurveyor - [Brief Description]`
- **PGP Key**: [Available upon request for sensitive reports]

#### 3. Private Vulnerability Reporting (PVR)

- Use GitHub's built-in PVR system
- Ensures complete confidentiality during investigation

### Required Information

When reporting a vulnerability, please include:

- **Description**: Clear explanation of the vulnerability
- **Steps to Reproduce**: Detailed reproduction steps (sanitized)
- **Impact Assessment**: Severity and potential consequences
- **Affected Versions**: Specific versions or commit ranges
- **Proposed Fix**: Any mitigation suggestions (if available)

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 1 week
- **Resolution**: Depends on severity and complexity
- **Public Disclosure**: Coordinated after fix is available

### Security Guarantees

DBSurveyor maintains the following security guarantees:

- **Offline-Only Operation**: No external network calls except to target databases
- **No Telemetry**: Zero data collection or external reporting
- **Credential Protection**: Database credentials never stored, logged, or output
- **Encryption**: AES-GCM with random nonce for sensitive data
- **Airgap Compatibility**: Full functionality in disconnected environments

### Responsible Disclosure

- Keep vulnerability details confidential until resolved
- Allow reasonable time for investigation and fix development
- Coordinate public disclosure with maintainers
- Do not exploit vulnerabilities beyond what's necessary for reporting

### Security Contacts

- **Primary**: <security@evilbitlabs.io>
- **Maintainer**: @unclesp1d3r
- **Response Time**: 48 hours acknowledgment

### Security Updates

Security updates are released as patch versions (e.g., 0.1.1, 0.1.2) and should be applied promptly. Critical vulnerabilities may result in immediate patch releases.

---

**Note**: This security policy is designed to protect users while ensuring vulnerabilities are addressed promptly and responsibly.
