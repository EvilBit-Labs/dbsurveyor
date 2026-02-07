# DBSurveyor Core Flows - Red Team and DBA Workflows

## Overview

This document defines the core user flows for DBSurveyor's primary personas: Red Team Operators and Database Administrators. Each flow captures the complete user journey from entry point through actions to completion, focusing on the product-level experience rather than technical implementation.

## Flow 1: Red Team Covert Collection

**Description**: Red Team operator performs covert database intelligence gathering with minimal detection footprint.

**Trigger**: Operator has obtained database credentials and needs to extract schema information without triggering alerts.

**Flow**:

1. Operator constructs single command with all stealth parameters:
   - Throttle delay to avoid slow query logs
   - Reduced sample size to minimize data transfer
   - Encryption to protect exfiltrated data
   - Custom output path to avoid default naming

2. Execute collection command:
   ```bash
   dbsurveyor-collect --throttle 1000 --sample 50 --encrypt --output intel.enc postgres://target/db
   ```

3. System operates silently (no progress output by default):
   - Connects to database with read-only operations
   - Applies throttling between queries (1000ms delay)
   - Collects limited samples (50 rows per table)
   - Prompts for encryption password (hidden input)
   - Prompts for password confirmation

4. On completion, system displays minimal summary:
   - "✓ Schema saved to intel.enc"
   - Tables/views/indexes count
   - No detailed logging unless `-v` flag used

5. Operator transfers encrypted file to analysis environment for offline processing.

**Exit**: Encrypted schema file ready for offline analysis, minimal operational footprint.

---

## Flow 2: DBA Multi-Database Collection

**Description**: Database administrator collects schemas from multiple databases on a server for comprehensive documentation.

**Trigger**: DBA needs to document a filtered set of production databases on a PostgreSQL server.

**Flow**:

1. DBA tests connection to verify credentials:
   ```bash
   dbsurveyor-collect test postgres://admin@prod-server/postgres
   ```
   System responds: "✓ Connection to PostgreSQL database successful"

2. DBA initiates multi-database collection with pg_dump-style, **flag-driven** filtering (no interactive prompts):
   ```bash
   dbsurveyor-collect --all-databases \
     --include-databases "app_*,analytics_*" \
     --exclude-databases "postgres,template*" \
     --output schemas.json \
     postgres://admin@prod-server
   ```
   Filtering rules:
   - `--include-databases` and `--exclude-databases` accept glob patterns.
   - If both are provided: apply **include first**, then remove matches from exclude.

3. System enumerates accessible databases and applies filters:
   - Default output is silent; `-v` shows per-database progress
   - Displays (in verbose): "Found 12 databases; collecting 7"

4. System collects each selected database sequentially:
   - Retries transient failures (up to 3 attempts) with exponential backoff and jitter
   - Skips permission/authorization failures immediately (no retry)
   - Records per-database and per-object failures for targeted re-runs

5. System writes output:
   - **Default**: single multi-database output file (bundle)
   - Includes per-database status (success/failed/skipped) and structured failure metadata

6. DBA reviews the recorded failures and optionally re-runs collection for a narrowed include set.

**Alternative - One File Per Database** (explicit opt-in):
```bash
dbsurveyor-collect --all-databases --one-per-database --output-dir ./schemas/ postgres://admin@prod-server
```
Creates: `schemas/<db>.json` per database.

**Exit**: Multi-database schema collection with deterministic filtering and machine-actionable failure reporting.

---

## Flow 3: DBA Quick Documentation Generation

**Description**: DBA generates human-readable documentation from collected schema files.

**Trigger**: DBA has collected schema files and needs Markdown documentation for team review.

**Flow**:

1. DBA generates documentation for a single schema file:
   ```bash
   dbsurveyor generate schema.json
   ```
   System auto-detects output filename: `schema.md`

2. System validates the input format and generates documentation using the configured redaction mode.

**Alternative - Batch Processing (Directory Mode)**:
```bash
dbsurveyor generate --directory ./schemas/ --format markdown
```
Processes all schema files in the directory and generates corresponding reports.

**Multi-Database Bundle Input (Single File)**:
If the input schema contains multiple databases, the user must choose the output behavior explicitly:
- `--combined`: generate one combined report with per-database sections
- `--split`: generate per-database reports into an output directory

**Exit**: Offline documentation ready for review and distribution.

---

## Flow 4: Encrypted Transfer Workflow

**Description**: Operator collects schema with encryption, transfers to air-gapped environment, and processes offline.

**Trigger**: Security analyst needs to document database in air-gapped environment.

**Flow**:

**Phase 1 - Collection (Connected Environment)**:

1. Analyst collects schema with encryption:
   ```bash
   dbsurveyor-collect --encrypt --compress postgres://db-server/production
   ```

2. System prompts for encryption password:
   - "Enter encryption password: " (hidden input)
   - "Confirm encryption password: " (hidden input)
   - Validates passwords match

3. System collects schema and encrypts output:
   - Uses AES-GCM with random nonce
   - Derives key using Argon2id
   - Compresses before encryption
   - Saves to `schema.dbsurveyor.enc`

**Phase 2 - Transfer (Air Gap)**:

4. Analyst transfers encrypted file via approved media (USB, secure file transfer).

**Phase 3 - Processing (Air-Gapped Environment)**:

5. Analyst generates documentation offline:
   ```bash
   dbsurveyor generate schema.dbsurveyor.enc --format markdown
   ```

6. System prompts for decryption password:
   - "Enter decryption password: " (hidden input)
   - Decrypts and decompresses data
   - Validates schema format

7. System generates documentation completely offline:
   - No network connectivity required
   - Applies redaction based on `--redact-mode` setting
   - Saves to `schema.md`

**Exit**: Secure documentation generated in air-gapped environment.

---

## Flow 5: Automated Collection with Environment Variables

**Description**: DBA sets up automated schema collection in CI/CD pipeline.

**Trigger**: DBA needs nightly schema backups without interactive prompts.

**Flow**:

1. DBA configures environment variables in CI/CD:
   ```bash
   export DATABASE_URL="postgres://backup-user@prod-server/app_db"
   export DBSURVEYOR_ENCRYPTION_KEY="$(cat /secure/encryption.key)"
   ```

2. CI/CD job executes collection:
   ```bash
   dbsurveyor-collect --encrypt --compress --output "backup-$(date +%Y%m%d).enc"
   ```

3. System uses environment variables:
   - Reads `DATABASE_URL` for connection
   - Reads `DBSURVEYOR_ENCRYPTION_KEY` for encryption (no prompt)
   - Operates non-interactively

4. System generates timestamped backup:
   - Collects schema silently
   - Encrypts using key from environment
   - Saves to `backup-20260207.enc`

5. CI/CD job archives backup to secure storage.

**Alternative - Key from File**:
```bash
dbsurveyor-collect --encrypt --key-file /secure/key.txt postgres://server/db
```

**Alternative - Key from Stdin**:
```bash
echo "$ENCRYPTION_KEY" | dbsurveyor-collect --encrypt --key-stdin postgres://server/db
```

**Exit**: Automated, non-interactive schema backup.

---

## Flow 6: Error Recovery and Partial Collection

**Description**: System handles failures gracefully during multi-database collection while remaining automation-friendly.

**Trigger**: DBA collects from 10 databases; one database and one table fail due to permissions.

**Flow**:

1. DBA initiates multi-database collection:
   ```bash
   dbsurveyor-collect --all-databases -v postgres://admin@server
   ```

2. System collects databases sequentially:
   - Database 1-4: Success
   - Database 5: Permission denied on table `sensitive_data`
   - Database 6: Intermittent timeout during view enumeration

3. System applies retry rules:
   - **Transient failures** (timeouts, connection hiccups): bounded retry (up to 3 attempts) with exponential backoff and jitter
   - **Permission/authorization failures**: do not retry; skip immediately and record

4. System continues collection and records structured failure metadata in the output:
   - Per-database status: success/failed/skipped
   - Per-object failures (when known): object type + identifier (e.g., table/schema) + error summary
   - Human-readable warnings list for quick review

5. Exit code semantics support automation:
   - Default behavior: exit **0** if at least one database succeeded
   - Optional strict mode (`--strict`): exit non-zero if *any* selected database fails

6. DBA uses recorded failure identifiers to re-run targeted collection:
   ```bash
   dbsurveyor-collect --all-databases --include-databases restricted_db --output restricted_db.json postgres://admin@server
   ```

**Exit**: Partial collection produces usable outputs plus machine-actionable failure metadata for targeted recovery.

---

## Flow 7: SQL DDL Reconstruction for Migration

**Description**: DBA reconstructs SQL DDL from collected schema for database migration.

**Trigger**: DBA needs to recreate database structure in new environment.

**Flow**:

1. DBA generates SQL DDL from schema file:
   ```bash
   dbsurveyor sql schema.json --dialect postgresql --output recreate.sql
   ```

2. System loads schema and generates DDL:
   - Parses table definitions
   - Generates CREATE TABLE statements
   - Includes constraints, indexes, foreign keys
   - Respects target SQL dialect

3. System saves DDL script:
   - Displays: "✓ SQL DDL generated: recreate.sql"
   - File contains executable SQL statements

4. DBA reviews and executes DDL in target environment:
   ```bash
   psql -h new-server -d new_db -f recreate.sql
   ```

**Exit**: Database structure recreated in new environment.

---

## Flow 8: Schema Analysis and Validation

**Description**: DBA analyzes collected schema for insights and validates file integrity.

**Trigger**: DBA receives schema file from colleague and needs to verify it before processing.

**Flow**:

1. DBA validates schema file:
   ```bash
   dbsurveyor validate schema.json
   ```

2. System validates file:
   - Checks JSON structure
   - Validates against JSON Schema specification
   - Verifies format version compatibility
   - Displays: "✓ Schema file is valid"

3. DBA analyzes schema for insights:
   ```bash
   dbsurveyor analyze schema.json --detailed
   ```

4. System displays analysis:
   - Database name and version
   - Object counts (tables, views, indexes, constraints)
   - Detailed statistics (procedures, functions, triggers, custom types)
   - Collection metadata (date, duration, warnings)

5. DBA reviews warnings:
   - System displays any collection warnings
   - Helps identify incomplete or problematic collections

**Exit**: Validated schema file with comprehensive analysis.

---

## Progress and Feedback Patterns

### Silent by Default
- No output during collection unless errors occur
- Final summary only: "✓ Schema saved to schema.json"
- Suitable for automation and stealth operations

### Verbose Mode (-v)
- Shows major milestones: "Connecting...", "Collecting tables...", "Sampling data..."
- Table-by-table progress for multi-table collections
- Suitable for interactive DBA workflows

### Very Verbose Mode (-vv)
- Detailed query logging
- Connection pool statistics
- Performance metrics
- Suitable for debugging and optimization

### Quiet Mode (--quiet)
- Suppresses all output except errors
- Exit code indicates success/failure
- Suitable for scripting and CI/CD

---

## Error Communication Patterns

### Connection Errors
- Clear message: "Failed to connect to database: Connection refused"
- Suggests: "Verify host, port, and credentials"
- Exit code: 5

### Permission Errors
- Clear message: "Permission denied on table 'users'"
- Suggests: "Ensure database user has SELECT privileges"
- Stores in failure metadata for partial collections

### Encryption Errors
- Clear message: "Decryption failed: Invalid password or corrupted file"
- Suggests: "Verify password or check file integrity"
- Exit code: 6

### Validation Errors
- Clear message: "Schema validation failed: Missing required field 'format_version'"
- Shows specific validation errors
- Helps identify corrupted or incompatible files

---

## Key UX Principles

1. **Security First**: Credentials never appear in logs, errors, or process lists
2. **Silent by Default**: Minimal output unless requested (stealth-friendly)
3. **Graceful Degradation**: Partial success better than complete failure
4. **Clear Feedback**: Errors include actionable suggestions
5. **Automation-Friendly**: Non-interactive modes for CI/CD
6. **Offline-Capable**: Postprocessor requires zero network connectivity
