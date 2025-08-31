---
name: Bug Report
about: Create a report to help us improve DBSurveyor
title: '[BUG] '
labels: [bug, needs-triage]
assignees: [UncleSp1d3r]
---

## Bug Description

A clear and concise description of what the bug is.

## Steps to Reproduce

1. Database type: [PostgreSQL/MySQL/SQLite/SQL Server/MongoDB]
2. Connection string format: `database://<user>:<password>@host:port/db` (⚠️ **NEVER include real credentials**)
3. Command executed: `dbsurveyor [command] [options]`
4. Expected behavior: What you expected to happen
5. Actual behavior: What actually happened

## Environment Information

- **OS**: [e.g., Ubuntu 22.04, macOS 14.0, Windows 11]
- **Architecture**: [e.g., x86_64, aarch64]
- **DBSurveyor Version**: [e.g., 0.1.0, commit hash]
- **Rust Version**: [e.g., 1.77.0]
- **Database Version**: [e.g., PostgreSQL 15.4, MySQL 8.0]

## Error Details

### Error Message

```text
[Paste the exact error message here]
```

### Log Output

```text
[Paste relevant log output here - ensure no credentials are included]
```

### Stack Trace (if applicable)

```text
[Paste stack trace here - ensure no credentials are included]
```

## Security Information

- [ ] No credentials or sensitive data included in this report
- [ ] Error messages have been sanitized
- [ ] Database connection details are generic examples
- [ ] No internal system information exposed

## Additional Context

- **Database Size**: [e.g., 100 tables, 1000 columns]
- **Schema Complexity**: [e.g., Many foreign keys, custom types, views]
- **Network Environment**: [e.g., Local, VPN, air-gapped]
- **Previous Working State**: [e.g., Worked in version X, broke in version Y]

## Impact Assessment

- **Severity**: [Critical/High/Medium/Low]
- **Affected Users**: [e.g., All PostgreSQL users, specific database types]
- **Workaround Available**: [Yes/No] - Describe if applicable

## Files Attached

- [ ] Schema file (`.dbsurveyor.json`) - sanitized if needed
- [ ] Configuration file - sanitized if needed
- [ ] Screenshots (if applicable)

## Reproduction Data

If possible, provide a minimal reproduction case:

```bash
# Minimal reproduction command
dbsurveyor [command] [options]

# Expected output
[Expected output]

# Actual output
[Actual output]
```

---

**Note**: Please ensure all information provided is sanitized and contains no sensitive data such as credentials, internal IPs, or proprietary information.
