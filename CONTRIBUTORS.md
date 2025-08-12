# Contributors Guide

## Project Maintainer

**Primary Maintainer**: UncleSp1d3r
**Role**: Single maintainer and project owner

## Contribution Workflow

### Single-Maintainer Model

This project operates under a **single-maintainer model** with the following characteristics:

- **No Second Reviewer Required**: All changes are reviewed and approved by the primary maintainer
- **Direct Push Access**: The maintainer has direct push access to all branches
- **No Branch Protection**: Push restrictions are configured for maintainer-only access
- **Streamlined Process**: Optimized for rapid iteration and decision-making

### Code Review Preferences

#### Primary Code Review Tool: CodeRabbit.ai

- **Preferred AI Assistant**: [CodeRabbit.ai](https://coderabbit.ai) for automated code reviews
- **Conversational Review**: Supports back-and-forth dialogue for review feedback
- **Integration**: Automatically provides AI summaries and line-by-line code analysis
- **Benefits**: Intelligent code understanding, security analysis, and best practice enforcement

#### GitHub Copilot Policy

- **No Automatic Copilot Reviews**: GitHub Copilot automatic reviews are disabled
- **Manual Use Only**: Copilot may be used manually during development but not for automated review
- **Rationale**: CodeRabbit.ai provides superior conversational review capabilities

### Quality Standards

#### Rust Quality Gate

All Rust code must pass strict linting requirements:

```bash
cargo clippy -- -D warnings
```

**Enforcement**: This command is explicitly listed in CI/CD pipelines and must pass without any warnings.

#### Additional Quality Checks

- **Formatting**: `cargo fmt --check` must pass
- **Tests**: Full test suite must pass with coverage requirements
- **Security**: All security scans (CodeQL, Syft, Grype) must pass
- **License Compliance**: FOSSA license validation must pass

### Milestone Strategy

#### Version-Based Milestone Naming

Milestones are named using version numbers with contextual descriptions:

- **Format**: `v{major}.{minor}` (e.g., `v0.1`, `v0.2`, `v0.3`, `v1.0`)
- **Description**: Each milestone includes a contextual description of its goals and scope

#### Example Milestones

- **v0.1**: Collector MVP - Basic schema collection with multi-engine support
- **v0.2**: Postprocessor MVP - Documentation generation and analysis
- **v0.3**: Pro Features - Advanced analysis, visualization, and extensibility
- **v1.0**: Production Release - Cross-platform packaging and polish

### Future Development Notes

#### HTTP Client Standards

**Important**: If future development requires HTTP client functionality:

- **Preferred Tool**: Use **OpenAPI Generator** for Rust client code generation
- **Rationale**: Ensures type-safe, well-documented API clients
- **Standard Practice**: Aligns with organization preferences for code generation

#### Repository Workflow Constraints

- **Single Maintainer**: All decisions flow through the primary maintainer
- **No Multi-Approval**: Workflows do not require multiple approvers
- **Direct Access**: Maintainer can merge PRs and push directly as needed
- **Rapid Iteration**: Optimized for quick development cycles and immediate feedback

## Contact

For questions about contribution workflows or project direction, contact:

**UncleSp1d3r** - Primary Maintainer

---

*This document reflects the current organizational preferences and may be updated as the project evolves.*
