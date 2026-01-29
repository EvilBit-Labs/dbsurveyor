//! MongoDB connection management.
//!
//! This module handles MongoDB client creation, connection validation,
//! and connection string parsing.
//!
//! # Security Features
//! - Connection string credentials are never logged
//! - Connection validation without side effects
//! - Timeout configuration for all operations

use super::MongoAdapter;
use crate::Result;
use crate::adapters::ConnectionConfig;
use mongodb::Client;
use mongodb::options::ClientOptions;
use std::time::Duration;
use url::Url;

impl MongoAdapter {
    /// Creates a new MongoDB adapter from a connection string.
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URL (credentials sanitized in errors)
    ///
    /// # Security
    /// - Validates connection string format
    /// - Sanitizes connection string in all error messages
    /// - Sets appropriate timeouts for all operations
    ///
    /// # Errors
    /// Returns error if:
    /// - Connection string format is invalid
    /// - Client creation fails
    /// - Configuration is invalid
    pub async fn new(connection_string: &str) -> Result<Self> {
        // Parse and validate connection configuration
        let config = Self::parse_connection_config(connection_string)?;

        // Create MongoDB client options
        let client_options = Self::create_client_options(connection_string, &config).await?;

        // Create the client
        let client = Client::with_options(client_options).map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to create MongoDB client for {}",
                    crate::adapters::redact_database_url(connection_string)
                ),
                e,
            )
        })?;

        Ok(Self {
            client,
            config,
            connection_url: connection_string.to_string(),
        })
    }

    /// Creates a new MongoDB adapter with custom configuration.
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URL
    /// * `config` - Custom connection configuration
    ///
    /// # Security
    /// Same security guarantees as `new()` but allows custom configuration
    pub async fn with_config(connection_string: &str, config: ConnectionConfig) -> Result<Self> {
        // Validate the provided configuration
        config.validate()?;

        // Validate connection string
        Self::validate_connection_string(connection_string)?;

        // Create MongoDB client options
        let client_options = Self::create_client_options(connection_string, &config).await?;

        // Create the client
        let client = Client::with_options(client_options).map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to create MongoDB client for {}",
                    crate::adapters::redact_database_url(connection_string)
                ),
                e,
            )
        })?;

        Ok(Self {
            client,
            config,
            connection_url: connection_string.to_string(),
        })
    }

    /// Parses a MongoDB connection string to extract configuration.
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URL
    ///
    /// # Returns
    /// Validated connection configuration with security defaults
    ///
    /// # Errors
    /// Returns error if connection string is malformed
    pub fn parse_connection_config(connection_string: &str) -> Result<ConnectionConfig> {
        // Validate connection string first
        Self::validate_connection_string(connection_string)?;

        let url = Url::parse(connection_string).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Invalid MongoDB connection string format: {}",
                e
            ))
        })?;

        // Start with security-focused defaults
        let mut config = ConnectionConfig::new(url.host_str().unwrap_or("localhost").to_string());

        // Set port with validation
        if let Some(port) = url.port() {
            if port == 0 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Invalid port number: must be greater than 0",
                ));
            }
            config = config.with_port(port);
        } else {
            config = config.with_port(27017); // MongoDB default port
        }

        // Extract database name from path
        let path = url.path().trim_start_matches('/');
        if !path.is_empty() {
            config = config.with_database(path.to_string());
        }

        // Extract username
        let username = url.username();
        if !username.is_empty() {
            config = config.with_username(username.to_string());
        }

        // Parse query parameters for additional configuration
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "connectTimeoutMS" => {
                    if let Ok(timeout_ms) = value.parse::<u64>()
                        && timeout_ms > 0
                        && timeout_ms <= 300_000
                    {
                        config.connect_timeout = Duration::from_millis(timeout_ms);
                    }
                }
                "serverSelectionTimeoutMS" => {
                    if let Ok(timeout_ms) = value.parse::<u64>()
                        && timeout_ms > 0
                        && timeout_ms <= 300_000
                    {
                        config.query_timeout = Duration::from_millis(timeout_ms);
                    }
                }
                "maxPoolSize" => {
                    if let Ok(max_pool) = value.parse::<u32>()
                        && max_pool > 0
                        && max_pool <= 100
                    {
                        config.max_connections = max_pool;
                    }
                }
                "minPoolSize" => {
                    if let Ok(min_pool) = value.parse::<u32>()
                        && min_pool <= 100
                    {
                        config.min_idle_connections = min_pool;
                    }
                }
                _ => {} // Ignore other parameters
            }
        }

        // Final validation of the complete configuration
        config.validate()?;

        Ok(config)
    }

    /// Validates a MongoDB connection string.
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URL to validate
    ///
    /// # Errors
    /// Returns error if connection string is invalid or unsafe
    pub fn validate_connection_string(connection_string: &str) -> Result<()> {
        // Parse URL to validate format
        let url = Url::parse(connection_string).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Invalid MongoDB connection string format: {}",
                e
            ))
        })?;

        // Validate scheme
        if !matches!(url.scheme(), "mongodb" | "mongodb+srv") {
            return Err(crate::error::DbSurveyorError::configuration(
                "Connection string must use mongodb:// or mongodb+srv:// scheme",
            ));
        }

        // Validate host is present
        if url.host_str().is_none() {
            return Err(crate::error::DbSurveyorError::configuration(
                "Connection string must specify a host",
            ));
        }

        Ok(())
    }

    /// Creates MongoDB client options with security settings.
    ///
    /// # Arguments
    /// * `connection_string` - MongoDB connection URL
    /// * `config` - Connection configuration
    ///
    /// # Returns
    /// Configured client options
    async fn create_client_options(
        connection_string: &str,
        config: &ConnectionConfig,
    ) -> Result<ClientOptions> {
        let mut options = ClientOptions::parse(connection_string).await.map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Failed to parse MongoDB connection options: {}",
                e
            ))
        })?;

        // Apply configuration overrides
        options.connect_timeout = Some(config.connect_timeout);
        options.server_selection_timeout = Some(config.query_timeout);

        // Connection pool settings
        options.max_pool_size = Some(config.max_connections);
        options.min_pool_size = Some(config.min_idle_connections);

        if let Some(idle_timeout) = config.idle_timeout {
            options.max_idle_time = Some(idle_timeout);
        }

        // Set application name for connection tracking
        options.app_name = Some(format!("dbsurveyor-collect-{}", env!("CARGO_PKG_VERSION")));

        Ok(options)
    }

    /// Tests the MongoDB connection.
    ///
    /// # Returns
    /// Ok if connection is successful, error otherwise
    pub async fn test_connection_internal(&self) -> Result<()> {
        // List databases to verify connection works
        let _ = self
            .client
            .list_database_names()
            .await
            .map_err(crate::error::DbSurveyorError::connection_failed)?;

        Ok(())
    }

    /// Gets the MongoDB client reference.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Gets the default database name from the connection URL.
    pub fn default_database(&self) -> Option<String> {
        self.config.database.clone()
    }

    /// Creates a new adapter connected to a specific database.
    ///
    /// # Arguments
    /// * `database` - Name of the database to connect to
    ///
    /// # Returns
    /// A new `MongoAdapter` configured for the specified database
    pub fn for_database(&self, database: &str) -> Result<Self> {
        let mut config = self.config.clone();
        config.database = Some(database.to_string());

        Ok(Self {
            client: self.client.clone(),
            config,
            connection_url: self.connection_url.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connection_config() {
        let connection_string = "mongodb://testuser@localhost:27017/testdb";
        let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(27017));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
    }

    #[test]
    fn test_parse_connection_config_with_query_params() {
        let connection_string =
            "mongodb://user@host/db?connectTimeoutMS=5000&maxPoolSize=20&minPoolSize=5";
        let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "host");
        assert_eq!(config.connect_timeout, Duration::from_millis(5000));
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_idle_connections, 5);
    }

    #[test]
    fn test_parse_connection_config_defaults() {
        let connection_string = "mongodb://localhost";
        let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(27017)); // Default MongoDB port
        assert_eq!(config.database, None);
        assert_eq!(config.username, None);
    }

    #[test]
    fn test_parse_connection_config_srv() {
        let connection_string = "mongodb+srv://user@cluster.example.com/testdb";
        let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "cluster.example.com");
        assert_eq!(config.database, Some("testdb".to_string()));
    }

    #[test]
    fn test_validate_connection_string_valid() {
        assert!(MongoAdapter::validate_connection_string("mongodb://localhost:27017/test").is_ok());
        assert!(
            MongoAdapter::validate_connection_string("mongodb+srv://cluster.example.com/test")
                .is_ok()
        );
    }

    #[test]
    fn test_validate_connection_string_invalid_scheme() {
        let result = MongoAdapter::validate_connection_string("postgres://localhost/db");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("mongodb://"));
    }

    #[test]
    fn test_validate_connection_string_no_host() {
        let result = MongoAdapter::validate_connection_string("mongodb:///db");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("host"));
    }

    #[test]
    fn test_parse_connection_config_invalid_port() {
        let connection_string = "mongodb://user@host:0/db";
        let result = MongoAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("port"));
    }
}
