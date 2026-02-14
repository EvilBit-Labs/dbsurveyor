# Release Process

## Overview

DBSurveyor releases are automated through [GoReleaser](https://goreleaser.com/) with
cross-compilation via [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild).
Pushing a semver tag triggers the full pipeline.

Each release produces **multiple variants** of the `dbsurveyor-collect` binary, one per
database driver, plus an all-features build. This lets operators download only the
driver they need.

## How to Release

1. Ensure all tests pass and the main branch is clean.
2. Bump the version in the workspace `Cargo.toml`.
3. Tag the release and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The `Release` workflow builds, signs, and publishes automatically.

## Release Variants

Each variant archive contains both `dbsurveyor` (postprocessor) and `dbsurveyor-collect`
(collector). The collector is built with the specified database driver(s). All variants
include compression and encryption support.

| Variant | Database Drivers | Archive Name Pattern |
|---------|-----------------|---------------------|
| `all` | PostgreSQL, MySQL, SQLite, MongoDB, MSSQL | `dbsurveyor_all_<Os>_<Arch>` |
| `postgresql` | PostgreSQL only | `dbsurveyor_postgresql_<Os>_<Arch>` |
| `mysql` | MySQL only | `dbsurveyor_mysql_<Os>_<Arch>` |
| `sqlite` | SQLite only | `dbsurveyor_sqlite_<Os>_<Arch>` |
| `mongodb` | MongoDB only | `dbsurveyor_mongodb_<Os>_<Arch>` |
| `mssql` | MSSQL only | `dbsurveyor_mssql_<Os>_<Arch>` |

## What Gets Published

Each release produces:

| Artifact | Description |
|----------|-------------|
| **Variant archives** | tar.gz (Linux/macOS), zip (Windows) per variant per platform |
| **Linux packages** | .deb, .rpm, .apk (all-features variant only) |
| **Checksums** | SHA256 checksum file covering all artifacts |
| **Cosign signatures** | Keyless signatures on the checksum file |
| **SBOM** | Software Bill of Materials via Syft (per archive) |
| **Homebrew cask** | Published to `EvilBit-Labs/homebrew-tap` (all-features variant) |

## Supported Platforms

All variants are built for all platforms:

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

This installs the **all-features** variant with every database driver.

### Download Binary

Download the archive for your platform and desired database variant from the
[latest release](https://github.com/EvilBit-Labs/dbsurveyor/releases/latest)
and extract it.

### Linux Packages

Linux packages contain the **all-features** variant.

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

Build a snapshot locally (no publish). This builds all 42 binaries (7 build
configurations x 6 targets):

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
| Disk space on CI | The `free-disk-space` step runs before builds |
| Slow builds | 42 binaries take ~35-50 min; cargo caches shared deps |

## Security

- All release artifacts are signed with Cosign (keyless OIDC via GitHub Actions)
- SBOM generated for supply chain transparency
- Checksums for integrity verification
- No credentials or telemetry in release artifacts
