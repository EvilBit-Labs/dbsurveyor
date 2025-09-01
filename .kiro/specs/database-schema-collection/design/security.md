# Security Architecture

## Core Security Philosophy

DBSurveyor is built with security-first principles that are non-negotiable:

- **Offline-Only Operation**: No network calls except to target databases
- **No Telemetry**: Zero data collection or external reporting  
- **Credential Protection**: Database credentials never stored, logged, or output
- **Airgap Compatibility**: Full functionality in air-gapped environments

## Security Guarantees

### 1. Offline-Only Operation

**Guarantee**: The system operates completely offline after initial installation, with no external network dependencies beyond the target database connection.

**Implementation**:

- No external API calls or web requests
- All processing happens locally
- Documentation generation requires zero network connectivity
- Airgap-compatible operation validated through testing

### 2. Credential Protection

**Guarantee**: Database credentials are never stored, logged, or included in any output files.

**Implementation**:

```rust
use zeroize::{Zeroize, Zeroizing};

#[derive(Zeroize)]
#[zeroize(drop)]
struct Credentials {
    username: Zeroizing<String>,
    password: Zeroizing<Option<String>>,
}

// Credentials are immediately consumed and zeroed
pub fn parse_connection_string(url: &str) -> Result<(ConnectionParams, Credentials), SecurityError> {
    let parsed_url = url::Url::parse(url)?;
    
    let credentials = Credentials {
        username: Zeroizing::new(parsed_url.username().to_string()),
        password: Zeroizing::new(parsed_url.password().map(|p| p.to_string())),
    };
    
    let params = ConnectionParams {
        host: parsed_url.host_str().unwrap_or("localhost").to_string(),
        port: parsed_url.port().unwrap_or(5432),
        database: parsed_url.path().trim_start_matches('/').to_string(),
    };
    
    Ok((params, credentials))
    // Credentials automatically zeroed when dropped
}

// Sanitized display for logging
impl ConnectionParams {
    pub fn sanitized_display(&self) -> String {
        format!("{}:{}/{}", self.host, self.port, self.database)
        // No credentials included
    }
}
```

### 3. Encryption Standards

**Guarantee**: All encrypted data uses AES-GCM-256 with random nonces and Argon2id key derivation.

**Implementation**:

```rust
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, NewAead}};
use argon2::{Argon2, Version, Variant, Config};
use rand::{RngCore, OsRng};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    pub algorithm: String,     // "AES-GCM-256"
    pub nonce: Vec<u8>,        // 96-bit random nonce
    pub ciphertext: Vec<u8>,   // Encrypted payload
    pub auth_tag: Vec<u8>,     // Authentication tag
    pub kdf_params: KdfParams, // Key derivation parameters
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,     // "Argon2id"
    pub salt: Vec<u8>,         // 32-byte random salt
    pub memory_cost: u32,      // 64 MiB
    pub time_cost: u32,        // 3 iterations
    pub parallelism: u32,      // 4 threads
    pub version: String,       // "1.3"
}

pub async fn encrypt_schema_data(
    plaintext: &[u8],
    password: &str,
) -> Result<EncryptedData, EncryptionError> {
    // Generate random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    // Derive key using Argon2id
    let config = Config {
        variant: Variant::Argon2id,
        version: Version::Version13,
        mem_cost: 65536,      // 64 MiB
        time_cost: 3,         // 3 iterations
        lanes: 4,             // 4 parallel threads
        secret: &[],
        ad: &[],
        hash_length: 32,      // 256-bit key
    };

    let key_bytes = argon2::hash_raw(password.as_bytes(), &salt, &config)?;
    
    // Encrypt with AES-GCM
    let key = Key::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    Ok(EncryptedData {
        algorithm: "AES-GCM-256".to_string(),
        nonce: nonce_bytes.to_vec(),
        ciphertext,
        auth_tag: vec![], // Included in ciphertext for AES-GCM
        kdf_params: KdfParams {
            algorithm: "Argon2id".to_string(),
            salt: salt.to_vec(),
            memory_cost: 65536,
            time_cost: 3,
            parallelism: 4,
            version: "1.3".to_string(),
        },
    })
}
```

### 4. Database Security

**Guarantee**: All database operations are strictly read-only with proper access controls.

**Implementation**:

- Connection strings validated and sanitized
- Query timeouts prevent resource exhaustion (30s default)
- Parameterized queries only - no string concatenation
- Connection pooling with appropriate limits
- TLS/SSL certificate validation when available

```rust
// PostgreSQL security configuration
impl PostgresAdapter {
    pub async fn create_secure_pool(connection_string: &str) -> Result<PgPool, AdapterError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)                    // Limit concurrent connections
            .min_connections(2)                     // Maintain minimum pool
            .acquire_timeout(Duration::from_secs(30)) // Connection timeout
            .idle_timeout(Duration::from_secs(600))   // 10 minute idle timeout
            .max_lifetime(Duration::from_secs(3600))  // 1 hour max lifetime
            .test_before_acquire(true)              // Validate connections
            .connect(connection_string)
            .await?;

        // Set read-only mode and statement timeout
        sqlx::query("SET default_transaction_read_only = true")
            .execute(&pool)
            .await?;
        
        sqlx::query("SET statement_timeout = '30s'")
            .execute(&pool)
            .await?;

        Ok(pool)
    }
}
```

## Security Testing Requirements

### Credential Protection Tests

```rust
#[tokio::test]
async fn test_no_credentials_in_schema_output() -> Result<(), Box<dyn std::error::Error>> {
    let config = DatabaseConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "testdb".to_string(),
    };

    let password = Zeroizing::new("secretpass".to_string());
    let schema = collect_schema_with_config(&config, &password).await?;
    let json_output = serde_json::to_string(&schema)?;

    // Verify sensitive data is not present
    assert!(!json_output.contains("secretpass"));
    assert!(!json_output.contains("testuser:secretpass"));
    assert!(!json_output.contains("password"));

    Ok(())
}

#[tokio::test]
async fn test_connection_string_sanitization() {
    let connection_string = "postgres://user:secret@localhost/db";
    let error = DatabaseAdapter::connect(connection_string).await.unwrap_err();
    let error_message = error.to_string();

    // Verify credentials are not in error messages
    assert!(!error_message.contains("secret"));
    assert!(!error_message.contains("user:secret"));
}
```

### Encryption Tests

```rust
#[tokio::test]
async fn test_encryption_randomness() {
    let data = b"test schema data";
    let password = "test_password";

    let encrypted1 = encrypt_schema_data(data, password).await?;
    let encrypted2 = encrypt_schema_data(data, password).await?;

    // Same data should produce different ciphertext (random nonce)
    assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    assert_ne!(encrypted1.nonce, encrypted2.nonce);

    // Both should decrypt to same plaintext
    assert_eq!(decrypt_schema_data(&encrypted1, password).await?, data);
    assert_eq!(decrypt_schema_data(&encrypted2, password).await?, data);
}

#[tokio::test]
async fn test_key_derivation_parameters() {
    let encrypted = encrypt_schema_data(b"test", "password").await?;
    
    // Verify Argon2id parameters
    assert_eq!(encrypted.kdf_params.algorithm, "Argon2id");
    assert_eq!(encrypted.kdf_params.memory_cost, 65536); // 64 MiB
    assert_eq!(encrypted.kdf_params.time_cost, 3);
    assert_eq!(encrypted.kdf_params.parallelism, 4);
    assert_eq!(encrypted.kdf_params.version, "1.3");
    assert_eq!(encrypted.kdf_params.salt.len(), 32);
}
```

### Offline Operation Tests

```rust
#[tokio::test]
async fn test_airgap_compatibility() {
    // Simulate airgap environment by blocking network
    let _network_guard = MockNetworkGuard::block_all_except_localhost();

    let schema_data = include_bytes!("fixtures/sample_schema.json");
    let schema: DatabaseSchema = serde_json::from_slice(schema_data)?;

    // All postprocessor functionality should work offline
    let markdown_report = generate_markdown_report(&schema).await?;
    assert!(!markdown_report.is_empty());

    let html_report = generate_html_report(&schema).await?;
    assert!(!html_report.is_empty());

    let sql_ddl = generate_sql_ddl(&schema, SqlDialect::PostgreSQL).await?;
    assert!(!sql_ddl.is_empty());
}
```

## Security Anti-Patterns (Never Do)

### Credential Exposure

```rust
// ❌ NEVER: Log connection strings
log::info!("Connecting to {}", database_url);

// ❌ NEVER: Include credentials in error messages  
#[error("Failed to connect to {url}")]
ConnectionError { url: String },

// ❌ NEVER: Store credentials in structs
pub struct Config {
    pub password: String, // FORBIDDEN
}
```

### Security Violations

```rust
// ❌ NEVER: Use unwrap in production
let result = operation().unwrap();

// ❌ NEVER: SQL injection risk
let query = format!("SELECT * FROM {}", table);

// ❌ NEVER: Ignore security lints
#[allow(clippy::all)] // FORBIDDEN
```

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

## Compliance and Standards

- **No `unsafe` code**: Workspace-level denial prevents unsafe operations
- **Comprehensive security test coverage**: All security-critical paths tested
- **Regular dependency vulnerability scanning**: Automated with cargo-audit
- **SBOM generation**: Software Bill of Materials for supply chain security
- **Memory safety**: Rust's ownership system prevents common security vulnerabilities

This security architecture ensures that DBSurveyor meets the highest standards for security-conscious database analysis while maintaining usability and performance.
