# Release Process

## Overview

DBSurveyor releases are automated through [GoReleaser](https://goreleaser.com/)
using its native Go builder. Pushing a semver tag triggers the full pipeline.

There is **one build per platform**, not one per database driver. Every driver is
pure Go, so there are no optional native dependencies to gate behind a build
variant -- a single binary speaks all six engines. That is also why there is no
separate musl target: with `CGO_ENABLED=0` the binary is already statically
linked.

## How to Release

1. Ensure all tests pass and the main branch is clean.

2. Update `CHANGELOG.md`. The version itself is not stored in a file: it is
   derived from the tag and stamped into the binaries through ldflags.

3. Tag the release and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The `Release` workflow builds, signs, and publishes automatically.

## Release Artifacts

One archive per platform, each containing both binaries: `dbsurveyor-collect`
(the collector) and `dbsurveyor` (the postprocessor). Every archive supports all
six engines, plus compression and encryption.

| Platform | Architectures  | Archive Name Pattern              |
| -------- | -------------- | --------------------------------- |
| Linux    | amd64, arm64   | `dbsurveyor_Linux_<Arch>.tar.gz`  |
| macOS    | amd64, arm64   | `dbsurveyor_Darwin_<Arch>.tar.gz` |
| Windows  | amd64          | `dbsurveyor_Windows_<Arch>.zip`   |

Windows on arm64 is not built. It can be added when somebody needs it.

## What Gets Published

Each release produces:

| Artifact              | Description                                                     |
| --------------------- | --------------------------------------------------------------- |
| **Variant archives**  | tar.gz (Linux/macOS), zip (Windows) per variant per platform    |
| **Linux packages**    | .deb, .rpm, .apk (all-features variant only)                    |
| **Checksums**         | SHA256 checksum file covering all artifacts                     |
| **Cosign signatures** | Keyless signatures on the checksum file                         |
| **SBOM**              | Software Bill of Materials via Syft (per archive)               |
| **Homebrew cask**     | Published to `EvilBit-Labs/homebrew-tap` (all-features variant) |

## Supported Platforms

Most variants are built for all six platforms below. The `all` and `mssql` collector variants exclude `x86_64-unknown-linux-musl` because the `mssql` feature pulls `tiberius` with `native-tls`, which links system OpenSSL and cannot statically link against musl. Static-musl users should pick a per-driver variant (`postgresql`, `mysql`, `sqlite`, `mongodb`).

| OS           | Architecture            | Target Triple               | mssql / all-features |
| ------------ | ----------------------- | --------------------------- | -------------------- |
| Linux        | x86_64                  | `x86_64-unknown-linux-gnu`  | yes                  |
| Linux        | aarch64                 | `aarch64-unknown-linux-gnu` | yes                  |
| Linux (musl) | x86_64                  | `x86_64-unknown-linux-musl` | no (see above)       |
| macOS        | x86_64                  | `x86_64-apple-darwin`       | yes                  |
| macOS        | aarch64 (Apple Silicon) | `aarch64-apple-darwin`      | yes                  |
| Windows      | x86_64                  | `x86_64-pc-windows-gnu`     | yes                  |

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

Download the archive for your platform and desired database variant from the [latest release](https://github.com/EvilBit-Labs/dbsurveyor/releases/latest) and extract it.

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

Build a snapshot locally (no publish). This builds 40 binaries (1 postprocessor for 6 targets + 6 collector variants for 6 targets each, minus the two `mssql`-containing collector variants on `x86_64-unknown-linux-musl`):

```bash
goreleaser build --snapshot --clean
```

Full dry-run release:

```bash
goreleaser release --snapshot --clean --skip=publish
```

## Troubleshooting

| Issue               | Resolution                                                         |
| ------------------- | ------------------------------------------------------------------ |
| Build failures      | Check the Go version in `go.mod` matches the workflow's setup-go   |
| Signing failures    | Verify `id-token: write` permission and Cosign version             |
| Homebrew push fails | Verify `HOMEBREW_TAP_TOKEN` secret is set                          |
| Missing SBOM        | Ensure Syft is installed in the workflow                           |
| Tag format rejected | Tags must match `v*.*.*` (e.g., `v0.1.0`)                          |
| Disk space on CI    | The `free-disk-space` step runs before builds                      |
| Slow builds         | Ten binaries across five platform pairs; minutes, not tens of them |

## Required Secrets

| Secret               | Description                                          |
| -------------------- | ---------------------------------------------------- |
| `GITHUB_TOKEN`       | Provided automatically by GitHub Actions             |
| `HOMEBREW_TAP_TOKEN` | PAT with write access to `EvilBit-Labs/homebrew-tap` |

## Security

- All release artifacts are signed with Cosign (keyless OIDC via GitHub Actions)
- SBOM generated for supply chain transparency
- Checksums for integrity verification
- No credentials or telemetry in release artifacts
