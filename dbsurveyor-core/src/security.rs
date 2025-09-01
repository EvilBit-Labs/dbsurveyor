//! Security utilities for credential protection and encryption.
//!
//! This module provides security-focused utilities including credential
//! sanitization, secure memory handling, and optional encryption capabilities.

use zeroize::{Zeroize, Zeroizing};

/// Secure credential container that automatically zeros memory on drop
#[derive(Debug, Clone, Zeroize)]
#[zeroize(drop)]
pub struct Credentials {
    pub username: Zeroizing<String>,
    pub password: Zeroizing<Option<String>>,
}

impl Credentials {
    /// Creates new credentials with automatic memory zeroing
    pub fn new(username: String, password: Option<String>) -> Self {
        Self {
            username: Zeroizing::new(username),
            password: Zeroizing::new(password),
        }
    }

    /// Gets the username (still protected by Zeroizing)
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Checks if password is present without exposing it
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }
}

/// Parses a database connection string and extracts credentials safely
///
/// # Security
/// - Credentials are immediately moved into secure containers
/// - Original connection string is not modified
/// - Password is never stored in plain String
///
/// # Arguments
/// * `connection_string` - Database connection URL
///
/// # Returns
/// Tuple of (sanitized_config, credentials) where credentials are secured
///
/// # Example
/// ```rust
/// use dbsurveyor_core::security::parse_connection_string;
///
/// let (config, creds) = parse_connection_string("postgres://user:pass@localhost/db")?;
/// assert_eq!(config.host, "localhost");
/// assert_eq!(creds.username(), "user");
/// assert!(creds.has_password());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_connection_string(
    connection_string: &str,
) -> crate::Result<(ConnectionInfo, Credentials)> {
    let url = url::Url::parse(connection_string).map_err(|e| {
        crate::error::DbSurveyorError::configuration(format!(
            "Invalid connection string format: {}",
            e
        ))
    })?;

    let host = url
        .host_str()
        .ok_or_else(|| {
            crate::error::DbSurveyorError::configuration("Missing host in connection string")
        })?
        .to_string();

    let port = url.port();
    let database = if url.path().len() > 1 {
        Some(url.path()[1..].to_string()) // Remove leading '/'
    } else {
        None
    };

    let username = if !url.username().is_empty() {
        url.username().to_string()
    } else {
        "".to_string()
    };

    let password = url.password().map(|p| p.to_string());

    let credentials = Credentials::new(username, password);

    let config = ConnectionInfo {
        scheme: url.scheme().to_string(),
        host,
        port,
        database,
        query_params: url.query_pairs().into_owned().collect(),
    };

    Ok((config, credentials))
}

/// Connection information with credentials removed
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub query_params: Vec<(String, String)>,
}

impl ConnectionInfo {
    /// Reconstructs a connection string without credentials
    pub fn to_safe_string(&self) -> String {
        let mut url = format!("{}://{}", self.scheme, self.host);

        if let Some(port) = self.port {
            url.push_str(&format!(":{}", port));
        }

        if let Some(database) = &self.database {
            url.push_str(&format!("/{}", database));
        }

        if !self.query_params.is_empty() {
            url.push('?');
            let params: Vec<String> = self
                .query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            url.push_str(&params.join("&"));
        }

        url
    }
}

#[cfg(feature = "encryption")]
pub mod encryption {
    //! AES-GCM encryption with Argon2id key derivation.
    //!
    //! This module provides secure encryption and decryption using AES-GCM-256
    //! with random 96-bit nonces and Argon2id key derivation function.
    //!
    //! # Security Guarantees
    //! - AES-GCM-256 authenticated encryption with random nonces
    //! - Argon2id key derivation with secure parameters
    //! - No nonce reuse (each encryption uses a fresh random nonce)
    //! - Authenticated headers prevent tampering
    //! - Memory-safe key handling with automatic zeroing

    use aes_gcm::{
        Aes256Gcm, Key, Nonce,
        aead::{Aead, AeadCore, KeyInit, OsRng, rand_core::RngCore},
    };
    use argon2::{
        Argon2, Params, Version,
        password_hash::{PasswordHasher, SaltString},
    };
    use serde::{Deserialize, Serialize};
    use zeroize::Zeroizing;

    /// Cryptographic constants for AES-GCM encryption
    const AES_GCM_NONCE_SIZE: usize = 12; // 96 bits
    const AES_GCM_TAG_SIZE: usize = 16; // 128 bits
    const AES_KEY_SIZE: usize = 32; // 256 bits
    const ARGON2_SALT_SIZE: usize = 16; // 128 bits minimum
    const ARGON2_MEMORY_COST: u32 = 65536; // 64 MiB in KiB
    const ARGON2_TIME_COST: u32 = 3; // 3 iterations
    const ARGON2_PARALLELISM: u32 = 4; // 4 threads

    /// Key derivation parameters for Argon2id
    ///
    /// Uses secure defaults as specified in requirements:
    /// - 16-byte salt
    /// - Version 1.3 (Argon2id)
    /// - Time cost: 3 iterations
    /// - Memory: 64 MiB (65536 KiB)
    /// - Parallelism: 4 threads
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KdfParams {
        /// Random salt (16 bytes as per requirements)
        pub salt: Vec<u8>,
        /// Memory cost in KiB (64 MiB = 65536 KiB)
        pub memory_cost: u32,
        /// Time cost (iterations)
        pub time_cost: u32,
        /// Parallelism factor
        pub parallelism: u32,
        /// Argon2 version (always 1.3 for Argon2id)
        pub version: String,
    }

    impl Default for KdfParams {
        fn default() -> Self {
            Self {
                salt: Vec::new(), // Will be filled with random data
                memory_cost: ARGON2_MEMORY_COST,
                time_cost: ARGON2_TIME_COST,
                parallelism: ARGON2_PARALLELISM,
                version: "1.3".to_string(),
            }
        }
    }

    impl KdfParams {
        /// Creates new KDF parameters with a random 16-byte salt
        pub fn new() -> Self {
            let mut salt = vec![0u8; ARGON2_SALT_SIZE];
            OsRng.fill_bytes(&mut salt);

            Self {
                salt,
                memory_cost: ARGON2_MEMORY_COST,
                time_cost: ARGON2_TIME_COST,
                parallelism: ARGON2_PARALLELISM,
                version: "1.3".to_string(),
            }
        }

        /// Validates that KDF parameters meet security requirements
        pub fn validate(&self) -> crate::Result<()> {
            if self.salt.len() < ARGON2_SALT_SIZE {
                return Err(crate::error::DbSurveyorError::configuration(format!(
                    "Salt must be at least {} bytes",
                    ARGON2_SALT_SIZE
                )));
            }
            if self.memory_cost < ARGON2_MEMORY_COST {
                return Err(crate::error::DbSurveyorError::configuration(format!(
                    "Memory cost must be at least {} KiB (64 MiB)",
                    ARGON2_MEMORY_COST
                )));
            }
            if self.time_cost < ARGON2_TIME_COST {
                return Err(crate::error::DbSurveyorError::configuration(format!(
                    "Time cost must be at least {} iterations",
                    ARGON2_TIME_COST
                )));
            }
            if self.parallelism < 1 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Parallelism must be at least 1",
                ));
            }
            Ok(())
        }
    }

    /// Encrypted data container with AES-GCM and embedded KDF parameters
    ///
    /// Contains all information needed for decryption including:
    /// - Algorithm identifier
    /// - Random 96-bit nonce
    /// - Encrypted ciphertext
    /// - Authentication tag (separate from ciphertext)
    /// - KDF parameters and salt for key derivation
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EncryptedData {
        /// Encryption algorithm (always "AES-GCM-256")
        pub algorithm: String,
        /// Random 96-bit (12-byte) nonce
        pub nonce: Vec<u8>,
        /// Encrypted payload
        pub ciphertext: Vec<u8>,
        /// Authentication tag (16 bytes)
        pub auth_tag: Vec<u8>,
        /// Key derivation parameters
        pub kdf_params: KdfParams,
    }

    /// Validates encrypted data structure before decryption
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted data to validate
    ///
    /// # Returns
    /// Ok(()) if valid, error with specific validation failure
    fn validate_encrypted_data(encrypted: &EncryptedData) -> crate::Result<()> {
        // Validate algorithm
        if encrypted.algorithm != "AES-GCM-256" {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Unsupported encryption algorithm: {}",
                encrypted.algorithm
            )));
        }

        // Validate nonce length
        if encrypted.nonce.len() != AES_GCM_NONCE_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Invalid nonce length: expected {}, got {}",
                AES_GCM_NONCE_SIZE,
                encrypted.nonce.len()
            )));
        }

        // Validate auth tag length
        if encrypted.auth_tag.len() != AES_GCM_TAG_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Invalid authentication tag length: expected {}, got {}",
                AES_GCM_TAG_SIZE,
                encrypted.auth_tag.len()
            )));
        }

        // Validate KDF parameters
        encrypted.kdf_params.validate()?;

        Ok(())
    }

    /// Derives an AES-256 key from a password using Argon2id
    ///
    /// # Security
    /// - Uses Argon2id with secure parameters
    /// - Key material is automatically zeroed on drop
    /// - Salt and parameters are embedded for verification
    ///
    /// # Arguments
    /// * `password` - Password for key derivation
    /// * `kdf_params` - KDF parameters including salt
    ///
    /// # Returns
    /// 32-byte AES-256 key in a zeroizing container
    fn derive_key(password: &str, kdf_params: &KdfParams) -> crate::Result<Zeroizing<[u8; 32]>> {
        // Validate parameters first
        kdf_params.validate()?;

        // Create Argon2 parameters
        let params = Params::new(
            kdf_params.memory_cost,
            kdf_params.time_cost,
            kdf_params.parallelism,
            Some(AES_KEY_SIZE), // Output length: 32 bytes for AES-256
        )
        .map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Invalid Argon2 parameters: {}",
                e
            ))
        })?;

        // Create Argon2id instance
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

        // Create salt string from bytes
        let salt_string = SaltString::encode_b64(&kdf_params.salt).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!("Invalid salt: {}", e))
        })?;

        // Derive key
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt_string)
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Key derivation failed: {}",
                    e
                ))
            })?;

        // Extract the hash bytes (32 bytes for AES-256)
        let hash_bytes = password_hash.hash.ok_or_else(|| {
            crate::error::DbSurveyorError::configuration("Key derivation produced no output")
        })?;

        if hash_bytes.as_bytes().len() != AES_KEY_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Key derivation produced incorrect key length: expected {}, got {}",
                AES_KEY_SIZE,
                hash_bytes.as_bytes().len()
            )));
        }

        // Copy to fixed-size array with automatic zeroing
        let mut key = Zeroizing::new([0u8; AES_KEY_SIZE]);
        key.copy_from_slice(hash_bytes.as_bytes());

        Ok(key)
    }

    /// Encrypts data using AES-GCM-256 with Argon2id key derivation
    ///
    /// # Security Guarantees
    /// - Uses AES-GCM-256 authenticated encryption
    /// - Random 96-bit nonce generated for each encryption
    /// - Argon2id key derivation with secure parameters
    /// - Authentication tag prevents tampering
    /// - Key material is automatically zeroed
    ///
    /// # Arguments
    /// * `data` - Data to encrypt
    /// * `password` - Password for key derivation
    ///
    /// # Returns
    /// Encrypted data container with all parameters needed for decryption
    ///
    /// # Example
    /// ```rust
    /// use dbsurveyor_core::security::encryption::encrypt_data;
    ///
    /// let data = b"sensitive database schema";
    /// let encrypted = encrypt_data(data, "strong_password")?;
    /// assert_eq!(encrypted.algorithm, "AES-GCM-256");
    /// assert_eq!(encrypted.nonce.len(), 12); // 96 bits
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encrypt_data(data: &[u8], password: &str) -> crate::Result<EncryptedData> {
        // Generate KDF parameters with random salt
        let kdf_params = KdfParams::new();

        // Derive encryption key
        let key = derive_key(password, &kdf_params)?;

        // Create AES-GCM cipher
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key));

        // Generate random 96-bit nonce
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        // Encrypt data
        let ciphertext = cipher.encrypt(&nonce, data).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!("Encryption failed: {}", e))
        })?;

        // Split ciphertext and auth tag
        // AES-GCM appends the 16-byte auth tag to the ciphertext
        if ciphertext.len() < AES_GCM_TAG_SIZE {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Encrypted data too short (minimum {} bytes for auth tag)",
                AES_GCM_TAG_SIZE
            )));
        }

        let (payload, auth_tag) = ciphertext.split_at(ciphertext.len() - AES_GCM_TAG_SIZE);

        Ok(EncryptedData {
            algorithm: "AES-GCM-256".to_string(),
            nonce: nonce.to_vec(),
            ciphertext: payload.to_vec(),
            auth_tag: auth_tag.to_vec(),
            kdf_params,
        })
    }

    /// Decrypts data using AES-GCM-256 with embedded parameters
    ///
    /// # Security Guarantees
    /// - Validates all parameters before decryption
    /// - Verifies authentication tag to prevent tampering
    /// - Uses embedded KDF parameters for key derivation
    /// - Key material is automatically zeroed
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted data container
    /// * `password` - Password for key derivation
    ///
    /// # Returns
    /// Decrypted plaintext data
    ///
    /// # Errors
    /// Returns error if:
    /// - Authentication fails (data was tampered with)
    /// - Password is incorrect
    /// - Parameters are invalid
    ///
    /// # Example
    /// ```rust
    /// use dbsurveyor_core::security::encryption::{encrypt_data, decrypt_data};
    ///
    /// let original = b"sensitive database schema";
    /// let encrypted = encrypt_data(original, "strong_password")?;
    /// let decrypted = decrypt_data(&encrypted, "strong_password")?;
    /// assert_eq!(original, &decrypted[..]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn decrypt_data(encrypted: &EncryptedData, password: &str) -> crate::Result<Vec<u8>> {
        // Validate encrypted data structure
        validate_encrypted_data(encrypted)?;

        // Derive decryption key using embedded parameters
        let key = derive_key(password, &encrypted.kdf_params)?;

        // Create AES-GCM cipher
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key));

        // Reconstruct nonce
        let nonce = Nonce::from_slice(&encrypted.nonce);

        // Reconstruct full ciphertext (payload + auth tag)
        let mut full_ciphertext = encrypted.ciphertext.clone();
        full_ciphertext.extend_from_slice(&encrypted.auth_tag);

        // Decrypt and verify
        let plaintext = cipher
            .decrypt(nonce, full_ciphertext.as_slice())
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Decryption failed (wrong password or corrupted data): {}",
                    e
                ))
            })?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_zeroization() {
        let creds = Credentials::new("user".to_string(), Some("password".to_string()));
        assert_eq!(creds.username(), "user");
        assert!(creds.has_password());
        // Credentials will be automatically zeroized on drop
    }

    #[test]
    fn test_parse_connection_string() {
        let (config, creds) =
            parse_connection_string("postgres://user:pass@localhost:5432/testdb").unwrap();

        assert_eq!(config.scheme, "postgres");
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(creds.username(), "user");
        assert!(creds.has_password());
    }

    #[test]
    fn test_connection_info_safe_string() {
        let config = ConnectionInfo {
            scheme: "postgres".to_string(),
            host: "localhost".to_string(),
            port: Some(5432),
            database: Some("testdb".to_string()),
            query_params: vec![("sslmode".to_string(), "require".to_string())],
        };

        let safe_string = config.to_safe_string();
        assert_eq!(
            safe_string,
            "postgres://localhost:5432/testdb?sslmode=require"
        );
        assert!(!safe_string.contains("password"));
        assert!(!safe_string.contains("user"));
    }

    #[test]
    fn test_parse_connection_string_no_credentials() {
        let (config, creds) = parse_connection_string("postgres://localhost/testdb").unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(creds.username(), "");
        assert!(!creds.has_password());
    }

    #[cfg(feature = "encryption")]
    mod encryption_tests {
        use super::super::encryption::*;

        #[test]
        fn test_kdf_params_new() {
            let params = KdfParams::new();

            // Verify salt is 16 bytes
            assert_eq!(params.salt.len(), 16);

            // Verify secure defaults
            assert_eq!(params.memory_cost, 65536); // 64 MiB
            assert_eq!(params.time_cost, 3);
            assert_eq!(params.parallelism, 4);
            assert_eq!(params.version, "1.3");

            // Verify validation passes
            assert!(params.validate().is_ok());
        }

        #[test]
        fn test_kdf_params_validation() {
            let mut params = KdfParams::new();

            // Test salt length validation
            params.salt = vec![0u8; 15]; // Too short
            assert!(params.validate().is_err());

            params.salt = vec![0u8; 16]; // Minimum required
            assert!(params.validate().is_ok());

            // Test memory cost validation
            params.memory_cost = 32768; // Too low
            assert!(params.validate().is_err());

            params.memory_cost = 65536; // Minimum required
            assert!(params.validate().is_ok());

            // Test time cost validation
            params.time_cost = 2; // Too low
            assert!(params.validate().is_err());

            params.time_cost = 3; // Minimum required
            assert!(params.validate().is_ok());

            // Test parallelism validation
            params.parallelism = 0; // Too low
            assert!(params.validate().is_err());

            params.parallelism = 1; // Minimum required
            assert!(params.validate().is_ok());
        }

        #[test]
        fn test_encrypt_decrypt_roundtrip() {
            let original_data = b"This is sensitive database schema data that needs encryption";
            let password = "test_password_123";

            // Encrypt the data
            let encrypted = encrypt_data(original_data, password).unwrap();

            // Verify encrypted data structure
            assert_eq!(encrypted.algorithm, "AES-GCM-256");
            assert_eq!(encrypted.nonce.len(), 12); // 96 bits
            assert_eq!(encrypted.auth_tag.len(), 16); // 128 bits
            assert!(!encrypted.ciphertext.is_empty());
            assert_eq!(encrypted.kdf_params.salt.len(), 16);

            // Decrypt the data
            let decrypted = decrypt_data(&encrypted, password).unwrap();

            // Verify roundtrip
            assert_eq!(original_data, &decrypted[..]);
        }

        #[test]
        fn test_nonce_uniqueness() {
            let data = b"test data for nonce uniqueness";
            let password = "same_password";

            // Encrypt the same data multiple times
            let encrypted1 = encrypt_data(data, password).unwrap();
            let encrypted2 = encrypt_data(data, password).unwrap();
            let encrypted3 = encrypt_data(data, password).unwrap();

            // Nonces should be different (random)
            assert_ne!(encrypted1.nonce, encrypted2.nonce);
            assert_ne!(encrypted2.nonce, encrypted3.nonce);
            assert_ne!(encrypted1.nonce, encrypted3.nonce);

            // Ciphertext should be different due to different nonces
            assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
            assert_ne!(encrypted2.ciphertext, encrypted3.ciphertext);

            // But all should decrypt to the same plaintext
            let decrypted1 = decrypt_data(&encrypted1, password).unwrap();
            let decrypted2 = decrypt_data(&encrypted2, password).unwrap();
            let decrypted3 = decrypt_data(&encrypted3, password).unwrap();

            assert_eq!(data, &decrypted1[..]);
            assert_eq!(data, &decrypted2[..]);
            assert_eq!(data, &decrypted3[..]);
        }

        #[test]
        fn test_wrong_password_fails() {
            let data = b"secret data";
            let correct_password = "correct_password";
            let wrong_password = "wrong_password";

            let encrypted = encrypt_data(data, correct_password).unwrap();

            // Decryption with wrong password should fail
            let result = decrypt_data(&encrypted, wrong_password);
            assert!(result.is_err());

            // Verify error message indicates authentication failure
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Decryption failed"));
        }

        #[test]
        fn test_tampered_data_fails() {
            let data = b"secret data";
            let password = "test_password";

            let mut encrypted = encrypt_data(data, password).unwrap();

            // Tamper with ciphertext
            if !encrypted.ciphertext.is_empty() {
                encrypted.ciphertext[0] ^= 1; // Flip one bit
            }

            // Decryption should fail due to authentication failure
            let result = decrypt_data(&encrypted, password);
            assert!(result.is_err());
        }

        #[test]
        fn test_tampered_auth_tag_fails() {
            let data = b"secret data";
            let password = "test_password";

            let mut encrypted = encrypt_data(data, password).unwrap();

            // Tamper with auth tag
            encrypted.auth_tag[0] ^= 1; // Flip one bit

            // Decryption should fail due to authentication failure
            let result = decrypt_data(&encrypted, password);
            assert!(result.is_err());
        }

        #[test]
        fn test_invalid_nonce_length_fails() {
            let data = b"secret data";
            let password = "test_password";

            let mut encrypted = encrypt_data(data, password).unwrap();

            // Invalid nonce length
            encrypted.nonce = vec![0u8; 11]; // Should be 12 bytes

            let result = decrypt_data(&encrypted, password);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Invalid nonce length")
            );
        }

        #[test]
        fn test_invalid_auth_tag_length_fails() {
            let data = b"secret data";
            let password = "test_password";

            let mut encrypted = encrypt_data(data, password).unwrap();

            // Invalid auth tag length
            encrypted.auth_tag = vec![0u8; 15]; // Should be 16 bytes

            let result = decrypt_data(&encrypted, password);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Invalid authentication tag length")
            );
        }

        #[test]
        fn test_unsupported_algorithm_fails() {
            let data = b"secret data";
            let password = "test_password";

            let mut encrypted = encrypt_data(data, password).unwrap();

            // Change algorithm to unsupported one
            encrypted.algorithm = "AES-CBC-256".to_string();

            let result = decrypt_data(&encrypted, password);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Unsupported encryption algorithm")
            );
        }

        #[test]
        fn test_empty_data_encryption() {
            let data = b"";
            let password = "test_password";

            let encrypted = encrypt_data(data, password).unwrap();
            let decrypted = decrypt_data(&encrypted, password).unwrap();

            assert_eq!(data, &decrypted[..]);
        }

        #[test]
        fn test_large_data_encryption() {
            // Test with larger data (1MB)
            let data = vec![0x42u8; 1024 * 1024];
            let password = "test_password";

            let encrypted = encrypt_data(&data, password).unwrap();
            let decrypted = decrypt_data(&encrypted, password).unwrap();

            assert_eq!(data, decrypted);
        }

        #[test]
        fn test_kdf_parameters_embedded() {
            let data = b"test data";
            let password = "test_password";

            let encrypted = encrypt_data(data, password).unwrap();

            // Verify KDF parameters are properly embedded
            assert_eq!(encrypted.kdf_params.salt.len(), 16);
            assert_eq!(encrypted.kdf_params.memory_cost, 65536);
            assert_eq!(encrypted.kdf_params.time_cost, 3);
            assert_eq!(encrypted.kdf_params.parallelism, 4);
            assert_eq!(encrypted.kdf_params.version, "1.3");

            // Decryption should work using embedded parameters
            let decrypted = decrypt_data(&encrypted, password).unwrap();
            assert_eq!(data, &decrypted[..]);
        }

        #[test]
        fn test_serialization_roundtrip() {
            let data = b"test data for serialization";
            let password = "test_password";

            let encrypted = encrypt_data(data, password).unwrap();

            // Serialize to JSON
            let json = serde_json::to_string(&encrypted).unwrap();

            // Deserialize from JSON
            let deserialized: EncryptedData = serde_json::from_str(&json).unwrap();

            // Decrypt deserialized data
            let decrypted = decrypt_data(&deserialized, password).unwrap();
            assert_eq!(data, &decrypted[..]);
        }
    }
}
