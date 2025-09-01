# Security Features

DBSurveyor is built with security-first principles that are non-negotiable. This document details the comprehensive security features and guarantees.

## Core Security Guarantees

### 1. Offline-Only Operation

- **NO NETWORK CALLS**: Operates completely offline after initial installation
- **NO TELEMETRY**: Zero data collection, usage tracking, or external reporting
- **NO AUTO-UPDATES**: Manual control over all software updates
- **AIRGAP COMPATIBLE**: Full functionality in air-gapped environments

### 2. Credential Protection

Database credentials are never stored, logged, or included in any output files.

#### Implementation Details

- **Immediate Sanitization**: Connection strings sanitized in all error messages and logs
- **Secure Memory Handling**: Credentials automatically zeroed using the `zeroize` crate
- **Separate Handling**: Credentials parsed separately from connection configuration
- **No Serialization**: Credential structures are never serialized to disk

#### Example of Credential Sanitization

```rust
// Input: "postgres://user:secret123@localhost:5432/mydb"
// Logged: "postgres://user:****@localhost:5432/mydb"
// Error messages never contain actual passwords
```

### 3. AES-GCM Encryption

Industry-standard authenticated encryption for sensitive schema outputs.

#### Encryption Specifications

- **Algorithm**: AES-GCM-256 (256-bit key, 96-bit nonce, 128-bit authentication tag)
- **Nonce Generation**: Cryptographically secure random nonces (never reused)
- **Key Derivation**: Argon2id with secure parameters
- **Authentication**: Separate authentication tags prevent tampering

#### Key Derivation Parameters

| Parameter | Value | Purpose |
|-----------|-------|---------|
| Algorithm | Argon2id v1.3 | Memory-hard key derivation |
| Salt Size | 16 bytes (128 bits) | Random salt per encryption |
| Memory Cost | 64 MiB (65536 KiB) | Memory hardness |
| Time Cost | 3 iterations | Computational hardness |
| Parallelism | 4 threads | Parallel processing |
| Output Length | 32 bytes (256 bits) | AES-256 key size |

#### Encryption Process

```mermaid
graph TD
    A[Password] --> B[Argon2id KDF]
    C[Random Salt] --> B
    B --> D[256-bit Key]
    E[Schema Data] --> F[AES-GCM Encrypt]
    D --> F
    G[Random Nonce] --> F
    F --> H[Ciphertext]
    F --> I[Auth Tag]
    
    J[Encrypted File] --> K[Algorithm: AES-GCM-256]
    J --> L[Nonce: 96 bits]
    J --> M[Ciphertext]
    J --> N[Auth Tag: 128 bits]
    J --> O[KDF Params + Salt]
```

### 4. Database Security

All database operations follow strict security principles.

#### Read-Only Operations

- **SELECT/DESCRIBE Only**: No INSERT, UPDATE, DELETE, or DDL operations
- **Connection Flags**: Read-only mode enforced at connection level where supported
- **User Permissions**: Recommend using read-only database users

#### Connection Security

- **Timeouts**: All connections and queries have configurable timeouts (default: 30s)
- **Connection Pooling**: Limited connection pools prevent resource exhaustion
- **TLS/SSL**: Certificate validation when using encrypted connections
- **Parameterized Queries**: All queries use parameterized statements (no SQL injection risk)

#### Example Connection Configuration

```rust
ConnectionConfig {
    connect_timeout: Duration::from_secs(30),
    query_timeout: Duration::from_secs(30),
    max_connections: 10,
    read_only: true,  // Enforced
    // Credentials handled separately
}
```

## Security Testing

DBSurveyor includes comprehensive security tests to verify all guarantees.

### Credential Protection Tests

```rust
#[test]
fn test_no_credentials_in_output() {
    let schema = collect_schema("postgres://user:secret@localhost/db").await?;
    let json = serde_json::to_string(&schema)?;
    
    // Verify no sensitive data is present
    assert!(!json.contains("secret"));
    assert!(!json.contains("user:secret"));
    assert!(!json.contains("password"));
}
```

### Encryption Tests

```rust
#[test]
fn test_nonce_uniqueness() {
    let data = b"test schema data";
    let encrypted1 = encrypt_data(data, "password")?;
    let encrypted2 = encrypt_data(data, "password")?;
    
    // Same data should produce different ciphertext (random nonce)
    assert_ne!(encrypted1.nonce, encrypted2.nonce);
    assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
}
```

### Offline Operation Tests

```rust
#[test]
fn test_airgap_compatibility() {
    // Simulate airgap environment
    let schema = load_schema("fixtures/sample_schema.json")?;
    
    // All processing should work offline
    let documentation = generate_documentation(&schema, OutputFormat::Markdown)?;
    assert!(!documentation.is_empty());
}
```

## Security Best Practices

### For Users

1. **Use Read-Only Database Users**

   ```sql
   -- PostgreSQL
   CREATE USER dbsurveyor_readonly;
   GRANT CONNECT ON DATABASE mydb TO dbsurveyor_readonly;
   GRANT USAGE ON SCHEMA public TO dbsurveyor_readonly;
   GRANT SELECT ON ALL TABLES IN SCHEMA public TO dbsurveyor_readonly;
   
   -- MySQL
   CREATE USER 'dbsurveyor_readonly'@'%' IDENTIFIED BY 'password';
   GRANT SELECT ON mydb.* TO 'dbsurveyor_readonly'@'%';
   ```

2. **Use Strong Encryption Passwords**
   - Minimum 12 characters
   - Mix of uppercase, lowercase, numbers, symbols
   - Consider using password managers

3. **Secure File Handling**

   ```bash
   # Set restrictive permissions on encrypted files
   chmod 600 schema.enc
   
   # Use secure deletion when done
   shred -vfz -n 3 schema.enc
   ```

4. **Network Security**
   - Use TLS/SSL connections when possible
   - Avoid credentials in command history
   - Use environment variables for connection strings

### For Developers

1. **Never Log Credentials**

   ```rust
   // ✅ Correct
   log::info!("Connecting to database");
   
   // ❌ Never do this
   log::info!("Connecting to {}", database_url);
   ```

2. **Sanitize Error Messages**

   ```rust
   // ✅ Correct
   #[error("Connection failed to database")]
   ConnectionFailed,
   
   // ❌ Never do this
   #[error("Failed to connect to {url}")]
   ConnectionError { url: String },
   ```

3. **Use Secure Memory Handling**

   ```rust
   use zeroize::{Zeroize, Zeroizing};
   
   #[derive(Zeroize)]
   #[zeroize(drop)]
   struct Credentials {
       username: Zeroizing<String>,
       password: Zeroizing<Option<String>>,
   }
   ```

## Security Validation Commands

Run these commands to verify security guarantees:

```bash
# Complete security validation
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

## Threat Model

### In Scope

- **Credential Exposure**: Preventing database credentials from appearing in logs, outputs, or error messages
- **Data at Rest**: Protecting collected schema files with strong encryption
- **Network Isolation**: Ensuring offline operation after initial setup
- **Memory Safety**: Secure handling of sensitive data in memory

### Out of Scope

- **Network Transport Security**: TLS/SSL configuration is the user's responsibility
- **Database Server Security**: DBSurveyor assumes the database server is trusted
- **Host System Security**: Operating system and file system security is external
- **Social Engineering**: User education and operational security practices

## Compliance Considerations

DBSurveyor's security features support various compliance requirements:

- **SOC 2**: Offline operation and encryption support security controls
- **GDPR**: Data minimization and encryption for personal data protection
- **HIPAA**: Encryption and access controls for healthcare data
- **PCI DSS**: Secure handling of systems that may contain payment data

## Security Advisories

Current known security advisories and their status:

### RUSTSEC-2023-0071 (RSA crate - Marvin Attack)

- **Severity**: Medium
- **Impact**: Timing side-channel vulnerability in RSA implementation
- **Affected**: MySQL connections only (not enabled by default)
- **Mitigation**: MySQL support disabled by default; PostgreSQL and SQLite recommended
- **Status**: No fix available upstream; tracked for future SQLx updates
- **Review Date**: 2025-09-01

## Reporting Security Issues

To report security vulnerabilities:

1. **DO NOT** create public GitHub issues for security vulnerabilities
2. Email security issues to: [security@evilbitlabs.io](mailto:security@evilbitlabs.io)
3. Include detailed reproduction steps and impact assessment
4. Allow reasonable time for response and remediation

We follow responsible disclosure practices and will acknowledge security reports within 48 hours.
