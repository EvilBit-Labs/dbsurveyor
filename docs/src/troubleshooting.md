# Troubleshooting

This guide helps you diagnose and resolve common issues with DBSurveyor.

## Quick Diagnostics

### Check Installation

```bash
# Verify binaries are installed and working
dbsurveyor-collect --version
dbsurveyor --version

# Check compiled features
dbsurveyor-collect list

# Test with minimal example
echo "CREATE TABLE test (id INTEGER);" | sqlite3 test.db
dbsurveyor-collect sqlite://test.db
rm test.db schema.dbsurveyor.json
```

### Enable Debug Logging

```bash
# Enable debug logging for all modules
export RUST_LOG=debug
dbsurveyor-collect postgres://localhost/db

# Enable trace logging for specific modules
export RUST_LOG=dbsurveyor_collect=trace,dbsurveyor_core=debug
dbsurveyor-collect postgres://localhost/db

# Log to file
export RUST_LOG=debug
dbsurveyor-collect postgres://localhost/db 2> debug.log
```

## Connection Issues

### Database Connection Failures

**Symptoms**: Connection timeouts, authentication failures, network errors

#### PostgreSQL Connection Issues

```bash
# Test basic connectivity
ping localhost
telnet localhost 5432

# Test with psql
psql -h localhost -U user -d db -c "SELECT 1;"

# Check PostgreSQL logs
sudo tail -f /var/log/postgresql/postgresql-*.log

# Common connection string issues
# ❌ Wrong: postgres://user:pass@localhost/db:5432
# ✅ Correct: postgres://user:pass@localhost:5432/db

# SSL issues
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=disable"
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=require"
```

#### MySQL Connection Issues

```bash
# Test basic connectivity
telnet localhost 3306

# Test with mysql client
mysql -h localhost -u user -p -e "SELECT 1;" db

# Check MySQL logs
sudo tail -f /var/log/mysql/error.log

# Common issues
# Character set problems
dbsurveyor-collect "mysql://user:pass@localhost/db?charset=utf8mb4"

# SSL issues
dbsurveyor-collect "mysql://user:pass@localhost/db?ssl-mode=DISABLED"
```

#### SQLite Connection Issues

```bash
# Check file exists and is readable
ls -la /path/to/database.db
file /path/to/database.db

# Test with sqlite3 command
sqlite3 /path/to/database.db ".tables"

# Permission issues
chmod 644 /path/to/database.db

# Use absolute paths
dbsurveyor-collect "sqlite:///$(pwd)/database.db"
```

#### MongoDB Connection Issues

```bash
# Test basic connectivity
telnet localhost 27017

# Test with mongo client
mongosh "mongodb://user:pass@localhost:27017/db"

# Authentication issues
dbsurveyor-collect "mongodb://user:pass@localhost/db?authSource=admin"

# Replica set issues
dbsurveyor-collect "mongodb://user:pass@host1,host2/db?replicaSet=rs0"
```

### Permission and Authentication Issues

**Symptoms**: Access denied, insufficient privileges, authentication failed

#### PostgreSQL Permissions

```sql
-- Check current user permissions
SELECT current_user, session_user;

-- Check database access
SELECT datname FROM pg_database WHERE datname = 'your_db';

-- Check table permissions
SELECT schemaname, tablename 
FROM pg_tables 
WHERE schemaname NOT IN ('information_schema', 'pg_catalog');

-- Grant necessary permissions
GRANT CONNECT ON DATABASE mydb TO dbsurveyor_user;
GRANT USAGE ON SCHEMA public TO dbsurveyor_user;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO dbsurveyor_user;
```

#### MySQL Permissions

```sql
-- Check current user
SELECT USER(), CURRENT_USER();

-- Check permissions
SHOW GRANTS FOR 'dbsurveyor_user'@'%';

-- Grant necessary permissions
GRANT SELECT ON mydb.* TO 'dbsurveyor_user'@'%';
GRANT SELECT ON information_schema.* TO 'dbsurveyor_user'@'%';
FLUSH PRIVILEGES;
```

#### MongoDB Permissions

```javascript
// Check current user
db.runCommand({connectionStatus: 1})

// Check permissions
use mydb
db.runCommand({usersInfo: "dbsurveyor_user"})

// Grant read permissions
use admin
db.grantRolesToUser("dbsurveyor_user", [
  { role: "read", db: "mydb" }
])
```

## Collection Issues

### Schema Collection Failures

**Symptoms**: Partial collection, missing objects, collection errors

#### Missing Tables or Objects

```bash
# Enable verbose logging to see what's being collected
dbsurveyor-collect -vvv postgres://localhost/db

# Check if objects exist in database
psql -h localhost -U user -d db -c "\dt"  # PostgreSQL tables
mysql -u user -p -e "SHOW TABLES;" db     # MySQL tables
sqlite3 db.sqlite ".tables"               # SQLite tables
```

#### Large Database Timeouts

```bash
# Increase timeouts
dbsurveyor-collect --connect-timeout 60 --query-timeout 120 postgres://localhost/db

# Use throttling to reduce load
dbsurveyor-collect --throttle 1000 postgres://localhost/db

# Disable sample collection for speed
dbsurveyor-collect --sample 0 postgres://localhost/db
```

#### Memory Issues

```bash
# Monitor memory usage
top -p $(pgrep dbsurveyor-collect)

# Use compression to reduce memory usage
dbsurveyor-collect --compress postgres://localhost/db

# Process smaller chunks (multi-database collection)
dbsurveyor-collect --exclude-databases large_db1,large_db2 postgres://localhost
```

### Output File Issues

**Symptoms**: Empty files, corrupted output, permission errors

#### File Permission Issues

```bash
# Check output directory permissions
ls -la $(dirname schema.dbsurveyor.json)

# Ensure write permissions
chmod 755 $(dirname schema.dbsurveyor.json)

# Use explicit output path
dbsurveyor-collect --output /tmp/schema.json postgres://localhost/db
```

#### Corrupted Output Files

```bash
# Validate schema file
dbsurveyor validate schema.dbsurveyor.json

# Check file size and format
ls -la schema.dbsurveyor.json
file schema.dbsurveyor.json

# Test JSON parsing
jq . schema.dbsurveyor.json > /dev/null
```

#### Compression/Encryption Issues

```bash
# Test compression
dbsurveyor-collect --compress postgres://localhost/db
zstd -t schema.dbsurveyor.json.zst  # Test compressed file

# Test encryption (will prompt for password)
dbsurveyor-collect --encrypt postgres://localhost/db
dbsurveyor validate schema.enc  # Test encrypted file
```

## Documentation Generation Issues

### Processing Failures

**Symptoms**: Generation errors, empty output, format issues

#### Input File Issues

```bash
# Validate input file first
dbsurveyor validate schema.dbsurveyor.json

# Check file format
file schema.dbsurveyor.json

# Test with minimal processing
dbsurveyor --format json schema.dbsurveyor.json
```

#### Encrypted File Issues

```bash
# Verify encryption format
dbsurveyor validate schema.enc

# Test decryption
dbsurveyor --format json schema.enc

# Check password
# Note: Passwords are case-sensitive and don't show characters
```

#### Large Schema Processing

```bash
# Use JSON format for large schemas (most efficient)
dbsurveyor --format json large-schema.json

# Monitor memory usage
top -p $(pgrep dbsurveyor)

# Process in parts (planned feature)
# Currently: split large schemas manually
```

### Output Format Issues

**Symptoms**: Malformed output, missing content, rendering problems

#### HTML Generation Issues

```bash
# Test with simple format first
dbsurveyor --format markdown schema.json

# Check HTML output
dbsurveyor --format html schema.json
# Open in browser to check for issues
```

#### Mermaid Diagram Issues

```bash
# Generate Mermaid format
dbsurveyor --format mermaid schema.json

# Validate Mermaid syntax
# Copy content to https://mermaid.live/ for testing
```

## Performance Issues

### Slow Collection

**Symptoms**: Long collection times, high resource usage

#### Database Performance

```bash
# Use connection pooling
dbsurveyor-collect --max-connections 5 postgres://localhost/db

# Add throttling
dbsurveyor-collect --throttle 500 postgres://localhost/db

# Disable sample collection
dbsurveyor-collect --sample 0 postgres://localhost/db

# Monitor database load
# PostgreSQL: SELECT * FROM pg_stat_activity;
# MySQL: SHOW PROCESSLIST;
```

#### Network Performance

```bash
# Test network latency
ping database-host

# Use local connections when possible
dbsurveyor-collect postgres://localhost/db  # Better than remote

# Consider compression for remote databases
dbsurveyor-collect --compress postgres://remote-host/db
```

### Memory Usage

**Symptoms**: High memory consumption, out of memory errors

#### Memory Optimization

```bash
# Monitor memory usage
ps aux | grep dbsurveyor
top -p $(pgrep dbsurveyor)

# Use streaming for large datasets (automatic)
# Reduce sample size
dbsurveyor-collect --sample 10 postgres://localhost/db

# Use compression
dbsurveyor-collect --compress postgres://localhost/db
```

## Security Issues

### Credential Exposure

**Symptoms**: Passwords in logs, credential leakage

#### Verify Credential Sanitization

```bash
# Check logs for credentials
export RUST_LOG=debug
dbsurveyor-collect postgres://user:secret@localhost/db 2>&1 | grep -i secret
# Should return no results

# Test credential sanitization
dbsurveyor-collect test postgres://user:secret@localhost/db
# Should show: postgres://user:****@localhost/db
```

#### Secure Credential Handling

```bash
# Use environment variables
export DATABASE_URL="postgres://user:secret@localhost/db"
dbsurveyor-collect

# Avoid shell history
set +o history  # Disable history
dbsurveyor-collect postgres://user:secret@localhost/db
set -o history  # Re-enable history
```

### Encryption Issues

**Symptoms**: Encryption failures, decryption errors

#### Test Encryption

```bash
# Test encryption roundtrip
echo "test data" > test.txt
dbsurveyor-collect --encrypt sqlite://test.db
dbsurveyor validate schema.enc
rm test.db test.txt schema.enc
```

#### Password Issues

```bash
# Ensure password consistency
# Passwords are case-sensitive
# No visual feedback during password entry (security feature)

# Test with simple password first
# Use ASCII characters only initially
```

## Build and Installation Issues

### Compilation Failures

**Symptoms**: Build errors, missing dependencies, feature conflicts

#### Rust Toolchain Issues

```bash
# Update Rust toolchain
rustup update

# Check Rust version (minimum 1.87)
rustc --version

# Clean and rebuild
cargo clean
cargo build --release
```

#### Feature Compilation Issues

```bash
# Check available features
cargo build --help | grep -A 20 "FEATURES:"

# Build with specific features
cargo build --release --features postgresql,sqlite

# Debug feature compilation
cargo build --release --features postgresql --verbose
```

#### System Dependencies

```bash
# Ubuntu/Debian: Install system dependencies
sudo apt-get update
sudo apt-get install build-essential libssl-dev pkg-config

# For PostgreSQL support
sudo apt-get install libpq-dev

# For MySQL support
sudo apt-get install libmysqlclient-dev

# macOS: Install dependencies
brew install openssl pkg-config
# For PostgreSQL: brew install postgresql
# For MySQL: brew install mysql
```

### Runtime Dependencies

**Symptoms**: Missing shared libraries, runtime errors

#### Check Dependencies

```bash
# Check binary dependencies
ldd target/release/dbsurveyor-collect  # Linux
otool -L target/release/dbsurveyor-collect  # macOS

# Test minimal functionality
dbsurveyor-collect --version
dbsurveyor-collect list
```

## Getting Help

### Collect Debug Information

```bash
# System information
uname -a
rustc --version
cargo --version

# DBSurveyor information
dbsurveyor-collect --version
dbsurveyor-collect list

# Feature compilation
cargo build --release --verbose 2>&1 | grep -i feature

# Runtime debug
export RUST_LOG=debug
dbsurveyor-collect test postgres://localhost/db 2> debug.log
```

### Create Minimal Reproduction

```bash
# Create minimal test case
sqlite3 minimal.db "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT);"
sqlite3 minimal.db "INSERT INTO test VALUES (1, 'test');"

# Test collection
dbsurveyor-collect sqlite://minimal.db

# Test documentation
dbsurveyor generate schema.dbsurveyor.json

# Clean up
rm minimal.db schema.dbsurveyor.json schema.md
```

### Report Issues

When reporting issues, include:

1. **System Information**: OS, Rust version, DBSurveyor version
2. **Command Used**: Exact command that failed (sanitize credentials)
3. **Error Output**: Complete error message and stack trace
4. **Debug Logs**: Output with `RUST_LOG=debug`
5. **Minimal Reproduction**: Smallest example that reproduces the issue

**Security Note**: Never include actual database credentials in issue reports. Use placeholder values like `user:password@localhost/db`.

### Community Resources

- **GitHub Issues**: [Report bugs and request features](https://github.com/EvilBit-Labs/dbsurveyor/issues)
- **Documentation**: [Complete user guide](https://evilbitlabs.io/dbsurveyor)
- **Security Issues**: Email [security@evilbitlabs.io](mailto:security@evilbitlabs.io)

### Professional Support

For enterprise users requiring professional support, contact [support@evilbitlabs.io](mailto:support@evilbitlabs.io) for:

- Priority issue resolution
- Custom feature development
- Integration consulting
- Security auditing and compliance
