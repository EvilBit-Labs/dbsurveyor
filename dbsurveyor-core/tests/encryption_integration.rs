//! Integration tests for AES-GCM encryption implementation
//!
//! This test verifies that the encryption implementation meets all requirements
//! from Task 1 of the database-schema-collection spec.

#[cfg(feature = "encryption")]
mod encryption_integration_tests {
    use dbsurveyor_core::security::encryption::{KdfParams, decrypt_data, encrypt_data};

    #[test]
    fn test_task_1_requirements_compliance() {
        // Test data representing database schema
        let schema_data = br#"
        {
            "format_version": "1.0",
            "database_info": {
                "name": "production_db",
                "type": "PostgreSQL",
                "version": "15.2"
            },
            "tables": [
                {
                    "name": "users",
                    "columns": [
                        {"name": "id", "type": "INTEGER", "primary_key": true},
                        {"name": "email", "type": "VARCHAR(255)", "nullable": false},
                        {"name": "password_hash", "type": "VARCHAR(255)", "nullable": false}
                    ]
                }
            ]
        }"#;

        let password = "secure_encryption_key_2024";

        // Requirement: AES-GCM encryption with random 96-bit nonces
        let encrypted1 = encrypt_data(schema_data, password).unwrap();
        let encrypted2 = encrypt_data(schema_data, password).unwrap();

        assert_eq!(encrypted1.algorithm, "AES-GCM-256");
        assert_eq!(encrypted1.nonce.len(), 12); // 96 bits = 12 bytes
        assert_eq!(encrypted1.auth_tag.len(), 16); // 128 bits = 16 bytes

        // Requirement: Random nonces (no reuse)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // Requirement: Argon2id with exact settings
        let kdf_params = &encrypted1.kdf_params;
        assert_eq!(kdf_params.salt.len(), 16); // 16-byte salt
        assert_eq!(kdf_params.memory_cost, 65536); // 64 MiB in KiB
        assert_eq!(kdf_params.time_cost, 3); // 3 iterations
        assert_eq!(kdf_params.parallelism, 4); // 4 threads
        assert_eq!(kdf_params.version, "1.3"); // Argon2id v1.3

        // Requirement: Proper nonce and tag validation
        let decrypted = decrypt_data(&encrypted1, password).unwrap();
        assert_eq!(schema_data, &decrypted[..]);

        // Requirement: Embedded KDF parameters
        assert!(!kdf_params.salt.is_empty());
        assert!(kdf_params.validate().is_ok());

        // Test serialization (for file storage)
        let json = serde_json::to_string(&encrypted1).unwrap();
        let deserialized: dbsurveyor_core::security::encryption::EncryptedData =
            serde_json::from_str(&json).unwrap();
        let deserialized_decrypted = decrypt_data(&deserialized, password).unwrap();
        assert_eq!(schema_data, &deserialized_decrypted[..]);
    }

    #[test]
    fn test_comprehensive_error_handling() {
        let data = b"test data";
        let password = "test_password";
        let encrypted = encrypt_data(data, password).unwrap();

        // Wrong password should fail
        assert!(decrypt_data(&encrypted, "wrong_password").is_err());

        // Tampered ciphertext should fail
        let mut tampered_ciphertext = encrypted.clone();
        tampered_ciphertext.ciphertext[0] ^= 1;
        assert!(decrypt_data(&tampered_ciphertext, password).is_err());

        // Tampered auth tag should fail
        let mut tampered_tag = encrypted.clone();
        tampered_tag.auth_tag[0] ^= 1;
        assert!(decrypt_data(&tampered_tag, password).is_err());

        // Invalid nonce length should fail
        let mut invalid_nonce = encrypted.clone();
        invalid_nonce.nonce = vec![0u8; 11]; // Should be 12
        assert!(decrypt_data(&invalid_nonce, password).is_err());

        // Invalid auth tag length should fail
        let mut invalid_tag = encrypted.clone();
        invalid_tag.auth_tag = vec![0u8; 15]; // Should be 16
        assert!(decrypt_data(&invalid_tag, password).is_err());

        // Unsupported algorithm should fail
        let mut unsupported_algo = encrypted.clone();
        unsupported_algo.algorithm = "AES-CBC-256".to_string();
        assert!(decrypt_data(&unsupported_algo, password).is_err());
    }

    #[test]
    fn test_nonce_uniqueness_across_multiple_encryptions() {
        let data = b"test data for nonce uniqueness";
        let password = "same_password";
        let mut nonces = std::collections::HashSet::new();

        // Allow CI to increase iterations via ENCRYPTION_TEST_ITERATIONS while
        // keeping a small, fast default for local development.
        const DEFAULT_NONCE_TEST_ITERATIONS: usize = 10;
        let iterations = std::env::var("ENCRYPTION_TEST_ITERATIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_NONCE_TEST_ITERATIONS);

        // Generate multiple encryptions and verify all nonces are unique
        for _ in 0..iterations {
            let encrypted = encrypt_data(data, password).unwrap();
            let nonce_clone = encrypted.nonce.clone();
            assert!(nonces.insert(nonce_clone), "Nonce collision detected!");

            // Verify decryption still works
            let decrypted = decrypt_data(&encrypted, password).unwrap();
            assert_eq!(data, &decrypted[..]);
        }

        // Additional verification: Check that nonces are actually random
        // by ensuring they're not sequential or predictable
        let encrypted1 = encrypt_data(data, password).unwrap();
        let encrypted2 = encrypt_data(data, password).unwrap();
        let encrypted3 = encrypt_data(data, password).unwrap();

        // Verify nonces are different
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted2.nonce, encrypted3.nonce);
        assert_ne!(encrypted1.nonce, encrypted3.nonce);
    }

    #[test]
    fn test_kdf_parameter_validation() {
        let mut params = KdfParams::new();

        // Valid parameters should pass
        assert!(params.validate().is_ok());

        // Test each validation rule
        params.salt = vec![0u8; 15]; // Too short
        assert!(params.validate().is_err());

        params.salt = vec![0u8; 16]; // Reset to valid
        params.memory_cost = 32768; // Too low (< 64 MiB)
        assert!(params.validate().is_err());

        params.memory_cost = 65536; // Reset to valid
        params.time_cost = 2; // Too low (< 3)
        assert!(params.validate().is_err());

        params.time_cost = 3; // Reset to valid
        params.parallelism = 0; // Too low (< 1)
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_large_data_encryption() {
        // Test with 1MB of data to ensure it works with large schemas
        let large_data = vec![0x42u8; 1024 * 1024];
        let password = "test_password";

        let encrypted = encrypt_data(&large_data, password).unwrap();
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        assert_eq!(large_data, decrypted);
    }

    #[test]
    fn test_empty_data_encryption() {
        let empty_data = b"";
        let password = "test_password";

        let encrypted = encrypt_data(empty_data, password).unwrap();
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        assert_eq!(empty_data, &decrypted[..]);
    }
}
