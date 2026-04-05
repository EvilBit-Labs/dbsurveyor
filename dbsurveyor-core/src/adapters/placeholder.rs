//! Placeholder adapter macro and utilities.
//!
//! This module provides a macro for generating placeholder database adapter
//! implementations. These adapters are used as stubs for database engines
//! that have not yet been fully implemented.
//!
//! # Usage
//!
//! The `define_placeholder_adapter!` macro generates a complete adapter
//! implementation with:
//! - A struct with `ConnectionConfig`
//! - A constructor that returns a placeholder
//! - `DatabaseAdapter` trait implementation with placeholder methods
//! - Feature support configuration
//!
//! # Example
//!
//! ```rust,ignore
//! define_placeholder_adapter!(
//!     MySqlAdapter,
//!     "MySQL",
//!     DatabaseType::MySQL,
//!     [SchemaCollection, DataSampling, MultiDatabase, ConnectionPooling, QueryTimeout, ReadOnlyMode]
//! );
//! ```

/// Generates a placeholder database adapter implementation.
///
/// This macro reduces boilerplate for database adapters that are not yet
/// implemented, providing a consistent structure and graceful sampling behavior.
///
/// # Parameters
///
/// - `$adapter_name`: The name of the adapter struct (e.g., `MySqlAdapter`)
/// - `$display_name`: Human-readable name for error messages (e.g., `"MySQL"`)
/// - `$db_type`: The `DatabaseType` enum variant
/// - `$features`: Array of `AdapterFeature` variants (accepted for compatibility but ignored;
///   placeholder adapters always report `false` for all features)
///
/// # Generated Code
///
/// The macro generates:
/// 1. A public struct with `ConnectionConfig` field
/// 2. An async `new` constructor returning a placeholder
/// 3. Full `DatabaseAdapter` trait implementation
#[macro_export]
macro_rules! define_placeholder_adapter {
    (
        $adapter_name:ident,
        $display_name:literal,
        $db_type:expr,
        [$($feature:ident),* $(,)?]
    ) => {
        /// Placeholder database adapter (not yet implemented).
        ///
        /// This adapter will be fully implemented in future releases.
        /// Currently, it provides the structural foundation for the adapter
        /// and returns placeholder behavior for unsupported operations.
        pub struct $adapter_name {
            config: $crate::adapters::ConnectionConfig,
        }

        impl $adapter_name {
            /// Creates a new placeholder adapter.
            ///
            /// # Arguments
            /// * `_connection_string` - Ignored in placeholder implementation
            ///
            /// # Returns
            /// A placeholder adapter instance
            pub async fn new(_connection_string: &str) -> $crate::Result<Self> {
                Ok(Self {
                    config: $crate::adapters::ConnectionConfig::default(),
                })
            }
        }

        #[async_trait::async_trait]
        impl $crate::adapters::DatabaseAdapter for $adapter_name {
            async fn test_connection(&self) -> $crate::Result<()> {
                Err($crate::error::DbSurveyorError::configuration(concat!(
                    $display_name,
                    " adapter not yet implemented"
                )))
            }

            async fn collect_schema(&self) -> $crate::Result<$crate::models::DatabaseSchema> {
                let db_info = $crate::models::DatabaseInfo::new("placeholder".to_string());
                Ok($crate::models::DatabaseSchema::new(db_info))
            }

            fn database_type(&self) -> $crate::models::DatabaseType {
                $db_type
            }

            fn supports_feature(&self, _feature: $crate::adapters::AdapterFeature) -> bool {
                false
            }

            async fn sample_table(
                &self,
                table_ref: $crate::adapters::TableRef<'_>,
                _config: &$crate::adapters::SamplingConfig,
            ) -> $crate::Result<$crate::models::TableSample> {
                Ok($crate::models::TableSample {
                    table_name: table_ref.table_name.to_string(),
                    schema_name: table_ref.schema_name.map(str::to_string),
                    rows: Vec::new(),
                    sample_size: 0,
                    total_rows: None,
                    sampling_strategy: $crate::models::SamplingStrategy::None,
                    collected_at: chrono::Utc::now(),
                    warnings: vec![
                        concat!($display_name, " adapter not yet implemented").to_string(),
                    ],
                    sample_status: Some($crate::models::SampleStatus::Skipped {
                        reason: concat!($display_name, " adapter not yet implemented").to_string(),
                    }),
                })
            }

            fn connection_config(&self) -> $crate::adapters::ConnectionConfig {
                self.config.clone()
            }
        }
    };
}

// Re-export the macro at module level
pub use define_placeholder_adapter;

#[cfg(test)]
mod tests {
    use crate::adapters::{AdapterFeature, DatabaseAdapter, SamplingConfig, TableRef};
    use crate::models::{DatabaseType, SampleStatus};

    // Define a test adapter using the macro
    define_placeholder_adapter!(
        TestPlaceholderAdapter,
        "Test",
        DatabaseType::MySQL,
        [SchemaCollection, DataSampling]
    );

    #[tokio::test]
    async fn test_placeholder_adapter_creation() {
        let adapter = TestPlaceholderAdapter::new("test://connection").await;
        assert!(adapter.is_ok());
    }

    #[tokio::test]
    async fn test_placeholder_adapter_test_connection_fails() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .unwrap();
        let result = adapter.test_connection().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not yet implemented")
        );
    }

    #[tokio::test]
    async fn test_placeholder_adapter_collect_schema_returns_empty() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .unwrap();
        let result = adapter.collect_schema().await;
        assert!(result.is_ok());
        let schema = result.unwrap();
        assert_eq!(schema.database_info.name, "placeholder");
    }

    #[tokio::test]
    async fn test_placeholder_adapter_database_type() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .unwrap();
        assert_eq!(adapter.database_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    async fn test_placeholder_adapter_supports_feature() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .expect("failed to create test adapter");
        // Placeholder adapters should report false for all features since
        // they cannot actually perform any operations.
        assert!(!adapter.supports_feature(AdapterFeature::SchemaCollection));
        assert!(!adapter.supports_feature(AdapterFeature::DataSampling));
        assert!(!adapter.supports_feature(AdapterFeature::MultiDatabase));
        assert!(!adapter.supports_feature(AdapterFeature::ConnectionPooling));
    }

    #[tokio::test]
    async fn test_placeholder_adapter_connection_config() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .unwrap();
        let config = adapter.connection_config();
        assert_eq!(config.host, "localhost"); // Default value
    }

    #[tokio::test]
    async fn test_placeholder_adapter_sample_table_returns_skipped_status() {
        let adapter = TestPlaceholderAdapter::new("test://connection")
            .await
            .expect("failed to create test adapter");

        let sample = adapter
            .sample_table(
                TableRef {
                    schema_name: Some("public"),
                    table_name: "users",
                },
                &SamplingConfig::default(),
            )
            .await
            .expect("placeholder sampling should return a skipped sample");

        assert_eq!(sample.table_name, "users");
        assert_eq!(sample.schema_name.as_deref(), Some("public"));
        assert_eq!(sample.sample_size, 0);
        assert!(sample.rows.is_empty());
        assert_eq!(
            sample.sampling_strategy,
            crate::models::SamplingStrategy::None
        );
        match sample.sample_status {
            Some(SampleStatus::Skipped { reason }) => {
                assert!(reason.contains("not yet implemented"));
            }
            other => panic!("expected skipped sample status, got {:?}", other),
        }
    }

    #[test]
    fn test_placeholder_adapter_is_object_safe() {
        fn into_boxed_adapter(adapter: TestPlaceholderAdapter) -> Box<dyn DatabaseAdapter> {
            Box::new(adapter)
        }

        let adapter = futures::executor::block_on(TestPlaceholderAdapter::new("test://connection"))
            .expect("failed to create test adapter");
        let boxed = into_boxed_adapter(adapter);
        assert_eq!(boxed.database_type(), DatabaseType::MySQL);
    }
}
