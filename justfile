# dbsurveyor task runner
#
# Go recipes. The Rust workspace was removed from this branch in U12; it and its
# recipes remain recoverable from the rust-final branch.
#
# `just` uses the LAST comment line above a recipe as its description, so each
# recipe below carries a one-line description immediately above it and keeps any
# longer rationale in the body. A rationale paragraph placed directly above a
# recipe becomes its `--list` text, which is how six of these ended up described
# by a sentence fragment.

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

# -----------------------------------------------------------------------------
# Help
# -----------------------------------------------------------------------------

alias h := help

# Show available recipes
[group('help')]
help:
    @just --list

# Show recipes in one group, e.g. `just group test`
[group('help')]
group name:
    @just --list --list-heading='' | grep -A99 '\[{{ name }}\]' | sed -n '2,$p' | sed '/^\[/q'

# -----------------------------------------------------------------------------
# Setup
# -----------------------------------------------------------------------------

alias setup := install

# Install the pinned toolchain and the pre-commit hooks
[group('setup')]
install:
    # mise.toml pins every tool, so this is the whole job. Do not add a
    # `go install ...@latest` here: it lands in $GOPATH/bin, which shadows the
    # pinned copy, and is never refreshed afterwards. See GOTCHAS 4.3.
    mise install
    {{ mise_exec }} pre-commit install --hook-type pre-commit --hook-type commit-msg
    {{ mise_exec }} go mod tidy

# Install the pinned toolchain (alias kept for existing muscle memory)
[group('setup')]
dev-setup: install

# Update the toolchain, Go modules, and pre-commit hooks
[group('setup')]
update-deps: _update-mise _update-go _update-precommit

[private]
_update-mise:
    mise upgrade --bump --local --before 7d

[private]
_update-go:
    {{ mise_exec }} go get -u ./...
    {{ mise_exec }} go mod tidy
    {{ mise_exec }} go mod verify

[private]
_update-precommit:
    {{ mise_exec }} pre-commit autoupdate

# -----------------------------------------------------------------------------
# Build
# -----------------------------------------------------------------------------

# Build both binaries into ./dist
[group('build')]
build:
    {{ mise_exec }} go build -trimpath -o dist/ ./cmd/...

# Run the collector, e.g. `just collect --help`
[group('build')]
collect *args:
    {{ mise_exec }} go run ./cmd/dbsurveyor-collect {{ args }}

# Run the postprocessor, e.g. `just postprocess --help`
[group('build')]
postprocess *args:
    {{ mise_exec }} go run ./cmd/dbsurveyor {{ args }}

# Regenerate the published format schemas and examples under docs/formats
[group('build')]
gen-schema:
    # Derived from the Go types; TestPublishedArtifactsAreCurrent fails when a
    # committed file is stale.
    {{ mise_exec }} go run ./tools/genschema

# -----------------------------------------------------------------------------
# Testing
# -----------------------------------------------------------------------------

alias t := test

# Run the full test suite
[group('test')]
test:
    {{ mise_exec }} go test ./...

# Run the full test suite with verbose output
[group('test')]
test-v:
    {{ mise_exec }} go test -v ./...

# Run the container-backed adapter tests (needs Docker)
[group('test')]
test-integration:
    # These sit behind the `integration` build tag so the ordinary `just test`
    # needs no container runtime. This recipe needs Docker or another
    # Testcontainers-compatible runtime on the machine.
    {{ mise_exec }} go test -tags integration -timeout 15m ./...

# Run the suite under the race detector (the one place CGO is allowed)
[group('test')]
test-race:
    # -race requires cgo, so this recipe deliberately overrides the
    # repository-wide CGO_ENABLED=0. It is a test-only exception and never
    # applies to a shipped build. See GOTCHAS 4.2.
    CGO_ENABLED=1 {{ mise_exec }} go test -race ./...

# Report total coverage
[group('test')]
coverage:
    {{ mise_exec }} go test -coverprofile=coverage.out -covermode=atomic ./...
    {{ mise_exec }} go tool cover -func=coverage.out | tail -1

# Open the coverage report in a browser
[group('test')]
coverage-html: coverage
    {{ mise_exec }} go tool cover -html=coverage.out

# Fail when total coverage is below the threshold
[group('test')]
coverage-ci threshold="50":
    #!/usr/bin/env bash
    # The threshold starts low and rises as phases land -- the Rust tree's 55%
    # floor is not imported, since the Go tree starts from zero.
    set -euo pipefail
    {{ mise_exec }} go test -coverprofile=coverage.out -covermode=atomic ./...
    pct=$({{ mise_exec }} go tool cover -func=coverage.out | tail -1 | grep -oE '[0-9]+\.[0-9]+' | tail -1)
    echo "coverage: ${pct}% (threshold {{ threshold }}%)"
    awk -v p="$pct" -v t="{{ threshold }}" 'BEGIN { exit !(p < t) }' && {
        echo "FAIL: coverage ${pct}% is below the {{ threshold }}% threshold"
        exit 1
    } || true

# -----------------------------------------------------------------------------
# Quality
# -----------------------------------------------------------------------------

alias fmt := format

# Format (golangci-lint v2 owns gofumpt, goimports, gci, golines)
[group('quality')]
format:
    {{ mise_exec }} golangci-lint fmt

# Verify formatting without writing
[group('quality')]
format-check:
    {{ mise_exec }} golangci-lint fmt --diff

# Lint with the strict golangci-lint v2 set
[group('quality')]
lint:
    {{ mise_exec }} golangci-lint run

# Scan for known vulnerabilities
[group('quality')]
vuln:
    {{ mise_exec }} govulncheck ./...

# Run every pre-commit hook over the whole tree
[group('quality')]
pre-commit:
    {{ mise_exec }} pre-commit run --all-files

# Full local gate -- run `just format` first so a whitespace diff is not a lint failure
[group('quality')]
check: format-check lint test vuln
    @echo "check: OK"

# -----------------------------------------------------------------------------
# CI
# -----------------------------------------------------------------------------

# Everything CI runs on a pull request, in CI's order
[group('ci')]
ci-check: format-check lint test test-race vuln
    @echo "ci-check: OK"

# Fast feedback: build and run the short tests only
[group('ci')]
ci-smoke:
    {{ mise_exec }} go build -trimpath -o dist/ ./cmd/...
    {{ mise_exec }} go test -count=1 -failfast -short -timeout 5m ./...
    @echo "ci-smoke: OK"

# ci-check plus the container suites, the docs build, and release validation
[group('ci')]
ci-full: ci-check test-integration docs-build release-check
    @echo "ci-full: OK"

# Lint the GitHub Actions workflows
[group('ci')]
lint-actions:
    {{ mise_exec }} actionlint

# -----------------------------------------------------------------------------
# Docs
# -----------------------------------------------------------------------------

# Build the mdBook site into docs/book
[group('docs')]
docs-build:
    cd docs && {{ mise_exec }} mdbook build

# Serve the mdBook site with live reload
[group('docs')]
docs-serve:
    cd docs && {{ mise_exec }} mdbook serve --open

# There is deliberately no docs-check here. `mdbook test` compiles code blocks
# as Rust doctests, which fails on this book's Go and shell samples. The link
# checker mise pins, mdbook-linkcheck, only runs as an mdbook backend and
# docs/book.toml does not configure one, so wiring it up is a book.toml change
# rather than a recipe.

# -----------------------------------------------------------------------------
# Release
# -----------------------------------------------------------------------------

# Validate .goreleaser.yaml
[group('release')]
release-check:
    {{ mise_exec }} goreleaser check

# Local release dry run: builds all targets, publishes nothing
[group('release')]
release-snapshot:
    {{ mise_exec }} goreleaser release --snapshot --clean

# Build for the current platform only
[group('release')]
release-local:
    {{ mise_exec }} goreleaser build --snapshot --clean --single-target

# Generate the changelog
[group('release')]
changelog:
    {{ mise_exec }} git-cliff --output CHANGELOG.md

# Generate the changelog for unreleased commits only
[group('release')]
changelog-unreleased:
    {{ mise_exec }} git-cliff --unreleased

# Assert the built artifacts are CGO-free and trimpath-built
[group('release')]
verify-artifacts:
    #!/usr/bin/env bash
    # tools/nocgo_test.go checks that no package in the graph imports C, which
    # is the property at the source level. This checks the other end: that the
    # binaries GoReleaser actually produced were built with CGO_ENABLED=0 and
    # -trimpath. A build setting can be lost to a stray environment variable on
    # a release runner without a single source file changing, and that is
    # exactly the failure the source-level test cannot see.
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
