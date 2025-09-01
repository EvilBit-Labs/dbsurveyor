# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial project setup with security-first approach
- Database schema documentation and analysis capabilities
- Offline-only operation with no telemetry
- AES-GCM encryption for sensitive outputs
- Support for PostgreSQL, MySQL, SQLite, and MongoDB
- Versioned DatabaseSurvey v1.0 interchange format
- Comprehensive security controls and linting
- CI/CD pipeline with security scanning

### Security

- Enforced offline-only operation
- No credentials in output files
- Process isolation and audit-friendly separation (dual-binary model)
- AES-GCM authenticated encryption
- Configurable data redaction patterns
- Supply chain security controls
