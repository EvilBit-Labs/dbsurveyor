# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- [OK] Initial project setup with security-first approach
- [OK] Database schema documentation and analysis capabilities
- [OK] Offline-only operation with no telemetry
- [OK] AES-GCM encryption for sensitive outputs with Argon2id key derivation
- [OK] PostgreSQL adapter with comprehensive schema collection
- [OK] SQLite adapter support
- [OK] JSON Schema validation for all outputs (v1.0 format)
- [OK] Dual-binary architecture (collector + postprocessor)
- [OK] Zstandard compression support
- [OK] Comprehensive testing with nextest and testcontainers
- [OK] Security-focused development workflow with justfile
- [OK] CI/CD pipeline with security scanning
- [WIP] MySQL, MongoDB, SQL Server adapters (in development)
- [WIP] Advanced HTML report generation (placeholder)
- [WIP] SQL DDL reconstruction (placeholder)
- [WIP] Mermaid ERD diagram generation (placeholder)
- [WIP] Multi-database collection (planned)

### Security

- [OK] Enforced offline-only operation
- [OK] No credentials in output files with comprehensive sanitization
- [OK] Process isolation and audit-friendly separation (dual-binary model)
- [OK] AES-GCM authenticated encryption with random nonces
- [OK] Argon2id key derivation with secure parameters (64 MiB memory, 3 iterations)
- [OK] Memory-safe credential handling with zeroize
- [OK] JSON Schema validation prevents credential leakage
- [OK] Supply chain security controls (SBOM, vulnerability scanning)
- [WIP] Configurable data redaction patterns (planned)
