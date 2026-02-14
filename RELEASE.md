# Release Process

## Overview

DBSurveyor releases are automated through [GoReleaser](https://goreleaser.com/) with
cross-compilation via [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild).
Pushing a semver tag triggers the full pipeline.

## How to Release

1. Ensure all tests pass and the main branch is clean.
2. Bump the version in the workspace `Cargo.toml`.
3. Tag the release and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The `Release` workflow builds, signs, and publishes automatically.

## What Gets Published

Each release produces:

| Artifact | Description |
|----------|-------------|
| **Archives** | tar.gz (Linux/macOS), zip (Windows) containing both binaries |
| **Linux packages** | .deb, .rpm, .apk |
| **Checksums** | SHA256 checksum file |
| **Cosign signatures** | Keyless signatures on the checksum file |
| **SBOM** | Software Bill of Materials via Syft |
| **Homebrew formula** | Published to `EvilBit-Labs/homebrew-tap` |

## Supported Platforms

| OS | Architecture | Target Triple |
|----|-------------|---------------|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` |
| Linux | aarch64 | `aarch64-unknown-linux-gnu` |
| Linux (musl) | x86_64 | `x86_64-unknown-linux-musl` |
| macOS | x86_64 | `x86_64-apple-darwin` |
| macOS | aarch64 (Apple Silicon) | `aarch64-apple-darwin` |
| Windows | x86_64 | `x86_64-pc-windows-gnu` |

## Verification

### Checksums

```bash
sha256sum -c dbsurveyor_<VERSION>_checksums.txt
```

### Cosign Signature

```bash
cosign verify-blob \
  --certificate-identity-regexp="https://github.com/EvilBit-Labs/dbsurveyor/.*" \
  --certificate-oidc-issuer="https://token.actions.githubusercontent.com" \
  --certificate dbsurveyor_<VERSION>_checksums.txt.pem \
  --signature dbsurveyor_<VERSION>_checksums.txt.sig \
  dbsurveyor_<VERSION>_checksums.txt
```

## Installation

### Homebrew (macOS / Linux)

```bash
brew install EvilBit-Labs/tap/dbsurveyor
```

### Download Binary

Download the archive for your platform from the
[latest release](https://github.com/EvilBit-Labs/dbsurveyor/releases/latest)
and extract it.

### Linux Packages

Debian/Ubuntu:

```bash
sudo dpkg -i dbsurveyor_<VERSION>_amd64.deb
```

RHEL/Fedora:

```bash
sudo rpm -i dbsurveyor-<VERSION>.x86_64.rpm
```

Alpine (package is not signed with an Alpine key -- verify the checksum first):

```bash
sha256sum -c dbsurveyor_<VERSION>_checksums.txt
sudo apk add --allow-untrusted dbsurveyor-<VERSION>.apk
```

## Local Testing

Validate the GoReleaser configuration:

```bash
goreleaser check
```

Build a snapshot locally (no publish):

```bash
goreleaser build --snapshot --clean
```

Full dry-run release:

```bash
goreleaser release --snapshot --clean --skip=publish
```

## Troubleshooting

| Issue | Resolution |
|-------|------------|
| Build failures | Check Rust target support and cargo-zigbuild version |
| Signing failures | Verify `id-token: write` permission and Cosign version |
| Homebrew push fails | Verify `HOMEBREW_TAP_TOKEN` secret is set |
| Missing SBOM | Ensure Syft is installed in the workflow |
| Tag format rejected | Tags must match `v*.*.*` (e.g., `v0.1.0`) |

## Security

- All release artifacts are signed with Cosign (keyless OIDC via GitHub Actions)
- SBOM generated for supply chain transparency
- Checksums for integrity verification
- No credentials or telemetry in release artifacts
