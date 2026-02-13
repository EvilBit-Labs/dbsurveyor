# Release Process

## Overview

Releases are built by [GoReleaser](https://goreleaser.com/) using `cargo zigbuild`
for cross-compilation from source. The pipeline is defined in
`.github/workflows/release.yml` and configured by `.goreleaser.yaml`.

## Triggering a Release

Releases are triggered by pushing a semver tag:

```bash
cargo release patch   # or minor / major
```

`cargo-release` bumps versions, commits, and pushes a signed tag matching the
pattern `v<major>.<minor>.<patch>`. The workflow starts automatically.

## What the Pipeline Does

1. **Build** -- a single job on `ubuntu-latest` cross-compiles both binaries
   (`dbsurveyor` and `dbsurveyor-collect`) for all six target platforms via
   GoReleaser's native Rust builder and `cargo zigbuild`.
2. **Release** -- GoReleaser packages archives, generates SHA-256 checksums,
   creates the GitHub Release, and pushes a Homebrew cask.

## Supported Platforms

All targets are cross-compiled from `ubuntu-latest` using `cargo zigbuild`:

| Target | Archive |
|--------|---------|
| `x86_64-unknown-linux-gnu` | tar.gz |
| `aarch64-unknown-linux-gnu` | tar.gz |
| `x86_64-unknown-linux-musl` | tar.gz |
| `aarch64-apple-darwin` | tar.gz |
| `x86_64-apple-darwin` | tar.gz |
| `x86_64-pc-windows-gnu` | zip |

## Artifact Types

- **Archives** -- `.tar.gz` (Unix), `.zip` (Windows), each containing both
  binaries plus LICENSE and README
- **Checksums** -- `checksums.txt` with SHA-256 hashes
- **Homebrew cask** -- published to `EvilBit-Labs/homebrew-tap`

## Homebrew Installation

```bash
brew install EvilBit-Labs/tap/dbsurveyor
```

## Required Secrets

| Secret | Purpose |
|--------|---------|
| `GITHUB_TOKEN` | Provided automatically; creates releases and uploads artifacts |
| `HOMEBREW_TAP_TOKEN` | PAT with write access to `EvilBit-Labs/homebrew-tap` |

## Release Checklist

### Pre-Release

- [ ] All CI checks passing on `main`
- [ ] Security audit clean (`cargo audit`)
- [ ] Changelog / release notes prepared
- [ ] Version bumped via `cargo release`

### Post-Release

- [ ] Verify artifacts on the GitHub Release page
- [ ] Verify checksums file is present and correct
- [ ] Confirm Homebrew formula updated in tap repository
- [ ] Test `brew install EvilBit-Labs/tap/dbsurveyor` on a clean machine

## Troubleshooting

| Issue | Resolution |
|-------|------------|
| Build fails for a single target | Check the job logs; verify `cargo zigbuild` and the Zig toolchain for that target are correctly configured |
| GoReleaser fails with path error | Verify `.goreleaser.yaml` build targets and flags match the installed Rust targets |
| Homebrew push fails | Confirm `HOMEBREW_TAP_TOKEN` secret has write access to the tap repo |
| Prerelease not marked | Tags with `-rc.N`, `-beta.N`, etc. are auto-detected by GoReleaser |
