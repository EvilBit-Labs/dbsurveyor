# Installation

DBSurveyor ships as two static binaries with no runtime dependencies. There is
nothing to install alongside them: no database client libraries, no Oracle
Instant Client, no runtime.

## Pre-built binaries

Download the archive for your platform from the
[Releases](https://github.com/EvilBit-Labs/dbsurveyor/releases) page and extract
it. Each archive contains both binaries.

| Platform | Architectures |
| -------- | ------------- |
| Linux    | amd64, arm64  |
| macOS    | amd64, arm64  |
| Windows  | amd64         |

Every archive supports all six database engines. There are no per-driver
variants: the drivers are pure Go, so there is nothing to gate behind a build
flag.

## Homebrew

```bash
brew install EvilBit-Labs/tap/dbsurveyor
```

## From source

Requires Go 1.26 or later. See `mise.toml` for the exact pinned version.

```bash
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor
go build -trimpath -o dist/ ./cmd/...
```

Or with the task runner:

```bash
just build
```

Both binaries land in `./dist`.

### Why there are no build variants

The retired Rust implementation gated each database driver behind a Cargo
feature, because several drivers needed native client libraries and an operator
who only wanted PostgreSQL should not have had to install the rest.

That problem does not exist here. Every driver is pure Go, `CGO_ENABLED=0`
throughout, and a repository test fails on any cgo dependency entering the
graph -- so one binary speaks all six engines and links nothing at runtime. The
build-time choice went away because the cost it was avoiding went away.

## Verifying a release

Release artifacts are signed with Cosign using keyless OIDC, and each release
publishes an SBOM and checksums.

```bash
# Checksums
sha256sum --check dbsurveyor_checksums.txt

# Signature
cosign verify-blob \
  --certificate dbsurveyor_checksums.txt.pem \
  --signature dbsurveyor_checksums.txt.sig \
  --certificate-identity-regexp 'https://github.com/EvilBit-Labs/dbsurveyor/.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  dbsurveyor_checksums.txt
```

## Airgapped installation

This is the case the tool is built for, and it needs no special procedure: copy
the archive across and extract it. The binaries make no network call except to
the database they are pointed at, check for no updates, and send no telemetry.

Building from source on an airgapped host needs the module cache carried across:

```bash
# On a connected host
go mod download
go mod vendor

# Carry the tree across, then on the airgapped host
go build -mod=vendor -trimpath -o dist/ ./cmd/...
```

## Development setup

```bash
just dev-setup     # install the toolchain and Go tools
pre-commit install # git hooks
```

`dev-setup` installs `golangci-lint` and `govulncheck`. Note that `mise` puts Go
tools in the Go toolchain's own `bin` rather than in `$GOPATH/bin`; if
`just vuln` reports "command not found", look under `$(go env GOBIN)`.

## Troubleshooting

**"command not found" after extracting.** The binary is not on your `PATH`.
Either move it somewhere that is, or invoke it by path.

**macOS refuses to run the binary.** Gatekeeper quarantines downloaded files.
`xattr -d com.apple.quarantine dbsurveyor` clears it, or use the Homebrew
installation, which is not quarantined.

**A build fails with a Go version error.** The module requires Go 1.26. Check
`go version`, and prefer `mise install` to get the pinned toolchain rather than
whatever the system package manager has.
