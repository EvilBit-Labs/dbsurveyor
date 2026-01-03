//! Database connection configuration.
//!
//! This module provides the `ConnectionConfig` struct for configuring
//! database connections with security-focused defaults.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for database connections.
///
/// # Security
/// This struct intentionally does NOT store passwords or credentials.
/// Credentials must be handled separately and never logged or serialized.
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::ConnectionConfig;
///
/// let config = ConnectionConfig::new("localhost".to_string())
///     .with_port(5432)
///     .with_database("mydb".to_string())
///     .with_username("admin".to_string());
///
/// assert!(config.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Database host address
    pub host: String,
    /// Optional port number
    pub port: Option<u16>,
    /// Optional database name
    pub database: Option<String>,
    /// Optional username (password handled separately)
    pub username: Option<String>,
    /// Connection timeout duration
    pub connect_timeout: Duration,
    /// Query timeout duration
    pub query_timeout: Duration,
    /// Maximum number of connections in pool
    pub max_connections: u32,
    /// Whether to enforce read-only mode
    pub read_only: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: None,
            database: None,
            username: None,
            connect_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(30),
            max_connections: 10,
            read_only: true,
        }
    }
}

impl std::fmt::Display for ConnectionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConnectionConfig({}{}{})",
            self.host,
            self.port.map_or_else(String::new, |p| format!(":{}", p)),
            self.database
                .as_ref()
                .map_or_else(String::new, |db| format!("/{}", db))
        )
        // Intentionally omit username and never include credentials
    }
}

impl ConnectionConfig {
    /// Validates connection configuration parameters.
    ///
    /// # Errors
    /// Returns error if configuration values are invalid or unsafe
    pub fn validate(&self) -> crate::Result<()> {
        if self.host.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(
                "host cannot be empty",
            ));
        }

        if let Some(port) = self.port {
            if port == 0 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "port must be greater than 0",
                ));
            }
        }

        if self.max_connections == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_connections must be greater than 0",
            ));
        }

        if self.max_connections > 100 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_connections should not exceed 100 for safety",
            ));
        }

        if self.connect_timeout.as_secs() == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "connect_timeout must be greater than 0",
            ));
        }

        if self.query_timeout.as_secs() == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "query_timeout must be greater than 0",
            ));
        }

        Ok(())
    }

    /// Creates a new connection config with safe defaults.
    pub fn new(host: String) -> Self {
        Self {
            host,
            ..Default::default()
        }
    }

    /// Builder method to set port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Builder method to set database.
    pub fn with_database(mut self, database: String) -> Self {
        self.database = Some(database);
        self
    }

    /// Builder method to set username.
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, None);
        assert_eq!(config.max_connections, 10);
        assert!(config.read_only);
    }

    #[test]
    fn test_connection_config_validation() {
        // Valid config should pass
        let config = ConnectionConfig::new("localhost".to_string());
        assert!(config.validate().is_ok());

        // Empty host should fail
        let config = ConnectionConfig {
            host: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid port should fail
        let config = ConnectionConfig {
            port: Some(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Too many connections should fail
        let config = ConnectionConfig {
            max_connections: 101,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_connection_config_builder() {
        let config = ConnectionConfig::new("example.com".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("admin".to_string());

        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("admin".to_string()));
    }

    #[test]
    fn test_connection_config_display_no_credentials() {
        let config = ConnectionConfig::new("example.com".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("testuser".to_string());

        let display = format!("{}", config);

        // Should contain connection info
        assert!(display.contains("example.com"));
        assert!(display.contains("5432"));
        assert!(display.contains("testdb"));

        // Should NOT contain username (security)
        assert!(!display.contains("testuser"));
    }
}
