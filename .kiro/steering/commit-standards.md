---
inclusion: always
---

# Commit Standards for DBSurveyor

## Conventional Commits Format

**Required Format**: `<type>(<scope>): <description>`

All commits MUST follow [Conventional Commits](https://www.conventionalcommits.org) specification.

## Commit Types (Required)

- **feat**: New features or functionality
- **fix**: Bug fixes and corrections
- **security**: Security-related changes (credential handling, encryption, vulnerabilities)
- **refactor**: Code refactoring without functional changes
- **test**: Adding or updating tests
- **docs**: Documentation changes
- **perf**: Performance improvements
- **build**: Build system or dependency changes
- **ci**: CI/CD configuration changes
- **chore**: Maintenance tasks and housekeeping

## Scopes (Required)

Choose the most specific applicable scope:

- **collector**: Database collection functionality (`dbsurveyor-collect` crate)
- **processor**: Report generation and analysis (`dbsurveyor` crate)
- **core**: Shared library code (`dbsurveyor-core` crate)
- **postgres**: PostgreSQL-specific functionality
- **mysql**: MySQL-specific functionality
- **sqlite**: SQLite-specific functionality
- **mongodb**: MongoDB-specific functionality
- **encryption**: AES-GCM encryption and key management
- **cli**: Command-line interface and argument parsing
- **config**: Configuration management and environment handling

## Description Rules (Mandatory)

- Use imperative mood: "add", "fix", "update" (not "added", "fixed", "updated")
- Maximum 72 characters
- No period at the end
- Capitalize first letter
- Be specific about what changed

## Security-Focused Examples

```bash
# Security fixes (always use security type)
security(core): prevent credential leakage in error messages
security(encryption): use random nonce for each AES-GCM operation
security(collector): sanitize connection strings in logs

# Database functionality
feat(postgres): add connection pooling with timeout handling
fix(mysql): handle connection failures without exposing credentials
refactor(sqlite): simplify schema extraction queries

# Core functionality  
feat(core): add AES-GCM encryption for schema output
fix(core): ensure proper cleanup of sensitive data structures
test(core): add comprehensive credential sanitization tests

# CLI and configuration
feat(cli): add --encrypt flag for secure output
fix(config): validate database URLs without logging credentials
```

## Breaking Changes

Indicate breaking changes:

1. Add `!` after scope: `feat(core)!: change SchemaCollector trait interface`
2. Use footer: `BREAKING CHANGE: SchemaCollector now requires async new() method`

## Body Guidelines (Optional)

- Start after blank line
- Use bullet points for multiple changes
- Explain what and why, not how
- Focus on security implications when relevant
- Wrap at 72 characters

## Footer Guidelines (Optional)

- Reference issues: `Closes #123`, `Fixes #456`
- Breaking changes: `BREAKING CHANGE: description`
- Security advisories: `Security: Fixes CVE-2024-XXXX`

## Quality Requirements

Before committing, ensure:

- Code passes `just lint` (zero warnings policy)
- Tests pass with `just test`
- Security validation passes with `just security-full`
- No credentials exposed in any output or logs

## Common Patterns

```bash
# Adding new database support
feat(mongodb): implement schema collection with connection pooling

# Security improvements
security(core): implement secure memory cleanup for credentials
security(encryption): add key derivation parameter validation

# Performance optimizations
perf(collector): optimize schema queries for large databases
perf(core): reduce memory allocation in schema processing

# Testing additions
test(postgres): add integration tests with testcontainers
test(security): verify no credential leakage in all outputs
```

## Anti-Patterns to Avoid

```bash
# ❌ Too vague
fix: bug fix
feat: add support

# ❌ Wrong tense  
feat(collector): added postgres support

# ❌ Missing security scope
fix(collector): prevent credential exposure  # Should be security(collector)

# ❌ Too long
feat(collector): add comprehensive PostgreSQL database schema collection functionality

# ❌ Exposes implementation details
fix(postgres): change connection string parsing in line 45
```

This standard ensures security-conscious, clear commit history that aligns with DBSurveyor's offline-first, security-first architecture.
