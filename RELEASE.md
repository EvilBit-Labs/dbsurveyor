# Release Process Documentation

## Overview

This document outlines the release process for DBSurveyor, including current capabilities and planned enhancements.

## Current Release Process

### Automated Release Pipeline

The release process is fully automated through GitHub Actions:

1. **Trigger**: Release is triggered when a GitHub release is published
2. **Build**: Cross-platform builds for all supported architectures
3. **Testing**: Full test suite execution before release
4. **Security**: SBOM generation, vulnerability scanning, and artifact signing
5. **Distribution**: GitHub releases with signed artifacts

### Supported Platforms

- **Linux**: x86_64, aarch64
- **macOS**: x86_64, aarch64
- **Windows**: x86_64, aarch64

### Artifact Types

- Binary executables for all platforms
- SHA256 checksums for integrity verification
- Cryptographic signatures using Cosign
- SBOM (Software Bill of Materials) in SPDX format
- Vulnerability scan reports

## Planned Enhancements

### Homebrew Integration

**Status**: Disabled (see `cargo-dist.toml`)

**TODO**: Enable Homebrew support for `brew install` capability

#### Required Tasks

1. **Enable Homebrew in cargo-dist**:

   ```toml
   [cargo-dist.homebrew]
   enabled = true
   ```

2. **Create GitHub Tap Repository**:

   - Repository: `EvilBit-Labs/homebrew-tap`
   - Purpose: Host Homebrew formula
   - Access: Public repository for formula distribution

3. **Update Release Workflow**:

   - Add Homebrew formula generation step
   - Configure formula metadata and dependencies
   - Set up automated formula publishing

4. **Formula Configuration**:

   - Define dependencies and requirements
   - Configure installation paths and permissions
   - Set up proper versioning and updates

#### Implementation Steps

1. **Create Homebrew Tap**:

   ```bash
   # Create tap repository
   gh repo create EvilBit-Labs/homebrew-tap --public --description "Homebrew tap for EvilBit Labs"
   ```

2. **Update Release Workflow**:
   Add to `.github/workflows/release.yml`:

   ```yaml
     - name: Generate Homebrew Formula
       if: matrix.target == 'x86_64-apple-darwin' # Only for macOS x86_64
       run: |
         cargo dist generate-homebrew-formula
         # Copy formula to tap repository
   ```

3. **Configure Formula Metadata**:

   - Package name: `dbsurveyor`
   - Description: "Secure database schema documentation tool"
   - Homepage: Project repository URL
   - License: MIT
   - Dependencies: None (self-contained binary)

#### Benefits

- **Easy Installation**: `brew install EvilBit-Labs/tap/dbsurveyor`
- **Automatic Updates**: `brew upgrade dbsurveyor`
- **Dependency Management**: Homebrew handles all dependencies
- **macOS Integration**: Native macOS package management

### Package Manager Support

**Future Enhancements**:

- **Snap**: Linux package distribution
- **Chocolatey**: Windows package management
- **Cargo**: Rust package registry (for library components)

## Release Checklist

### Pre-Release

- [ ] All tests passing
- [ ] Security audit clean
- [ ] Documentation updated
- [ ] Changelog updated
- [ ] Version bumped in Cargo.toml

### Release Process

- [ ] Create GitHub release
- [ ] Verify all artifacts generated
- [ ] Confirm signatures valid
- [ ] Test installation on target platforms
- [ ] Update documentation if needed

### Post-Release

- [ ] Monitor for issues
- [ ] Update release notes if needed
- [ ] Archive old releases (if applicable)

## Security Considerations

### Artifact Verification

All release artifacts are:

- **Signed**: Using Cosign with keyless OIDC
- **Checksummed**: SHA256 for integrity verification
- **Scanned**: Vulnerability scanning with Grype
- **Documented**: SBOM for supply chain transparency

### Distribution Security

- **HTTPS Only**: All downloads via HTTPS
- **Signed Releases**: GitHub releases with cryptographic signatures
- **Audit Trail**: Complete build and release audit trail
- **Reproducible**: Deterministic builds for verification

## Troubleshooting

### Common Issues

1. **Build Failures**: Check target support and dependencies
2. **Signing Issues**: Verify OIDC token and permissions
3. **Upload Failures**: Check GitHub token permissions
4. **Formula Issues**: Validate Homebrew formula syntax

### Support

For release-related issues:

- Check GitHub Actions logs
- Review security scan results
- Verify artifact integrity
- Contact maintainer if needed

---

**Last Updated**: 2025-08-30
**Next Review**: 2026-08-30
