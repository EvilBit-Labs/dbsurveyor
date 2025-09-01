# CLI Reference

Complete reference for all DBSurveyor command-line options.

## dbsurveyor-collect

Database schema collection tool.

### Synopsis

```bash
dbsurveyor-collect [OPTIONS] [DATABASE_URL]
dbsurveyor-collect <COMMAND>
```

### Global Options

| Option      | Short | Description                        |
| ----------- | ----- | ---------------------------------- |
| `--verbose` | `-v`  | Increase verbosity (-v, -vv, -vvv) |
| `--quiet`   | `-q`  | Suppress all output except errors  |
| `--help`    | `-h`  | Print help information             |
| `--version` | `-V`  | Print version information          |

### Collection Options

| Option                       | Description                      | Default                     |
| ---------------------------- | -------------------------------- | --------------------------- |
| `--database-url <URL>`       | Database connection string       | From `DATABASE_URL` env var |
| `--output <PATH>`            | Output file path                 | `schema.dbsurveyor.json`    |
| `--sample <N>`               | Number of sample rows per table  | `100`                       |
| `--throttle <MS>`            | Delay between operations (ms)    | None                        |
| `--compress`                 | Compress output using Zstandard  | `false`                     |
| `--encrypt`                  | Encrypt output using AES-GCM     | `false`                     |
| `--all-databases`            | Collect all accessible databases | `false`                     |
| `--include-system-databases` | Include system databases         | `false`                     |
| `--exclude-databases <LIST>` | Comma-separated list to exclude  | None                        |

### Commands

#### collect

Collect schema from database.

```bash
dbsurveyor-collect collect <DATABASE_URL> [--output <PATH>]
```

#### test

Test database connection without collecting schema.

```bash
dbsurveyor-collect test <DATABASE_URL>
```

#### list

List supported database types and connection formats.

```bash
dbsurveyor-collect list
```

### Examples

```bash
# Basic collection
dbsurveyor-collect postgres://user:pass@localhost/db

# With custom output file
dbsurveyor-collect --output my-schema.json postgres://localhost/db

# Encrypted and compressed
dbsurveyor-collect --encrypt --compress postgres://localhost/db

# Multi-database collection
dbsurveyor-collect --all-databases --exclude-databases postgres,template0 postgres://localhost

# Test connection only
dbsurveyor-collect test mysql://root:password@localhost/mydb

# Throttled collection (stealth mode)
dbsurveyor-collect --throttle 1000 postgres://localhost/db
```

### Connection String Formats

| Database   | Format                              | Example                                       |
| ---------- | ----------------------------------- | --------------------------------------------- |
| PostgreSQL | `postgres://user:pass@host:port/db` | `postgres://admin:secret@localhost:5432/mydb` |
| MySQL      | `mysql://user:pass@host:port/db`    | `mysql://root:password@localhost:3306/mydb`   |
| SQLite     | `sqlite:///path/to/file`            | `sqlite:///home/user/data.db`                 |
| MongoDB    | `mongodb://user:pass@host:port/db`  | `mongodb://admin:secret@localhost:27017/mydb` |
| SQL Server | `mssql://user:pass@host:port/db`    | `mssql://sa:password@localhost:1433/mydb`     |

### Environment Variables

| Variable       | Description                                                       |
| -------------- | ----------------------------------------------------------------- |
| `DATABASE_URL` | Default database connection string                                |
| `RUST_LOG`     | Logging configuration (`error`, `warn`, `info`, `debug`, `trace`) |

---

## dbsurveyor

Database schema documentation and analysis tool.

### Synopsis

```bash
dbsurveyor [OPTIONS] [INPUT_FILE]
dbsurveyor <COMMAND>
```

### Global Options

| Option      | Short | Description                        |
| ----------- | ----- | ---------------------------------- |
| `--verbose` | `-v`  | Increase verbosity (-v, -vv, -vvv) |
| `--quiet`   | `-q`  | Suppress all output except errors  |
| `--help`    | `-h`  | Print help information             |
| `--version` | `-V`  | Print version information          |

### Documentation Options

| Option                 | Short | Description                | Default       |
| ---------------------- | ----- | -------------------------- | ------------- |
| `--format <FORMAT>`    | `-f`  | Output format              | `markdown`    |
| `--output <PATH>`      | `-o`  | Output file path           | Auto-detected |
| `--redact-mode <MODE>` |       | Data redaction level       | `balanced`    |
| `--no-redact`          |       | Disable all data redaction | `false`       |

### Output Formats

| Format     | Description             | Extension |
| ---------- | ----------------------- | --------- |
| `markdown` | Markdown documentation  | `.md`     |
| `html`     | HTML report with search | `.html`   |
| `json`     | JSON analysis report    | `.json`   |
| `mermaid`  | Mermaid ERD diagram     | `.mmd`    |

### Redaction Modes

| Mode           | Description                                       |
| -------------- | ------------------------------------------------- |
| `none`         | No redaction (show all data)                      |
| `minimal`      | Minimal redaction (only obvious sensitive fields) |
| `balanced`     | Balanced redaction (recommended default)          |
| `conservative` | Conservative redaction (maximum privacy)          |

### Commands

#### generate

Generate documentation from schema file.

```bash
dbsurveyor generate <INPUT_FILE> [OPTIONS]
```

**Options:**

- `--format <FORMAT>` - Output format
- `--output <PATH>` - Output file path

#### analyze

Analyze schema for insights and statistics.

```bash
dbsurveyor analyze <INPUT_FILE> [--detailed]
```

**Options:**

- `--detailed` - Show detailed analysis statistics

#### sql

Reconstruct SQL DDL from schema.

```bash
dbsurveyor sql <INPUT_FILE> [OPTIONS]
```

**Options:**

- `--dialect <DIALECT>` - Target SQL dialect (default: `postgresql`)
- `--output <PATH>` - Output file path

**SQL Dialects:**

- `postgresql` - PostgreSQL dialect
- `mysql` - MySQL dialect
- `sqlite` - SQLite dialect
- `sqlserver` - SQL Server dialect
- `generic` - Generic SQL (ANSI standard)

#### validate

Validate schema file format.

```bash
dbsurveyor validate <INPUT_FILE>
```

### Examples

```bash
# Generate Markdown documentation
dbsurveyor generate schema.dbsurveyor.json

# Generate HTML report
dbsurveyor --format html --output report.html schema.json

# Process encrypted schema (will prompt for password)
dbsurveyor generate schema.enc

# Generate SQL DDL for MySQL
dbsurveyor sql schema.json --dialect mysql --output recreate.sql

# Analyze schema with detailed statistics
dbsurveyor analyze schema.json --detailed

# Validate schema file format
dbsurveyor validate schema.dbsurveyor.json

# Generate with conservative redaction
dbsurveyor --redact-mode conservative schema.json

# Generate without any redaction
dbsurveyor --no-redact schema.json
```

### Input File Formats

DBSurveyor automatically detects input file formats:

| Extension | Format     | Description               |
| --------- | ---------- | ------------------------- |
| `.json`   | JSON       | Standard schema format    |
| `.zst`    | Compressed | Zstandard compressed JSON |
| `.enc`    | Encrypted  | AES-GCM encrypted JSON    |

### Exit Codes

| Code | Description                  |
| ---- | ---------------------------- |
| `0`  | Success                      |
| `1`  | General error                |
| `2`  | Invalid arguments            |
| `3`  | File not found               |
| `4`  | Permission denied            |
| `5`  | Database connection failed   |
| `6`  | Encryption/decryption failed |

### Environment Variables

| Variable   | Description            |
| ---------- | ---------------------- |
| `RUST_LOG` | Logging configuration  |
| `NO_COLOR` | Disable colored output |

## Common Usage Patterns

### Secure Workflow

```bash
# 1. Test connection
dbsurveyor-collect test postgres://user:pass@localhost/db

# 2. Collect with encryption
dbsurveyor-collect --encrypt postgres://user:pass@localhost/db

# 3. Generate documentation offline
dbsurveyor generate schema.enc

# 4. Validate output
dbsurveyor validate schema.enc
```

### Multi-Database Documentation

```bash
# Collect from multiple databases
dbsurveyor-collect --all-databases postgres://localhost > collection.log

# Generate comprehensive report
dbsurveyor --format html --output full-report.html schema.dbsurveyor.json

# Extract SQL for specific database
dbsurveyor sql schema.json --dialect postgresql --output postgres-ddl.sql
```

### Development Workflow

```bash
# Quick collection and documentation
dbsurveyor-collect sqlite://dev.db && dbsurveyor generate schema.dbsurveyor.json

# Analyze changes
dbsurveyor analyze schema.dbsurveyor.json --detailed

# Generate multiple formats
for format in markdown html mermaid; do
    dbsurveyor --format $format schema.dbsurveyor.json
done
```
