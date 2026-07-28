# Pull Request Template

## Description

Brief description of changes made and why they are needed.

**Security Impact**: [None/Low/Medium/High] - Describe any security implications

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Security enhancement
- [ ] Performance improvement
- [ ] Refactoring (no functional changes)

## Database Support

- [ ] PostgreSQL
- [ ] MySQL
- [ ] SQLite
- [ ] SQL Server
- [ ] MongoDB
- [ ] No database changes

## Security Checklist

- [ ] No credentials or sensitive data in code, logs, or error messages
- [ ] All database operations are read-only
- [ ] No external network calls (except to target databases)
- [ ] Encryption implementation follows AES-GCM with random nonce
- [ ] Input validation and sanitization implemented
- [ ] Error messages are sanitized (no credential exposure)
- [ ] Offline operation capability maintained
- [ ] No telemetry added

## Testing

- [ ] Unit tests added/updated
- [ ] Integration tests added/updated and verified to run offline in read-only mode
- [ ] Security tests added/updated
- [ ] All tests pass locally (`just test`)
- [ ] Container-backed adapter tests pass (`just test-integration`, needs Docker)
- [ ] `govulncheck` reports nothing reachable (`just vuln`)
- [ ] Offline operation verified

**Integration Test Requirements**: All integration tests must be verified to run offline and execute in read-only mode. Any network-write tests must be opt-in and documented.

## Code Quality

- [ ] Code follows the conventions in AGENTS.md and matches the surrounding style
- [ ] Zero `golangci-lint` issues (`just lint`)
- [ ] Code formatted (`just format`)
- [ ] Any `//nolint` names a specific linter and gives a reason
- [ ] Errors are wrapped with context, not discarded
- [ ] No new cgo dependency
- [ ] Source and Markdown are ASCII only

## Performance Impact

- [ ] No performance regression
- [ ] Memory usage remains efficient
- [ ] Database operations use connection pooling
- [ ] Large schema handling tested (>1000 tables)

## Breaking Changes

If this PR includes breaking changes:

- [ ] Breaking change documented in commit message
- [ ] Migration guide provided
- [ ] Version bump planned appropriately

## Related Issues

- Closes #[issue_number]
- Related to #[issue_number]

## Additional Notes

Any additional context, screenshots, or information that reviewers should know.

## Checklist for Reviewers

- [ ] Security implications reviewed
- [ ] Database safety verified (read-only operations)
- [ ] Offline operation capability confirmed
- [ ] Error handling and credential protection validated
- [ ] Performance impact assessed
- [ ] Documentation quality checked
- [ ] Test coverage adequate

---

**Note**: This PR must pass all CI checks including security validation before merging.
