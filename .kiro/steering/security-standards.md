---
inclusion: always
---

# Security Standards for DBSurveyor

## Core Security Philosophy

DBSurveyor is built with security-first principles that are non-negotiable:

- **Offline-Only Operation**: No network calls except to target databases
- **No Telemetry**: Zero data collection or external reporting
- **Credential Protection**: Database credentials never stored, logged, or output
- **Airgap Compatibility**: Full functionality in air-gapped environments

## Critical Security Guarantees

### 1. Offline-Only Operation

- No external network dependencies after database connection
- All processing happens locally
- No internet connectivity required for documentation generation

### 2. Credential Protection

```rust
// ✅ Correct: Secure error handling without credential exposure
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed to database")]
    ConnectionFailed,

    #[error("Query execution failed")]
    QueryFailed(#[from] sqlx::Error),
}

// ❌ Incorrect: Exposes credentials in error messages
#[error("Failed to connect to {url}")]
ConnectionFailedWithUrl { url: String },
```

### 3. Encryption Standards

Use AES-GCM with random nonce for all encrypted data:

```rust
pub struct EncryptedData {
    pub algorithm: String,             // "AES-GCM-256"
    pub nonce: Vec<u8>,                // 96-bit random nonce
    pub ciphertext: Vec<u8>,           // Encrypted data
    pub tag: Vec<u8>,                  // Authentication tag
    pub kdf_params: Option<KdfParams>, // Key derivation parameters
}
```

### 4. Database Security

- All database operations are strictly read-only
- Use connection pooling with appropriate limits
- Implement query timeouts (30 seconds default)
- Use parameterized queries only - NO string concatenation
- Validate certificates when using TLS/SSL

## Security Testing Requirements

### Credential Protection Tests

```rust
#[tokio::test]
async fn test_no_credentials_in_schema_output() {
    let database_url = "postgres://testuser:secretpass@localhost/testdb";
    let schema = collect_schema(database_url).await?;
    let json_output = serde_json::to_string(&schema)?;

    // Verify sensitive data is not present
    assert!(!json_output.contains("secretpass"));
    assert!(!json_output.contains("testuser:secretpass"));
    assert!(!json_output.contains("password"));
}
```

### Encryption Tests

```rust
#[tokio::test]
async fn test_encryption_randomness() {
    let data = b"test schema data";
    let encrypted1 = encrypt_data(data)?;
    let encrypted2 = encrypt_data(data)?;

    // Same data should produce different ciphertext (random nonce)
    assert_ne!(encrypted1, encrypted2);

    // Both should decrypt to same plaintext
    assert_eq!(decrypt_data(&encrypted1)?, data);
    assert_eq!(decrypt_data(&encrypted2)?, data);
}
```

### Offline Operation Tests

Verify that all functionality works without internet connectivity:

```rust
#[tokio::test]
async fn test_airgap_compatibility() {
    // Simulate airgap environment
    let schema_data = include_bytes!("fixtures/sample_schema.json");
    let schema: DatabaseSchema = serde_json::from_slice(schema_data).unwrap();

    // All processing should work offline
    let documentation = generate_documentation(&schema, OutputFormat::Markdown).await?;
    assert!(!documentation.is_empty());
}
```

## Security Anti-Patterns to Avoid

### Never Do These

```rust
// ❌ Exposing credentials in logs
log::info!("Connecting to {}", database_url);

// ❌ String concatenation for SQL
let query = format!("SELECT * FROM {} WHERE id = {}", table, id);

// ❌ Storing credentials in structs
pub struct Config {
    pub password: String, // Never store credentials
}

// ❌ Including credentials in error messages
#[error("Failed to connect to {url}")]
ConnectionError { url: String },
```

### Always Do These

```rust
// ✅ Log without sensitive data
log::info!("Establishing database connection");

// ✅ Parameterized queries
let query = "SELECT * FROM users WHERE id = $1";
let result = sqlx::query(query).bind(id).fetch_all(&pool).await?;

// ✅ Separate credentials from config
pub struct Config {
    pub host: String,
    pub port: u16,
    // Credentials passed separately
}

// ✅ Sanitized error messages
#[error("Connection failed to database")]
ConnectionFailed,
```

## File Security

### Output File Protection

- Generated documentation excludes all credential information
- Encrypted outputs use AES-GCM with random nonces
- File permissions should be restrictive (600 for sensitive files)
- Implement secure cleanup of temporary files

### Configuration Security

- Never store credentials in configuration files
- Use environment variables or secure credential stores
- Sanitize all file paths in logs and error messages
- Validate all input parameters

## Security Validation Commands

```bash
# Run complete security validation
just security-full

# Test encryption capabilities
just test-encryption

# Verify offline operation
just test-offline

# Check for credential leakage
just test-credential-security

# Security audit
just audit
```

## Compliance Requirements

- All code must pass security linting
- No `unsafe` code allowed (workspace-level denial)
- Comprehensive security test coverage
- Regular dependency vulnerability scanning
- SBOM (Software Bill of Materials) generation
