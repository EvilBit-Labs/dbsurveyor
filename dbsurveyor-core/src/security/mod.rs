//! Security utilities for credential protection and encryption.
//!
//! This module provides security-focused utilities including:
//! - Credential sanitization and secure memory handling
//! - Connection string parsing with automatic credential extraction
//! - Optional AES-GCM encryption (feature-gated)
//!
//! # Security Guarantees
//! - Credentials are stored in `Zeroizing` containers for automatic memory clearing
//! - Connection strings are parsed to extract credentials safely
//! - All sensitive data is redacted from logs and error messages
//!
//! # Module Structure
//! - `credentials`: Secure credential container with automatic memory zeroing
//! - `connection`: Connection string parsing and info extraction
//! - `encryption`: AES-GCM encryption with Argon2id key derivation (feature-gated)

mod connection;
mod credentials;

#[cfg(feature = "encryption")]
pub mod encryption;

// Re-export public types
pub use connection::{ConnectionInfo, parse_connection_string};
pub use credentials::Credentials;

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
}
