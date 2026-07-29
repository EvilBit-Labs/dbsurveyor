# dbsurveyor task runner
#
# Go recipes. The Rust workspace was removed from this branch in U12; it and its
# recipes remain recoverable from the rust-final branch.

set shell := ["bash", "-uc"]

# CGO is forbidden repository-wide (R13): pure-Go drivers are what make
# airgapped installs work without vendor client libraries.
export CGO_ENABLED := "0"

# Use mise to manage all dev tools (go, golangci-lint, govulncheck, goreleaser)
# See mise.toml for tool versions
#
# Every tool below is invoked through mise rather than by bare name, the Go
# toolchain included. $GOPATH/bin sits ahead of mise's tool directories on PATH,
# so a bare name runs whatever a past `go install ...@latest` left there --
# unpinned, never refreshed, and carrying the Go toolchain of the day it was
# installed. See GOTCHAS 4.3.
mise_exec := "mise exec --"

default:
    @just --list

# Build both binaries into ./dist
build:
    {{ mise_exec }} go build -trimpath -o dist/ ./cmd/...

# Regenerate the published format schemas and examples under docs/formats.
# These are derived from the Go types; TestPublishedArtifactsAreCurrent fails
# when a committed file is stale.
gen-schema:
    {{ mise_exec }} go run ./tools/genschema

# Run the full test suite
test:
    {{ mise_exec }} go test ./...

# Container-backed adapter tests. They sit behind the `integration` build tag so
# the ordinary `just test` needs no container runtime; this recipe needs Docker
# or a Testcontainers-compatible runtime on the machine.
test-integration:
    {{ mise_exec }} go test -tags integration -timeout 15m ./...

# Race-enabled run. -race requires cgo, so this recipe deliberately overrides
# the repository-wide CGO_ENABLED=0. It is a local/CI test-only exception and
# never applies to a shipped build.
test-race:
    CGO_ENABLED=1 {{ mise_exec }} go test -race ./...

# Coverage report
coverage:
    {{ mise_exec }} go test -coverprofile=coverage.out -covermode=atomic ./...
    {{ mise_exec }} go tool cover -func=coverage.out | tail -1

# Coverage gate. The threshold starts low and rises as phases land -- the Rust
# tree's 55% floor is not imported, since the Go tree starts from zero.
coverage-ci threshold="50":
    #!/usr/bin/env bash
    set -euo pipefail
    {{ mise_exec }} go test -coverprofile=coverage.out -covermode=atomic ./...
    pct=$({{ mise_exec }} go tool cover -func=coverage.out | tail -1 | grep -oE '[0-9]+\.[0-9]+' | tail -1)
    echo "coverage: ${pct}% (threshold {{ threshold }}%)"
    awk -v p="$pct" -v t="{{ threshold }}" 'BEGIN { exit !(p < t) }' && {
        echo "FAIL: coverage ${pct}% is below the {{ threshold }}% threshold"
        exit 1
    } || true

# Lint with the strict golangci-lint v2 set
lint:
    {{ mise_exec }} golangci-lint run

# Format (golangci-lint v2 owns the formatters: gofumpt, goimports, gci, golines)
format:
    {{ mise_exec }} golangci-lint fmt

# Verify formatting without writing
format-check:
    {{ mise_exec }} golangci-lint fmt --diff

# Vulnerability scan
vuln:
    {{ mise_exec }} govulncheck ./...

# Full local gate -- run `just format` BEFORE this to avoid format-check failures
check: format-check lint test vuln
    @echo "check: OK"

# Validate the release config
release-check:
    {{ mise_exec }} goreleaser check

# Local release dry run: builds all targets, publishes nothing
release-snapshot:
    {{ mise_exec }} goreleaser release --snapshot --clean

# Assert the no-CGO property on the built artifacts rather than only on the
# dependency graph.
#
# tools/nocgo_test.go checks that no package in the graph imports C, which is
# the property at the source level. This checks the other end: that the binaries
# GoReleaser actually produced were built with CGO_ENABLED=0 and -trimpath. A
# build setting can be lost to a stray environment variable on a release runner
# without a single source file changing, and that is exactly the failure the
# source-level test cannot see.
verify-artifacts:
    #!/usr/bin/env bash
    set -euo pipefail
    found=0
    while IFS= read -r binary; do
        found=$((found + 1))
        settings=$({{ mise_exec }} go version -m "$binary")
        grep -q 'CGO_ENABLED=0' <<<"$settings" || {
            echo "FAIL: $binary was not built with CGO_ENABLED=0"
            exit 1
        }
        grep -q '\-trimpath=true' <<<"$settings" || {
            echo "FAIL: $binary was not built with -trimpath"
            exit 1
        }
        echo "ok: $binary"
    done < <(find dist -type f -regex '.*/dbsurveyor\(-collect\)?\(\.exe\)?$')
    if [ "$found" -eq 0 ]; then
        echo "FAIL: no binaries found under dist/; run just release-snapshot first"
        exit 1
    fi
    echo "verify-artifacts: $found binaries OK"

# Install development tooling
#
# mise.toml pins every tool, so this is the whole job. Do not add a
# `go install ...@latest` here: it lands in $GOPATH/bin, which shadows the pinned
# copy, and it is never refreshed afterwards. See GOTCHAS 4.3.
dev-setup:
    mise install
