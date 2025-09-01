# Configuration

DBSurveyor can be configured through command-line options, environment variables, and configuration files. This guide covers all configuration options and best practices.

## Environment Variables

### Database Connection

```bash
# Primary database connection
export DATABASE_URL="postgres://user:password@localhost:5432/mydb"

# Use with collector without specifying URL
dbsurveyor-collect
```

### Logging Configuration

```bash
# Set log level for all modules
export RUST_LOG=info

# Set log level for specific modules
export RUST_LOG=dbsurveyor_collect=debug,dbsurveyor_core=trace

# Disable colored output (useful for CI)
export NO_COLOR=1

# Log levels: error, warn, info, debug, trace
export RUST_LOG=debug
```

### Security Configuration

```bash
# Disable credential prompts (use with caution)
export DBSURVEYOR_NO_PROMPT=1

# Default encryption password (NOT RECOMMENDED for production)
export DBSURVEYOR_ENCRYPTION_PASSWORD="your-password"
```

## Command-Line Configuration

### Global Options

Available for both `dbsurveyor-collect` and `dbsurveyor`:

```bash
# Verbosity levels
dbsurveyor-collect -v postgres://localhost/db      # Info level
dbsurveyor-collect -vv postgres://localhost/db     # Debug level
dbsurveyor-collect -vvv postgres://localhost/db    # Trace level

# Quiet mode (errors only)
dbsurveyor-collect -q postgres://localhost/db

# Help and version
dbsurveyor-collect --help
dbsurveyor-collect --version
```

### Collection Configuration

#### Connection Settings

```bash
# Connection timeout (default: 30s)
dbsurveyor-collect --connect-timeout 60 postgres://localhost/db

# Query timeout (default: 30s)
dbsurveyor-collect --query-timeout 45 postgres://localhost/db

# Maximum connections (default: 10)
dbsurveyor-collect --max-connections 5 postgres://localhost/db
```

#### Data Collection Settings

```bash
# Sample size per table (default: 100, 0 to disable)
dbsurveyor-collect --sample 50 postgres://localhost/db

# Throttle delay between operations in milliseconds
dbsurveyor-collect --throttle 1000 postgres://localhost/db

# Include/exclude specific object types
dbsurveyor-collect --no-views postgres://localhost/db
dbsurveyor-collect --no-procedures postgres://localhost/db
dbsurveyor-collect --no-triggers postgres://localhost/db
```

#### Multi-Database Settings

```bash
# Collect all databases
dbsurveyor-collect --all-databases postgres://localhost

# Include system databases
dbsurveyor-collect --all-databases --include-system-databases postgres://localhost

# Exclude specific databases
dbsurveyor-collect --all-databases --exclude-databases "postgres,template0,template1" postgres://localhost
```

### Documentation Configuration

#### Output Format Settings

```bash
# Output format
dbsurveyor --format html schema.json
dbsurveyor --format markdown schema.json
dbsurveyor --format json schema.json
dbsurveyor --format mermaid schema.json

# Custom output file
dbsurveyor --output custom-name.html --format html schema.json
```

#### Redaction Settings

```bash
# Redaction modes
dbsurveyor --redact-mode none schema.json        # No redaction
dbsurveyor --redact-mode minimal schema.json     # Minimal redaction
dbsurveyor --redact-mode balanced schema.json    # Balanced (default)
dbsurveyor --redact-mode conservative schema.json # Maximum redaction

# Disable all redaction
dbsurveyor --no-redact schema.json
```

## Configuration Files

### Workspace Configuration

Create a `.dbsurveyor.toml` file in your project root:

```toml
# .dbsurveyor.toml
[collection]
# Default connection settings
connect_timeout = "30s"
query_timeout = "30s"
max_connections = 10
sample_size = 100
throttle_ms = 0

# Object collection settings
include_views = true
include_procedures = true
include_functions = true
include_triggers = true
include_indexes = true
include_constraints = true
include_custom_types = true

# Multi-database settings
include_system_databases = false
exclude_databases = ["postgres", "template0", "template1"]

[output]
# Default output settings
format = "markdown"
compression = false
encryption = false

[redaction]
# Data redaction settings
mode = "balanced"
custom_patterns = [
    { pattern = "(?i)(api[_-]?key)", replacement = "[API_KEY]" },
    { pattern = "(?i)(token)", replacement = "[TOKEN]" },
]

[security]
# Security settings
read_only = true
sanitize_credentials = true
```

### User Configuration

Create a global configuration file at `~/.config/dbsurveyor/config.toml`:

```toml
# ~/.config/dbsurveyor/config.toml
[defaults]
# Default database type for ambiguous connections
database_type = "postgresql"

# Default output directory
output_dir = "~/Documents/database-docs"

# Default logging level
log_level = "info"

[security]
# Security preferences
always_encrypt = false
strong_passwords_only = true
credential_timeout = "5m"

[templates]
# Custom template directory
template_dir = "~/.config/dbsurveyor/templates"
```

## Database-Specific Configuration

### PostgreSQL Configuration

```bash
# SSL/TLS settings
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=require"
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=verify-full&sslcert=client.crt&sslkey=client.key"

# Connection pool settings
dbsurveyor-collect "postgres://user:pass@localhost/db?pool_max_conns=5&pool_timeout=30"

# Schema-specific collection
dbsurveyor-collect "postgres://user:pass@localhost/db?search_path=public,custom"
```

### MySQL Configuration

```bash
# SSL settings
dbsurveyor-collect "mysql://user:pass@localhost/db?ssl-mode=REQUIRED"

# Character set
dbsurveyor-collect "mysql://user:pass@localhost/db?charset=utf8mb4"

# Connection timeout
dbsurveyor-collect "mysql://user:pass@localhost/db?connect_timeout=30"
```

### SQLite Configuration

```bash
# Read-only mode
dbsurveyor-collect "sqlite:///path/to/db.sqlite?mode=ro"

# Busy timeout
dbsurveyor-collect "sqlite:///path/to/db.sqlite?busy_timeout=30000"

# Cache size
dbsurveyor-collect "sqlite:///path/to/db.sqlite?cache_size=2000"
```

### MongoDB Configuration

```bash
# Authentication database
dbsurveyor-collect "mongodb://user:pass@localhost/mydb?authSource=admin"

# SSL settings
dbsurveyor-collect "mongodb://user:pass@localhost/mydb?ssl=true&sslVerify=false"

# Connection timeout
dbsurveyor-collect "mongodb://user:pass@localhost/mydb?connectTimeoutMS=30000"
```

## Advanced Configuration

### Custom Redaction Patterns

Create custom redaction rules:

```toml
# .dbsurveyor.toml
[redaction.patterns]
# Credit card numbers
credit_card = { pattern = "\\d{4}[\\s-]?\\d{4}[\\s-]?\\d{4}[\\s-]?\\d{4}", replacement = "[CREDIT_CARD]" }

# Social Security Numbers
ssn = { pattern = "\\d{3}-\\d{2}-\\d{4}", replacement = "[SSN]" }

# Phone numbers
phone = { pattern = "\\(\\d{3}\\)\\s?\\d{3}-\\d{4}", replacement = "[PHONE]" }

# Custom API keys
api_key = { pattern = "(?i)api[_-]?key[\"']?\\s*[:=]\\s*[\"']?([a-zA-Z0-9]{32,})", replacement = "[API_KEY]" }
```

### Performance Tuning

```toml
# .dbsurveyor.toml
[performance]
# Connection pool settings
max_connections = 10
min_connections = 2
connection_timeout = "30s"
idle_timeout = "10m"
max_lifetime = "1h"

# Query settings
query_timeout = "30s"
batch_size = 1000
max_concurrent_queries = 5

# Memory settings
max_memory_mb = 512
streaming_threshold_mb = 100
```

### Output Customization

```toml
# .dbsurveyor.toml
[output.html]
# HTML-specific settings
theme = "dark"
include_search = true
include_toc = true
syntax_highlighting = true

[output.markdown]
# Markdown-specific settings
include_toc = true
table_format = "github"
code_blocks = true

[output.json]
# JSON-specific settings
pretty_print = true
include_metadata = true
compress = false
```

## Environment-Specific Configuration

### Development Environment

```bash
# .env.development
DATABASE_URL=postgres://dev_user:dev_pass@localhost:5432/dev_db
RUST_LOG=debug
DBSURVEYOR_SAMPLE_SIZE=10
DBSURVEYOR_THROTTLE=0
```

### Production Environment

```bash
# .env.production
DATABASE_URL=postgres://readonly_user:secure_pass@prod-db:5432/prod_db
RUST_LOG=warn
DBSURVEYOR_SAMPLE_SIZE=0
DBSURVEYOR_THROTTLE=1000
DBSURVEYOR_ENCRYPT=true
```

### CI/CD Environment

```bash
# .env.ci
DATABASE_URL=postgres://ci_user:ci_pass@ci-db:5432/test_db
RUST_LOG=info
NO_COLOR=1
DBSURVEYOR_NO_PROMPT=1
DBSURVEYOR_OUTPUT_FORMAT=json
```

## Configuration Validation

Validate your configuration:

```bash
# Check configuration syntax
dbsurveyor-collect --check-config

# Dry run with configuration
dbsurveyor-collect --dry-run postgres://localhost/db

# Show effective configuration
dbsurveyor-collect --show-config
```

## Security Best Practices

### Credential Management

```bash
# Use environment variables instead of command line
export DATABASE_URL="postgres://user:pass@localhost/db"
dbsurveyor-collect

# Use credential files with restricted permissions
echo "postgres://user:pass@localhost/db" > ~/.dbsurveyor/credentials
chmod 600 ~/.dbsurveyor/credentials
dbsurveyor-collect --credentials-file ~/.dbsurveyor/credentials
```

### Configuration File Security

```bash
# Secure configuration files
chmod 600 .dbsurveyor.toml
chmod 600 ~/.config/dbsurveyor/config.toml

# Use environment variable substitution
# .dbsurveyor.toml
[collection]
database_url = "${DATABASE_URL}"
encryption_password = "${ENCRYPTION_PASSWORD}"
```

### Audit Configuration

```bash
# Log configuration usage
export RUST_LOG=dbsurveyor_core::config=debug

# Review effective configuration
dbsurveyor-collect --show-config --dry-run
```

## Troubleshooting Configuration

### Common Issues

**Configuration not found**:

```bash
# Check configuration file locations
ls -la .dbsurveyor.toml
ls -la ~/.config/dbsurveyor/config.toml

# Use explicit configuration file
dbsurveyor-collect --config custom-config.toml
```

**Environment variable not recognized**:

```bash
# Check environment variable names (case sensitive)
env | grep DBSURVEYOR
env | grep DATABASE_URL

# Verify variable export
echo $DATABASE_URL
```

**Permission denied**:

```bash
# Check file permissions
ls -la .dbsurveyor.toml
chmod 644 .dbsurveyor.toml  # Make readable
```

### Debug Configuration

```bash
# Show all configuration sources
dbsurveyor-collect --show-config --verbose

# Test configuration parsing
dbsurveyor-collect --check-config --verbose

# Trace configuration loading
export RUST_LOG=dbsurveyor_core::config=trace
dbsurveyor-collect --dry-run
```

## Migration and Upgrades

### Configuration Migration

When upgrading DBSurveyor versions:

```bash
# Backup existing configuration
cp .dbsurveyor.toml .dbsurveyor.toml.backup

# Check for configuration changes
dbsurveyor-collect --check-config --verbose

# Migrate configuration format
dbsurveyor-collect --migrate-config
```

### Version Compatibility

```toml
# .dbsurveyor.toml
[meta]
# Specify minimum required version
min_version = "0.1.0"
config_version = "1.0"
```

This comprehensive configuration system allows you to customize DBSurveyor for your specific needs while maintaining security and performance.
