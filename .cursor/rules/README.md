# Cursor Rules Organization

This directory contains Cursor AI development rules organized into logical subfolders for better maintainability and clarity.

## Directory Structure

```text
.cursor/rules/
├── core/                       # Foundation rules (always apply)
│   ├── core-concepts.mdc       # Complete project overview & core principles
│   └── commit-style.mdc        # Conventional commit standards
├── ai-assistant/               # AI behavior and workflows
│   ├── ai-assistant-guidelines.mdc  # AI behavior rules & practices
│   └── development-workflow.mdc     # Development process & QA
├── rust/                       # Rust language specifics
│   ├── rust-standards.mdc      # Rust coding standards
│   ├── rust-testing.mdc        # Rust testing best practices
│   └── rust-documentation.mdc  # Rust documentation standards
├── project/                    # Project architecture & design
│   └── dbsurveyor-architecture.mdc  # DBSurveyor architecture patterns
└── quality/                    # Code quality & enforcement
    └── code-quality.mdc        # Quality standards & enforcement
```

## Quick Access

- **New to project?** Start with [`core/core-concepts.mdc`](core/core-concepts.mdc)
- **AI agent setup?** Check [`ai-assistant/ai-assistant-guidelines.mdc`](ai-assistant/ai-assistant-guidelines.mdc)
- **Writing Rust code?** See [`rust/rust-standards.mdc`](rust/rust-standards.mdc)
- **Need commit help?** Reference [`core/commit-style.mdc`](core/commit-style.mdc)
- **Architecture guidance?** Review [`project/dbsurveyor-architecture.mdc`](project/dbsurveyor-architecture.mdc)
- **Quality standards?** Check [`quality/code-quality.mdc`](quality/code-quality.mdc)

## Detailed Folder Structure

### Core (`core/`)

#### Foundation rules that always apply - fundamental project principles

- **`core-concepts.mdc`** - Complete project overview covering security philosophy, development standards, architecture patterns, workflow guidelines, and core principles
- **`commit-style.mdc`** - Conventional commit message standards with project-specific scopes

*These rules have `alwaysApply: true` and form the foundation for all development work.*

### AI Assistant (`ai-assistant/`)

#### AI agent behavior, workflows, and mandatory practices

- **`ai-assistant-guidelines.mdc`** - AI behavior rules, development rules of engagement, mandatory practices, and common workflows
- **`development-workflow.mdc`** - Development process, quality assurance steps, and code review checklist

*Essential for AI agents working on the project.*

### Rust Language (`rust/`)

#### Rust-specific coding standards and best practices

- **`rust-standards.mdc`** - Rust development standards, coding conventions, and language-specific guidelines
- **`rust-testing.mdc`** - Rust testing best practices, benchmarking, and test organization
- **`rust-documentation.mdc`** - Rust documentation standards and commenting conventions

*Applied when working with Rust source files (`**/*.rs`, `**/Cargo.toml`).*

### Project (`project/`)

#### Project architecture and system design

- **`dbsurveyor-architecture.mdc`** - Complete architecture guidelines covering module organization, data flow, security architecture, performance patterns, and configuration management

*Defines the DBSurveyor system architecture and design patterns.*

### Quality (`quality/`)

#### Code quality assurance and enforcement

- **`code-quality.mdc`** - Comprehensive quality standards including clippy configuration, rustfmt settings, error handling quality, documentation quality, testing quality, and CI/CD quality gates

*Ensures consistent code quality, security, and compliance standards.*

## Rule Precedence

**CRITICAL - Rules are applied in the following order of precedence:**

1. **Project-specific rules** (from project root instruction files like AGENTS.md or .cursor/rules/)
1. **General development standards** (outlined in these rules)
1. **Language-specific style guides** (Rust conventions, etc.)

When rules conflict, always follow the rule with higher precedence.

## Rule Application

### Always Applied Rules

- `core/core-concepts.mdc` (`alwaysApply: true`)
- `project/dbsurveyor-architecture.mdc` (`alwaysApply: true`)
- `quality/code-quality.mdc` (`alwaysApply: true`)

### Context-Specific Rules

- `rust/` rules apply to `**/*.rs`, `**/Cargo.toml` files
- `ai-assistant/ai-assistant-guidelines.mdc` applies to all files
- `ai-assistant/development-workflow.mdc` applies to `**/*.md,**/*.rs,**/justfile`
- Other rules apply based on their specific glob patterns

## Related Documentation

For comprehensive project information, also refer to:

- **[AGENTS.md](../../AGENTS.md)** - Complete AI agent development guidelines
- **[.github/copilot-instructions.md](../../.github/copilot-instructions.md)** - GitHub Copilot specific instructions
- **[project_specs/requirements.md](../../project_specs/requirements.md)** - Project requirements specification
- **[README.md](../../README.md)** - Project overview and getting started

## Maintenance

When updating cursor rules:

1. **Maintain consistency** with AGENTS.md and GitHub Copilot instructions
1. **Update related files** when making changes that affect multiple rule categories
1. **Test rule application** to ensure no conflicts between rules
1. **Document changes** in the appropriate category README if needed

## Benefits of This Organization

- **Logical Separation**: Related rules grouped together for easier maintenance
- **Reduced Cognitive Load**: Easier to find and update specific types of rules
- **Clear Ownership**: Each category has a specific purpose and scope
- **Maintainability**: Changes can be made to specific areas without affecting others
- **Consistency**: Aligned with project documentation and other AI tool instructions
