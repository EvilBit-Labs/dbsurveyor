---
inclusion: always
---


# Commit Standards for DBSurveyor

## Conventional Commits

All commits must follow [Conventional Commits](https://www.conventionalcommits.org) specification:

**Format**: `<type>(<scope>): <description>`

## Commit Types

- **feat**: New features or functionality
- **fix**: Bug fixes and corrections
- **docs**: Documentation changes
- **style**: Code style changes (formatting, etc.)
- **refactor**: Code refactoring without functional changes
- **perf**: Performance improvements
- **test**: Adding or updating tests
- **build**: Build system or dependency changes
- **ci**: CI/CD configuration changes
- **chore**: Maintenance tasks and housekeeping

## Scopes

Use these scopes to indicate the area of change:

- **collector**: Database collection functionality
- **processor**: Data processing and analysis
- **shared**: Shared library code
- **security**: Security-related changes
- **cli**: Command-line interface
- **encryption**: Encryption and cryptography
- **database**: Database-specific functionality
- **config**: Configuration management
- **docs**: Documentation updates

## Description Guidelines

- Use imperative mood ("add", not "added" or "adds")
- No period at the end
- Maximum 72 characters
- Capitalize first letter
- Be clear and specific about what changed

## Body (Optional)

- Start after a blank line
- Use itemized lists for multiple changes
- Explain what and why, not how
- Wrap at 72 characters

## Footer (Optional)

- Start after a blank line
- Use for issue references (`Closes #123`) or breaking changes
- Breaking changes: `BREAKING CHANGE: description`

## Breaking Changes

Indicate breaking changes in two ways:

1. Add `!` after type/scope: `feat(api)!: change authentication method`
2. Use footer: `BREAKING CHANGE: authentication now requires API key`

## Examples

### Good Commit Messages

```bash
# Feature addition
feat(collector): add PostgreSQL schema discovery with connection pooling

# Bug fix
fix(security): prevent credential leakage in error messages

# Documentation
docs(readme): update installation instructions for Rust 1.77+

# Refactoring
refactor(database): simplify connection management interface

# Testing
test(encryption): add AES-GCM randomness validation tests

# Maintenance
chore(deps): update SQLx to v0.7.3 for security patches

# Breaking change
feat(api)!: change collector interface to async/await

# With body and footer
feat(collector): add MySQL support with comprehensive schema extraction

- Implement MySQL-specific queries for tables and columns
- Add connection pooling with configurable limits
- Include support for MySQL-specific data types
- Add comprehensive test coverage with testcontainers

Closes #45
```

### Bad Commit Messages

```bash
# Too vague
fix: bug fix

# Wrong tense
feat(collector): added postgres support

# Too long description
feat(collector): add comprehensive PostgreSQL database schema collection functionality with full support for tables, columns, indexes, constraints, and metadata extraction

# Missing scope
feat: add database support

# Unclear description
fix(collector): fix thing

# Not imperative
docs: updated readme file
```

## Commit Message Template

Use this template for consistent commit messages:

```bash
# <type>(<scope>): <description>
# 
# <body>
# 
# <footer>

# Example:
# feat(collector): add PostgreSQL connection pooling
# 
# - Implement connection pool with configurable size
# - Add timeout handling for database connections
# - Include connection health checks
# 
# Closes #123
```

## Git Configuration

Set up your git configuration for consistent commits:

```bash
# Set commit template
git config commit.template .gitmessage

# Enable commit message validation
git config core.editor "code --wait"

# Set up commit signing (recommended)
git config commit.gpgsign true
git config user.signingkey YOUR_GPG_KEY
```

## Pre-commit Validation

Ensure commits meet quality standards:

```bash
# Run before committing
just format
just lint
just test

# Or use pre-commit hooks
pre-commit install
```

## CI Compatibility

All commits must:

- Pass `just ci-check` validation
- Include appropriate scope for the change area
- Use `chore:` for meta or maintenance changes
- Use `security:` scope for security-related changes

## Scope Guidelines

### When to Use Each Scope

- **collector**: Changes to database collection logic, adapters, or schema extraction
- **processor**: Changes to data processing, analysis, or transformation
- **shared**: Changes to shared library code, models, or utilities
- **security**: Security fixes, encryption changes, or credential handling
- **cli**: Command-line interface changes, argument parsing, or user interaction
- **encryption**: Cryptography implementation, key management, or data protection
- **database**: Database-specific functionality, queries, or connection handling
- **config**: Configuration management, settings, or environment handling
- **docs**: Documentation updates, README changes, or code comments

### Multiple Scopes

If a change affects multiple areas, choose the primary scope or use the most general applicable scope.

## Commit Frequency

- Make small, focused commits
- Each commit should represent a single logical change
- Avoid mixing unrelated changes in a single commit
- Commit frequently during development
- Use interactive rebase to clean up commit history before pushing

## Revert Commits

When reverting commits, use this format:

```bash
revert: feat(collector): add PostgreSQL connection pooling

This reverts commit 1234567890abcdef.

Reason: Connection pooling caused memory leaks in production.
```

This commit standard ensures clear, consistent, and informative commit history that supports effective collaboration and project maintenance.
