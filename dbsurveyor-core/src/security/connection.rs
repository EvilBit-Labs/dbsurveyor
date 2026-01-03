//! Connection string parsing and credential extraction.
//!
//! This module provides utilities for parsing database connection strings
//! and safely extracting credentials into secure containers.
//!
//! # Security
//! - Credentials are immediately moved into `Zeroizing` containers
//! - Original connection string is not modified
//! - Password is never stored in plain String

use super::credentials::Credentials;

/// Connection information with credentials removed.
///
/// This struct stores connection details (host, port, database, etc.)
/// without any sensitive credential information. It can be safely
/// logged, displayed, or serialized.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Database protocol scheme (e.g., "postgres", "mysql")
    pub scheme: String,
    /// Database host address
    pub host: String,
    /// Optional port number
    pub port: Option<u16>,
    /// Optional database name
    pub database: Option<String>,
    /// Additional query parameters
    pub query_params: Vec<(String, String)>,
}

impl ConnectionInfo {
    /// Reconstructs a connection string without credentials.
    ///
    /// # Returns
    /// A safe connection URL string that can be logged or displayed
    ///
    /// # Example
    /// ```rust
    /// use dbsurveyor_core::security::ConnectionInfo;
    ///
    /// let info = ConnectionInfo {
    ///     scheme: "postgres".to_string(),
    ///     host: "localhost".to_string(),
    ///     port: Some(5432),
    ///     database: Some("mydb".to_string()),
    ///     query_params: vec![],
    /// };
    /// assert_eq!(info.to_safe_string(), "postgres://localhost:5432/mydb");
    /// ```
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

/// Parses a database connection string and extracts credentials safely.
///
/// This function parses a database URL and separates the connection
/// information from credentials. Credentials are immediately moved
/// into secure `Zeroizing` containers.
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
/// # Errors
/// Returns error if the connection string format is invalid
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
        String::new()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_postgres_connection_string() {
        let (config, creds) =
            parse_connection_string("postgres://admin:secret@db.example.com:5432/production")
                .unwrap();

        assert_eq!(config.scheme, "postgres");
        assert_eq!(config.host, "db.example.com");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("production".to_string()));
        assert_eq!(creds.username(), "admin");
        assert!(creds.has_password());
    }

    #[test]
    fn test_parse_mysql_connection_string() {
        let (config, creds) =
            parse_connection_string("mysql://root:password@localhost:3306/mydb").unwrap();

        assert_eq!(config.scheme, "mysql");
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(3306));
        assert_eq!(config.database, Some("mydb".to_string()));
        assert_eq!(creds.username(), "root");
        assert!(creds.has_password());
    }

    #[test]
    fn test_parse_connection_string_with_query_params() {
        let (config, _creds) = parse_connection_string(
            "postgres://user:pass@localhost/db?sslmode=require&connect_timeout=10",
        )
        .unwrap();

        assert!(!config.query_params.is_empty());
        assert!(
            config
                .query_params
                .iter()
                .any(|(k, v)| k == "sslmode" && v == "require")
        );
    }

    #[test]
    fn test_parse_connection_string_no_port() {
        let (config, _creds) =
            parse_connection_string("postgres://user:pass@localhost/testdb").unwrap();

        assert_eq!(config.port, None);
    }

    #[test]
    fn test_parse_connection_string_no_database() {
        let (config, _creds) =
            parse_connection_string("postgres://user:pass@localhost:5432").unwrap();

        assert_eq!(config.database, None);
    }

    #[test]
    fn test_connection_info_to_safe_string_full() {
        let info = ConnectionInfo {
            scheme: "postgres".to_string(),
            host: "example.com".to_string(),
            port: Some(5432),
            database: Some("testdb".to_string()),
            query_params: vec![
                ("sslmode".to_string(), "require".to_string()),
                ("timeout".to_string(), "30".to_string()),
            ],
        };

        let safe = info.to_safe_string();
        assert_eq!(
            safe,
            "postgres://example.com:5432/testdb?sslmode=require&timeout=30"
        );
        assert!(!safe.contains("password"));
    }

    #[test]
    fn test_connection_info_to_safe_string_minimal() {
        let info = ConnectionInfo {
            scheme: "mysql".to_string(),
            host: "localhost".to_string(),
            port: None,
            database: None,
            query_params: vec![],
        };

        assert_eq!(info.to_safe_string(), "mysql://localhost");
    }

    #[test]
    fn test_parse_invalid_connection_string() {
        let result = parse_connection_string("not-a-valid-url");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_connection_string_no_host() {
        let result = parse_connection_string("file:///path/to/db");
        assert!(result.is_err());
    }
}
