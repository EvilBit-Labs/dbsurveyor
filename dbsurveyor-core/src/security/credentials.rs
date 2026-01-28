//! Secure credential container with automatic memory zeroing.
//!
//! This module provides the `Credentials` struct which securely stores
//! database credentials with automatic memory clearing on drop using
//! the `zeroize` crate.
//!
//! # Security
//! - Credentials are stored in `Zeroizing<T>` containers
//! - Memory is automatically cleared when credentials go out of scope
//! - Passwords are never exposed in debug output or logs

use zeroize::{Zeroize, Zeroizing};

/// Secure credential container that automatically zeros memory on drop.
///
/// This struct wraps username and optional password in `Zeroizing` containers
/// to ensure sensitive data is cleared from memory when no longer needed.
///
/// # Example
///
/// ```rust
/// use dbsurveyor_core::security::Credentials;
///
/// let creds = Credentials::new("admin".to_string(), Some("secret".to_string()));
/// assert_eq!(creds.username(), "admin");
/// assert!(creds.has_password());
/// // Password is automatically zeroed when `creds` is dropped
/// ```
#[derive(Debug, Clone, Zeroize)]
#[zeroize(drop)]
pub struct Credentials {
    pub username: Zeroizing<String>,
    pub password: Zeroizing<Option<String>>,
}

impl Credentials {
    /// Creates new credentials with automatic memory zeroing.
    ///
    /// # Arguments
    /// * `username` - Database username
    /// * `password` - Optional database password
    ///
    /// # Returns
    /// A new `Credentials` instance with secure storage
    pub fn new(username: String, password: Option<String>) -> Self {
        Self {
            username: Zeroizing::new(username),
            password: Zeroizing::new(password),
        }
    }

    /// Gets the username (still protected by Zeroizing).
    ///
    /// # Returns
    /// Reference to the username string
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Checks if password is present without exposing it.
    ///
    /// # Returns
    /// `true` if a password was provided, `false` otherwise
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_new() {
        let creds = Credentials::new("testuser".to_string(), Some("testpass".to_string()));
        assert_eq!(creds.username(), "testuser");
        assert!(creds.has_password());
    }

    #[test]
    fn test_credentials_no_password() {
        let creds = Credentials::new("testuser".to_string(), None);
        assert_eq!(creds.username(), "testuser");
        assert!(!creds.has_password());
    }

    #[test]
    fn test_credentials_empty_username() {
        let creds = Credentials::new(String::new(), Some("password".to_string()));
        assert_eq!(creds.username(), "");
        assert!(creds.has_password());
    }

    #[test]
    fn test_credentials_clone() {
        let creds1 = Credentials::new("user".to_string(), Some("pass".to_string()));
        let creds2 = creds1.clone();
        assert_eq!(creds1.username(), creds2.username());
        assert_eq!(creds1.has_password(), creds2.has_password());
    }
}
