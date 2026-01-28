# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- ✅ Initial project setup with security-first approach
- ✅ Database schema documentation and analysis capabilities
- ✅ Offline-only operation with no telemetry
- ✅ AES-GCM encryption for sensitive outputs with Argon2id key derivation
- ✅ PostgreSQL adapter with comprehensive schema collection
- ✅ SQLite adapter support
- ✅ JSON Schema validation for all outputs (v1.0 format)
- ✅ Dual-binary architecture (collector + postprocessor)
- ✅ Zstandard compression support
- ✅ Comprehensive testing with nextest and testcontainers
- ✅ Security-focused development workflow with justfile
- ✅ CI/CD pipeline with security scanning
- 🚧 MySQL, MongoDB, SQL Server adapters (in development)
- 🚧 Advanced HTML report generation (placeholder)
- 🚧 SQL DDL reconstruction (placeholder)
- 🚧 Mermaid ERD diagram generation (placeholder)
- 🚧 Multi-database collection (planned)

### Security

- ✅ Enforced offline-only operation
- ✅ No credentials in output files with comprehensive sanitization
- ✅ Process isolation and audit-friendly separation (dual-binary model)
- ✅ AES-GCM authenticated encryption with random nonces
- ✅ Argon2id key derivation with secure parameters (64 MiB memory, 3 iterations)
- ✅ Memory-safe credential handling with zeroize
- ✅ JSON Schema validation prevents credential leakage
- ✅ Supply chain security controls (SBOM, vulnerability scanning)
- 🚧 Configurable data redaction patterns (planned)
