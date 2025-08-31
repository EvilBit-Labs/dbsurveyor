#!/bin/bash

# Generate SBOM provenance metadata
# This script creates a metadata file alongside SBOM outputs

set -euo pipefail

# Get commit SHA (short format, fallback to "unknown")
COMMIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")

# Get ISO8601 UTC timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Get Syft version (fallback to "unknown")
SYFT_VERSION=$(~/.local/bin/syft version 2>/dev/null | head -n1 | sed 's/syft //' || echo "unknown")

# Create metadata JSON file
cat > sbom.metadata.json << EOF
{
    "generated_at": "$TIMESTAMP",
    "commit_sha": "$COMMIT_SHA",
    "tool_version": "$SYFT_VERSION",
    "sbom_files": [
        "sbom.json",
        "sbom.spdx.json"
    ],
    "format": "JSON",
    "generator": "syft"
}
EOF

echo "✅ SBOM metadata generated: sbom.metadata.json"
