# dbsurveyor task runner
#
# Go recipes. The Rust workspace was removed from this branch in U12; it and its
# recipes remain recoverable from the rust-final branch.

set shell := ["bash", "-uc"]

# CGO is forbidden repository-wide (R13): pure-Go drivers are what make
# airgapped installs work without vendor client libraries.
export CGO_ENABLED := "0"

default:
    @just --list

# Build both binaries into ./dist
build:
    go build -trimpath -o dist/ ./cmd/...

# Regenerate the published format schemas and examples under docs/formats.
# These are derived from the Go types; TestPublishedArtifactsAreCurrent fails
# when a committed file is stale.
gen-schema:
    go run ./tools/genschema

# Run the full test suite
test:
    go test ./...

# Container-backed adapter tests. They sit behind the `integration` build tag so
# the ordinary `just test` needs no container runtime; this recipe needs Docker
# or a Testcontainers-compatible runtime on the machine.
test-integration:
    go test -tags integration -timeout 15m ./...

# Race-enabled run. -race requires cgo, so this recipe deliberately overrides
# the repository-wide CGO_ENABLED=0. It is a local/CI test-only exception and
# never applies to a shipped build.
test-race:
    CGO_ENABLED=1 go test -race ./...

# Coverage report
coverage:
    go test -coverprofile=coverage.out -covermode=atomic ./...
    go tool cover -func=coverage.out | tail -1

# Coverage gate. The threshold starts low and rises as phases land -- the Rust
# tree's 55% floor is not imported, since the Go tree starts from zero.
coverage-ci threshold="50":
    #!/usr/bin/env bash
    set -euo pipefail
    go test -coverprofile=coverage.out -covermode=atomic ./...
    pct=$(go tool cover -func=coverage.out | tail -1 | grep -oE '[0-9]+\.[0-9]+' | tail -1)
    echo "coverage: ${pct}% (threshold {{ threshold }}%)"
    awk -v p="$pct" -v t="{{ threshold }}" 'BEGIN { exit !(p < t) }' && {
        echo "FAIL: coverage ${pct}% is below the {{ threshold }}% threshold"
        exit 1
    } || true

# Lint with the strict golangci-lint v2 set
lint:
    golangci-lint run

# Format (golangci-lint v2 owns the formatters: gofumpt, goimports, gci, golines)
format:
    golangci-lint fmt

# Verify formatting without writing
format-check:
    golangci-lint fmt --diff

# Vulnerability scan
vuln:
    govulncheck ./...

# Full local gate -- run `just format` BEFORE this to avoid format-check failures
check: format-check lint test vuln
    @echo "check: OK"

# Validate the release config
release-check:
    goreleaser check

# Local release dry run: builds all targets, publishes nothing
release-snapshot:
    goreleaser release --snapshot --clean

# Install development tooling
dev-setup:
    mise install
    go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@latest
    go install golang.org/x/vuln/cmd/govulncheck@latest
