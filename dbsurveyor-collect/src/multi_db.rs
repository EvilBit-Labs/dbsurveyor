//! Multi-database collection support.
//!
//! Handles collecting schemas from all databases on a server.
//! Currently supported for PostgreSQL only.

use dbsurveyor_core::{MultiDatabaseConfig, Result, error::redact_database_url};
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::output::save_json;

/// Collects schemas from all databases on the server.
///
/// This function handles the `--all-databases` flag by detecting the database
/// type and using the engine-specific multi-database collection support.
/// Currently supported for PostgreSQL (which supports database enumeration
/// via `pg_database`).
///
/// The output is a `MultiDatabaseResult` JSON file containing all collected
/// schemas, failures, and collection metadata.
pub async fn collect_all_databases(
    database_url: &str,
    output_path: &PathBuf,
    config: &MultiDatabaseConfig,
) -> Result<()> {
    info!("Starting multi-database collection...");
    info!("Target: {}", redact_database_url(database_url));
    info!("Output: {}", output_path.display());

    // Detect database type and dispatch to engine-specific collection
    #[cfg(feature = "postgresql")]
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        return collect_all_databases_postgres(database_url, output_path, config).await;
    }

    // Unsupported database type for multi-database collection
    Err(dbsurveyor_core::error::DbSurveyorError::unsupported_feature(
        "Multi-database collection",
        "Multi-database collection is currently only supported for PostgreSQL",
    ))
}

/// PostgreSQL-specific multi-database collection.
///
/// Uses `PostgresAdapter` directly (not through the trait) because
/// multi-database collection requires concrete adapter methods for
/// database enumeration and per-database connection management.
#[cfg(feature = "postgresql")]
async fn collect_all_databases_postgres(
    database_url: &str,
    output_path: &PathBuf,
    config: &MultiDatabaseConfig,
) -> Result<()> {
    use dbsurveyor_core::adapters::postgres::PostgresAdapter;

    let adapter = PostgresAdapter::new(database_url).await.map_err(|e| {
        error!("Failed to create PostgreSQL adapter: {}", e);
        e
    })?;

    info!("Created PostgreSQL adapter for multi-database collection");

    let result = adapter.collect_all_databases(config).await.map_err(|e| {
        error!("Multi-database collection failed: {}", e);
        e
    })?;

    // Report results
    info!(
        "Multi-database collection completed: {} collected, {} failed, {} filtered",
        result.collection_metadata.databases_collected,
        result.collection_metadata.databases_failed,
        result.collection_metadata.databases_filtered,
    );

    for db in &result.databases {
        info!(
            "  {} - {} tables, {} views ({}ms)",
            db.database_name,
            db.schema.tables.len(),
            db.schema.views.len(),
            db.collection_duration_ms,
        );
    }

    for failure in &result.failures {
        warn!(
            "  {} - FAILED: {}",
            failure.database_name, failure.error_message,
        );
    }

    // Serialize and save
    let json_data = serde_json::to_string_pretty(&result).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON serialization", e)
    })?;

    save_json(&json_data, output_path).await?;

    info!("Multi-database results saved to {}", output_path.display());
    println!("Multi-database collection completed successfully");
    println!("Output: {}", output_path.display());
    println!(
        "Databases collected: {}",
        result.collection_metadata.databases_collected
    );
    println!(
        "Databases failed: {}",
        result.collection_metadata.databases_failed
    );

    let total_tables: usize = result.databases.iter().map(|d| d.schema.tables.len()).sum();
    println!("Total tables: {}", total_tables);

    Ok(())
}
