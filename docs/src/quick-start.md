# Quick Start

This guide will get you up and running with DBSurveyor in minutes.

## Prerequisites

- DBSurveyor installed (see [Installation](./installation.md))
- Access to a database (PostgreSQL, MySQL, SQLite, or MongoDB)

## Basic Workflow

DBSurveyor follows a two-step process:

1. **Collect** schema from database → schema file
2. **Generate** documentation from schema file → reports

```mermaid
graph LR
    A[Database] --> B[dbsurveyor-collect]
    B --> C[schema.dbsurveyor.json]
    C --> D[dbsurveyor]
    D --> E[Documentation]
```

## Example 1: SQLite Database

The simplest example using SQLite (no server setup required):

```bash
# Create a sample SQLite database
sqlite3 sample.db << EOF
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL,
    email TEXT UNIQUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE posts (
    id INTEGER PRIMARY KEY,
    user_id INTEGER,
    title TEXT NOT NULL,
    content TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

INSERT INTO users (username, email) VALUES
    ('alice', 'alice@example.com'),
    ('bob', 'bob@example.com');
EOF

# Step 1: Collect schema
dbsurveyor-collect sqlite://sample.db

# Step 2: Generate documentation
dbsurveyor generate schema.dbsurveyor.json

# View the generated documentation
cat schema.md
```

## Example 2: PostgreSQL Database

```bash
# Step 1: Collect schema from PostgreSQL
dbsurveyor-collect postgres://user:password@localhost:5432/mydb

# Step 2: Generate Markdown documentation (HTML is placeholder)
dbsurveyor generate schema.dbsurveyor.json --format markdown

# View the generated documentation
cat schema.md
```

## Example 3: Encrypted Schema Collection

For sensitive environments, encrypt the schema file:

```bash
# Collect with encryption (will prompt for password)
dbsurveyor-collect --encrypt postgres://user:pass@localhost/db

# Generate documentation from encrypted file (will prompt for password)
dbsurveyor generate schema.enc
```

## Common Commands

### Collection Commands

```bash
# Basic collection
dbsurveyor-collect <database-url>

# Test connection without collecting
dbsurveyor-collect test <database-url>

# Collect with compression
dbsurveyor-collect --compress <database-url>

# Collect all databases on server
dbsurveyor-collect --all-databases postgres://user:pass@localhost

# List supported database types
dbsurveyor-collect list
```

### Documentation Commands

```bash
# Generate Markdown (default)
dbsurveyor generate schema.dbsurveyor.json

# Generate HTML report
dbsurveyor --format html schema.json

# Generate SQL DDL
dbsurveyor sql schema.json --dialect postgresql

# Analyze schema statistics
dbsurveyor analyze schema.json --detailed

# Validate schema file
dbsurveyor validate schema.json
```

## Output Files

DBSurveyor generates several types of files:

### Schema Files (from collector)

- `schema.dbsurveyor.json` - Standard JSON format ([specification](./json-schema-specification.md))
- `schema.dbsurveyor.json.zst` - Compressed format (with `--compress`)
- `schema.enc` - Encrypted format (with `--encrypt`)

### Documentation Files (from generator)

- `schema.md` - Markdown documentation (✅ implemented)
- `schema_analysis.json` - Analysis report (✅ implemented)
- `schema.html` - HTML report with search (🚧 placeholder)
- `schema.sql` - SQL DDL reconstruction (🚧 placeholder)
- `schema.mmd` - Mermaid ERD diagram (🚧 placeholder)

## Next Steps

Now that you have the basics working:

1. **Explore Database Support**: Learn about [database-specific features](./database-support.md)
2. **Security Features**: Understand [encryption and security options](./security.md)
3. **Advanced Configuration**: Customize behavior with [configuration options](./configuration.md)
4. **CLI Reference**: Master all available [command-line options](./cli-reference.md)

## Troubleshooting Quick Start Issues

### Connection Issues

```bash
# Test connection first
dbsurveyor-collect test postgres://user:pass@localhost/db

# Check if database is accessible
ping localhost
telnet localhost 5432  # PostgreSQL default port
```

### Permission Issues

```bash
# Ensure user has read permissions
# PostgreSQL: GRANT SELECT ON ALL TABLES IN SCHEMA public TO username;
# MySQL: GRANT SELECT ON database.* TO 'username'@'%';
```

### File Not Found

```bash
# Check if schema file was created
ls -la *.dbsurveyor.json *.enc *.zst

# Verify file format
file schema.dbsurveyor.json
```

For more detailed troubleshooting, see the [Troubleshooting Guide](./troubleshooting.md).
