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
    //! Placeholder encryption module.
    //!
    //! This module will be implemented in a future task with proper
    //! AES-GCM encryption and Argon2id key derivation.

    use serde::{Deserialize, Serialize};

    /// Key derivation parameters (placeholder)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KdfParams {
        pub salt: Vec<u8>,
        pub memory_cost: u32,
        pub time_cost: u32,
        pub parallelism: u32,
    }

    impl Default for KdfParams {
        fn default() -> Self {
            Self {
                salt: Vec::new(),
                memory_cost: 65536,
                time_cost: 3,
                parallelism: 4,
            }
        }
    }

    /// Encrypted data container (placeholder)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EncryptedData {
        pub algorithm: String,
        pub nonce: Vec<u8>,
        pub ciphertext: Vec<u8>,
        pub auth_tag: Vec<u8>,
        pub kdf_params: KdfParams,
    }

    /// Placeholder encrypt function
    pub fn encrypt_data(_data: &[u8], _password: &str) -> crate::Result<EncryptedData> {
        Err(crate::error::DbSurveyorError::configuration(
            "Encryption not yet implemented. Will be added in future task.",
        ))
    }

    /// Placeholder decrypt function
    pub fn decrypt_data(_encrypted: &EncryptedData, _password: &str) -> crate::Result<Vec<u8>> {
        Err(crate::error::DbSurveyorError::configuration(
            "Decryption not yet implemented. Will be added in future task.",
        ))
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
}
