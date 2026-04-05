//! Schema collection orchestration and supporting helpers.
//!
//! Contains the main `collect_schema` workflow, data-sampling helpers,
//! quality-threshold parsing, and the `list` subcommand implementation.

use crate::Cli;
use crate::outcome::CollectionOutcome;
use crate::sampling::SamplingOrchestrator;
#[cfg(feature = "postgresql")]
use dbsurveyor_core::adapters::postgres::PostgresAdapter;
use dbsurveyor_core::{
    CollectionMode, CollectionStatus, DatabaseAdapter, DatabaseInfo, DatabaseSchema,
    DatabaseServerSchema, DatabaseType, Result, SamplingConfig, ServerInfo,
    adapters::create_adapter,
    error::redact_database_url,
    quality::{AnomalyConfig, QualityAnalyzer, QualityConfig},
};
use std::path::PathBuf;
use tracing::{error, info, warn};

/// Parsed quality threshold values from CLI arguments.
pub(crate) struct QualityThresholds {
    pub(crate) completeness: Option<f64>,
    pub(crate) uniqueness: Option<f64>,
    pub(crate) consistency: Option<f64>,
}

/// Parses quality thresholds from CLI arguments.
pub(crate) fn parse_quality_thresholds(thresholds: &[String]) -> QualityThresholds {
    let mut completeness = None;
    let mut uniqueness = None;
    let mut consistency = None;

    for threshold in thresholds {
        if let Some((metric, value)) = threshold.split_once(':') {
            if let Ok(v) = value.parse::<f64>() {
                // Validate threshold is in valid range
                if !(0.0..=1.0).contains(&v) {
                    warn!(
                        "Threshold value {} for {} is outside valid range [0.0, 1.0]",
                        v, metric
                    );
                }
                match metric.to_lowercase().as_str() {
                    "completeness" => completeness = Some(v.clamp(0.0, 1.0)),
                    "uniqueness" => uniqueness = Some(v.clamp(0.0, 1.0)),
                    "consistency" => consistency = Some(v.clamp(0.0, 1.0)),
                    _ => warn!("Unknown quality metric: {}", metric),
                }
            } else {
                warn!("Invalid threshold value for {}: {}", metric, value);
            }
        } else {
            warn!(
                "Invalid quality threshold format '{}', expected 'metric:value'",
                threshold
            );
        }
    }

    QualityThresholds {
        completeness,
        uniqueness,
        consistency,
    }
}

/// Builds a `SamplingConfig` from CLI arguments.
pub(crate) fn build_sampling_config(cli: &Cli) -> SamplingConfig {
    let mut config = SamplingConfig::default().with_sample_size(cli.sample);

    if let Some(throttle_ms) = cli.throttle {
        config = config.with_throttle_ms(throttle_ms);
    }

    config
}

/// Returns whether sampling is enabled for this CLI invocation.
pub(crate) fn sampling_enabled(cli: &Cli) -> bool {
    cli.sample > 0
}

/// Collects database schema and saves to file.
pub(crate) async fn collect_schema(
    database_url: &str,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<CollectionOutcome> {
    // CWE-22: warn if output path contains parent-directory traversal
    if output_path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        warn!(
            "Output path contains '..' traversal: {}",
            output_path.display()
        );
    }

    info!("Starting schema collection...");
    info!("Target: {}", redact_database_url(database_url));
    info!("Output: {}", output_path.display());

    if cli.all_databases {
        return collect_all_databases(database_url, output_path, cli).await;
    }

    let adapter = create_adapter(database_url).await.map_err(|e| {
        error!("Failed to create database adapter: {}", e);
        e
    })?;

    info!("Created {} adapter", adapter.database_type());

    // Collect schema
    let mut schema = adapter.collect_schema().await.map_err(|e| {
        error!("Schema collection failed: {}", e);
        e
    })?;

    info!("[OK]Schema collection completed");
    info!("Found {} tables", schema.tables.len());
    info!("Found {} views", schema.views.len());
    info!("Found {} indexes", schema.indexes.len());

    // Run sampling only when explicitly enabled.
    if sampling_enabled(cli) && !schema.tables.is_empty() {
        let sampling_config = build_sampling_config(cli);
        info!(
            "Sampling {} tables (limit {} rows each)...",
            schema.tables.len(),
            sampling_config.sample_size
        );

        let sampling_run = SamplingOrchestrator::new(&*adapter, &sampling_config)
            .run(&schema.tables)
            .await;

        if sampling_run.samples.is_empty() {
            info!("No samples collected (all tables may have been empty or inaccessible)");
        } else {
            info!(
                "[OK]Collected samples from {} tables",
                sampling_run.samples.len()
            );
            for warning in sampling_run.warnings {
                schema = schema.with_warning(warning);
            }
            schema = schema.with_samples(sampling_run.samples);
        }
    }

    // Run quality analysis if enabled and samples exist
    if cli.enable_quality {
        if let Some(ref samples) = schema.samples {
            info!(
                "Running data quality analysis on {} samples...",
                samples.len()
            );

            // Build quality config
            let thresholds = parse_quality_thresholds(&cli.quality_threshold);

            let mut config = QualityConfig::new();

            if let Some(c) = thresholds.completeness {
                config = config.with_completeness_min(c);
            }
            if let Some(u) = thresholds.uniqueness {
                config = config.with_uniqueness_min(u);
            }
            if let Some(c) = thresholds.consistency {
                config = config.with_consistency_min(c);
            }

            if cli.disable_anomaly_detection {
                config = config.with_anomaly_detection(AnomalyConfig::new().with_enabled(false));
            }

            let analyzer = QualityAnalyzer::new(config);
            let quality_metrics = analyzer.analyze_all(samples)?;

            // Report quality findings
            let mut violations_count = 0;
            for metric in &quality_metrics {
                if !metric.threshold_violations.is_empty() {
                    violations_count += metric.threshold_violations.len();
                    for violation in &metric.threshold_violations {
                        warn!(
                            "Quality violation in '{}': {} = {:.2}% (threshold: {:.2}%)",
                            metric.table_name,
                            violation.metric,
                            violation.actual * 100.0,
                            violation.threshold * 100.0
                        );
                    }
                }
            }

            schema = schema.with_quality_metrics(quality_metrics);

            if violations_count > 0 {
                info!(
                    "[OK]Quality analysis completed with {} violations",
                    violations_count
                );
            } else {
                info!("[OK]Quality analysis completed - all thresholds met");
            }
        } else {
            info!("Quality analysis skipped - no samples available");
        }
    }

    // Save to file
    crate::output::save_schema(&schema, output_path, cli).await?;

    info!("[OK]Schema saved to {}", output_path.display());
    println!("Schema collection completed successfully");
    println!("Output: {}", output_path.display());
    println!("Tables: {}", schema.tables.len());
    println!("Views: {}", schema.views.len());
    println!("Indexes: {}", schema.indexes.len());

    if cli.enable_quality
        && let Some(ref metrics) = schema.quality_metrics
    {
        println!("Quality metrics: {} tables analyzed", metrics.len());
    }

    Ok(CollectionOutcome::from_results(&[schema]))
}

#[cfg(feature = "postgresql")]
async fn collect_all_databases(
    database_url: &str,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<CollectionOutcome> {
    let adapter = PostgresAdapter::new(database_url).await.map_err(|e| {
        error!(
            "Failed to create PostgreSQL adapter for multi-database collection: {}",
            e
        );
        e
    })?;

    if adapter.database_type() != DatabaseType::PostgreSQL {
        return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
            "--all-databases is currently supported only for PostgreSQL",
        ));
    }

    let enumerated = adapter
        .list_databases_with_options(cli.include_system_databases)
        .await?;
    let system_databases_excluded = if cli.include_system_databases {
        0
    } else {
        enumerated.iter().filter(|db| db.is_system_database).count()
    };

    let mut databases = Vec::new();

    for database in &enumerated {
        if cli
            .exclude_databases
            .iter()
            .any(|excluded| excluded == &database.name)
        {
            continue;
        }

        if !database.is_accessible {
            databases.push(skipped_database_schema(
                &database.name,
                Some(database.owner.clone()),
                database.is_system_database,
                "Database is not accessible with current privileges".to_string(),
            ));
            continue;
        }

        match adapter.connect_to_database(&database.name).await {
            Ok(database_adapter) => match database_adapter.collect_schema().await {
                Ok(mut schema) => {
                    if sampling_enabled(cli) && !schema.tables.is_empty() {
                        let sampling_config = build_sampling_config(cli);
                        let sampling_run =
                            SamplingOrchestrator::new(&database_adapter, &sampling_config)
                                .run(&schema.tables)
                                .await;
                        for warning in sampling_run.warnings {
                            schema = schema.with_warning(warning);
                        }
                        schema = schema.with_samples(sampling_run.samples);
                    }
                    databases.push(schema);
                }
                Err(err) => {
                    databases.push(failed_database_schema(
                        &database.name,
                        Some(database.owner.clone()),
                        database.is_system_database,
                        err.to_string(),
                    ));
                }
            },
            Err(err) => {
                databases.push(failed_database_schema(
                    &database.name,
                    Some(database.owner.clone()),
                    database.is_system_database,
                    err.to_string(),
                ));
            }
        }
    }

    let collected = databases
        .iter()
        .filter(|schema| {
            matches!(
                schema.database_info.collection_status,
                CollectionStatus::Success
            )
        })
        .count();
    let failed = databases
        .iter()
        .filter(|schema| {
            matches!(
                schema.database_info.collection_status,
                CollectionStatus::Failed { .. }
            )
        })
        .count();

    let server_schema = DatabaseServerSchema {
        format_version: dbsurveyor_core::FORMAT_VERSION.to_string(),
        server_info: ServerInfo {
            server_type: DatabaseType::PostgreSQL,
            version: "unknown".to_string(),
            host: adapter.config.host.clone(),
            port: adapter.config.port,
            total_databases: databases.len(),
            collected_databases: collected,
            system_databases_excluded,
            connection_user: adapter
                .config
                .username
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            has_superuser_privileges: false,
            collection_mode: CollectionMode::MultiDatabase {
                discovered: databases.len(),
                collected,
                failed,
            },
        },
        databases: databases.clone(),
        collection_metadata: dbsurveyor_core::models::CollectionMetadata {
            collected_at: chrono::Utc::now(),
            collection_duration_ms: 0,
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            warnings: Vec::new(),
        },
    };

    crate::output::save_server_schema(&server_schema, output_path, cli).await?;

    Ok(CollectionOutcome::from_results(&databases))
}

#[cfg(not(feature = "postgresql"))]
async fn collect_all_databases(
    _database_url: &str,
    _output_path: &PathBuf,
    _cli: &Cli,
) -> Result<CollectionOutcome> {
    Err(dbsurveyor_core::error::DbSurveyorError::configuration(
        "--all-databases requires the postgresql feature",
    ))
}

fn failed_database_schema(
    name: &str,
    owner: Option<String>,
    is_system_database: bool,
    error_message: String,
) -> DatabaseSchema {
    let mut info = DatabaseInfo::new(name.to_string());
    info.owner = owner;
    info.is_system_database = is_system_database;
    info.collection_status = CollectionStatus::Failed {
        error: error_message.clone(),
    };
    info.access_level = dbsurveyor_core::AccessLevel::None;

    DatabaseSchema::new(info).with_warning(error_message)
}

fn skipped_database_schema(
    name: &str,
    owner: Option<String>,
    is_system_database: bool,
    reason: String,
) -> DatabaseSchema {
    let mut info = DatabaseInfo::new(name.to_string());
    info.owner = owner;
    info.is_system_database = is_system_database;
    info.collection_status = CollectionStatus::Skipped {
        reason: reason.clone(),
    };
    info.access_level = dbsurveyor_core::AccessLevel::Limited;

    DatabaseSchema::new(info).with_warning(reason)
}

/// Lists supported database types and their connection string formats.
pub(crate) fn list_supported_databases() {
    println!("Supported Database Types:");
    println!();

    #[cfg(feature = "postgresql")]
    {
        println!("PostgreSQL:");
        println!("  Connection: postgres://user:password@host:port/database");
        println!("  Example:    postgres://admin:secret@localhost:5432/mydb");
        println!();
    }

    #[cfg(feature = "mysql")]
    {
        println!("MySQL:");
        println!("  Connection: mysql://user:password@host:port/database");
        println!("  Example:    mysql://root:password@localhost:3306/mydb");
        println!();
    }

    #[cfg(feature = "sqlite")]
    {
        println!("SQLite:");
        println!("  Connection: sqlite:///path/to/database.db");
        println!("  Example:    sqlite:///home/user/data.db");
        println!("  Example:    /path/to/database.sqlite");
        println!();
    }

    #[cfg(feature = "mongodb")]
    {
        println!("MongoDB:");
        println!("  Connection: mongodb://user:password@host:port/database");
        println!("  Example:    mongodb://admin:secret@localhost:27017/mydb");
        println!();
    }

    #[cfg(feature = "mssql")]
    {
        println!("SQL Server:");
        println!("  Connection: mssql://user:password@host:port/database");
        println!("  Example:    mssql://sa:password@localhost:1433/mydb");
        println!();
    }

    println!("Output Formats:");
    println!("  .json      - Plain JSON (default)");

    #[cfg(feature = "compression")]
    println!("  .json.zst  - Compressed JSON (--compress)");

    #[cfg(feature = "encryption")]
    println!("  .enc       - Encrypted JSON (--encrypt)");

    println!();
    println!("Security Features:");
    println!("  -Read-only database operations");
    println!("  -Credential sanitization in logs");
    println!("  -Optional AES-GCM encryption");
    println!("  -Offline operation after connection");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quality_thresholds_valid() {
        let input = vec![
            "completeness:0.9".to_string(),
            "uniqueness:0.95".to_string(),
            "consistency:0.85".to_string(),
        ];
        let result = parse_quality_thresholds(&input);
        assert!((result.completeness.unwrap_or(0.0) - 0.9).abs() < f64::EPSILON);
        assert!((result.uniqueness.unwrap_or(0.0) - 0.95).abs() < f64::EPSILON);
        assert!((result.consistency.unwrap_or(0.0) - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_quality_thresholds_empty() {
        let result = parse_quality_thresholds(&[]);
        assert!(result.completeness.is_none());
        assert!(result.uniqueness.is_none());
        assert!(result.consistency.is_none());
    }

    #[test]
    fn test_parse_quality_thresholds_invalid_format() {
        let input = vec!["not-a-threshold".to_string()];
        let result = parse_quality_thresholds(&input);
        assert!(result.completeness.is_none());
        assert!(result.uniqueness.is_none());
        assert!(result.consistency.is_none());
    }

    #[test]
    fn test_parse_quality_thresholds_invalid_value() {
        let input = vec!["completeness:abc".to_string()];
        let result = parse_quality_thresholds(&input);
        assert!(result.completeness.is_none());
    }

    #[test]
    fn test_parse_quality_thresholds_clamps_out_of_range() {
        let input = vec![
            "completeness:1.5".to_string(),
            "uniqueness:-0.5".to_string(),
        ];
        let result = parse_quality_thresholds(&input);
        assert!((result.completeness.unwrap_or(0.0) - 1.0).abs() < f64::EPSILON);
        assert!(result.uniqueness.unwrap_or(1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_quality_thresholds_unknown_metric() {
        let input = vec!["unknown_metric:0.5".to_string()];
        let result = parse_quality_thresholds(&input);
        assert!(result.completeness.is_none());
        assert!(result.uniqueness.is_none());
        assert!(result.consistency.is_none());
    }

    #[test]
    fn test_parse_quality_thresholds_case_insensitive() {
        let input = vec!["Completeness:0.8".to_string()];
        let result = parse_quality_thresholds(&input);
        assert!((result.completeness.unwrap_or(0.0) - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sampling_disabled_for_zero_sample() {
        let cli = Cli {
            global: crate::GlobalArgs {
                verbose: 0,
                quiet: false,
            },
            command: None,
            database_url: None,
            output: "schema.dbsurveyor.json".into(),
            sample: 0,
            throttle: None,
            compress: false,
            encrypt: false,
            all_databases: false,
            include_system_databases: false,
            exclude_databases: Vec::new(),
            enable_quality: false,
            quality_threshold: Vec::new(),
            disable_anomaly_detection: false,
        };

        assert!(!sampling_enabled(&cli));
    }

    #[test]
    fn test_build_sampling_config_preserves_explicit_nonzero_sample() {
        let cli = Cli {
            global: crate::GlobalArgs {
                verbose: 0,
                quiet: false,
            },
            command: None,
            database_url: None,
            output: "schema.dbsurveyor.json".into(),
            sample: 25,
            throttle: None,
            compress: false,
            encrypt: false,
            all_databases: false,
            include_system_databases: false,
            exclude_databases: Vec::new(),
            enable_quality: false,
            quality_threshold: Vec::new(),
            disable_anomaly_detection: false,
        };

        let config = build_sampling_config(&cli);
        assert_eq!(config.sample_size, 25);
    }
}
