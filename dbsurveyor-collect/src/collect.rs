//! Schema collection, quality analysis, and sensitive data detection.
//!
//! This module contains the core collection workflow including:
//! - Single-database schema collection with quality analysis
//! - Connection testing
//! - Quality threshold parsing
//! - Sensitive column detection
//! - Supported database listing

use dbsurveyor_core::{
    MultiDatabaseConfig, Result,
    adapters::{SamplingConfig, create_adapter},
    error::redact_database_url,
    models::DatabaseSchema,
    quality::{AnomalyConfig, QualityAnalyzer, QualityConfig},
};
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::Cli;
use crate::multi_db;
use crate::output;

/// Tests database connection without collecting schema.
pub async fn test_connection(database_url: &str) -> Result<()> {
    info!("Testing database connection...");

    let adapter = create_adapter(database_url).await.map_err(|e| {
        error!("Failed to create database adapter: {}", e);
        e
    })?;

    info!("Created {} adapter", adapter.database_type());

    adapter.test_connection().await.map_err(|e| {
        error!("Connection test failed: {}", e);
        e
    })?;

    info!("Connection test successful");
    println!(
        "Connection to {} database successful",
        adapter.database_type()
    );

    Ok(())
}

/// Parses quality thresholds from CLI arguments.
pub fn parse_quality_thresholds(thresholds: &[String]) -> (Option<f64>, Option<f64>, Option<f64>) {
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
        }
    }

    (completeness, uniqueness, consistency)
}

/// Collects database schema and saves to file.
pub async fn collect_schema(
    database_url: &str,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<()> {
    // Multi-database collection takes a completely separate code path
    if cli.all_databases {
        let multi_db_config = MultiDatabaseConfig::new()
            .with_include_system(cli.include_system_databases)
            .with_exclude_patterns(cli.exclude_databases.clone())
            .with_continue_on_error(true);
        return multi_db::collect_all_databases(database_url, output_path, &multi_db_config).await;
    }

    info!("Starting schema collection...");
    info!("Target: {}", redact_database_url(database_url));
    info!("Output: {}", output_path.display());

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

    info!("Schema collection completed");
    info!("Found {} tables", schema.tables.len());
    info!("Found {} views", schema.views.len());
    info!("Found {} indexes", schema.indexes.len());

    // Build sampling config from CLI flags
    let mut sampling_config = SamplingConfig::new().with_sample_size(cli.sample);
    if let Some(throttle_ms) = cli.throttle {
        sampling_config = sampling_config.with_throttle_ms(throttle_ms);
    }

    // Detect and warn about sensitive columns before sampling
    warn_sensitive_columns(&schema, &sampling_config);

    // Sample data from tables if sample size > 0
    if cli.sample > 0 && !schema.tables.is_empty() {
        info!(
            "Sampling up to {} rows per table from {} tables...",
            cli.sample,
            schema.tables.len()
        );

        match adapter.sample_tables(&schema, &sampling_config).await {
            Ok(samples) => {
                let sampled_count = samples.len();
                let total_rows: u32 = samples.iter().map(|s| s.sample_size).sum();
                schema.add_samples(samples);
                info!(
                    "Sampled {} tables ({} total rows)",
                    sampled_count, total_rows
                );
            }
            Err(e) => {
                warn!("Data sampling failed: {}", e);
            }
        }
    }

    // Run quality analysis if enabled and samples exist
    if cli.enable_quality {
        run_quality_analysis(&mut schema, cli)?;
    }

    // Save to file
    output::save_schema(&schema, output_path, cli).await?;

    info!("Schema saved to {}", output_path.display());
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

    Ok(())
}

/// Runs quality analysis on sampled data.
fn run_quality_analysis(
    schema: &mut DatabaseSchema,
    cli: &Cli,
) -> Result<()> {
    if let Some(ref samples) = schema.samples {
        info!(
            "Running data quality analysis on {} samples...",
            samples.len()
        );

        // Build quality config
        let (completeness, uniqueness, consistency) =
            parse_quality_thresholds(&cli.quality_threshold);

        let mut config = QualityConfig::new();

        if let Some(c) = completeness {
            config = config.with_completeness_min(c);
        }
        if let Some(u) = uniqueness {
            config = config.with_uniqueness_min(u);
        }
        if let Some(c) = consistency {
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

        schema.add_quality_metrics(quality_metrics);

        if violations_count > 0 {
            info!(
                "Quality analysis completed with {} violations",
                violations_count
            );
        } else {
            info!("Quality analysis completed - all thresholds met");
        }
    } else {
        info!("Quality analysis skipped - no samples available");
    }

    Ok(())
}

/// Checks column names against sensitive data patterns and logs warnings.
///
/// This function scans all column names in the collected schema for patterns
/// that suggest sensitive data (passwords, emails, SSNs, etc.). It only logs
/// warnings - it never redacts data. Redaction is handled by the postprocessor.
///
/// Pattern matching uses case-insensitive substring matching derived from
/// the `SamplingConfig::sensitive_detection_patterns` field. The pattern
/// strings are expected to contain simple keywords (regex groups are stripped).
pub fn warn_sensitive_columns(schema: &DatabaseSchema, config: &SamplingConfig) -> usize {
    if !config.warn_sensitive {
        return 0;
    }

    if config.sensitive_detection_patterns.is_empty() {
        return 0;
    }

    // Extract keywords from patterns (strip regex syntax for simple matching)
    let keywords: Vec<(Vec<String>, &str)> = config
        .sensitive_detection_patterns
        .iter()
        .map(|p| {
            // Extract keywords from patterns like "(?i)(password|passwd|pwd)"
            let cleaned = p.pattern.replace("(?i)", "");
            let cleaned = cleaned.trim_matches(|c| c == '(' || c == ')');
            let words: Vec<String> = cleaned
                .split('|')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            (words, p.description.as_str())
        })
        .collect();

    let mut detection_count = 0;

    for table in &schema.tables {
        for column in &table.columns {
            let col_lower = column.name.to_lowercase();
            for (words, description) in &keywords {
                if words.iter().any(|w| col_lower.contains(w.as_str())) {
                    warn!(
                        "Sensitive data detected: table '{}' column '{}' - {}",
                        table.name, column.name, description
                    );
                    detection_count += 1;
                    break; // One warning per column is enough
                }
            }
        }
    }

    if detection_count > 0 {
        info!(
            "Detected {} potentially sensitive columns. \
             Use the postprocessor with --redact to sanitize sampled data.",
            detection_count
        );
    }

    detection_count
}

/// Lists supported database types and their connection string formats.
pub fn list_supported_databases() {
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
    println!("  - Read-only database operations");
    println!("  - Credential sanitization in logs");
    println!("  - Optional AES-GCM encryption");
    println!("  - Offline operation after connection");
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tests::{make_column, make_table, make_test_schema};
    use dbsurveyor_core::adapters::SensitivePattern;

    #[test]
    fn test_parse_quality_thresholds_valid() {
        let thresholds = vec![
            "completeness:0.9".to_string(),
            "uniqueness:0.95".to_string(),
            "consistency:0.85".to_string(),
        ];
        let (c, u, co) = parse_quality_thresholds(&thresholds);
        assert_eq!(c, Some(0.9));
        assert_eq!(u, Some(0.95));
        assert_eq!(co, Some(0.85));
    }

    #[test]
    fn test_parse_quality_thresholds_empty() {
        let thresholds: Vec<String> = Vec::new();
        let (c, u, co) = parse_quality_thresholds(&thresholds);
        assert_eq!(c, None);
        assert_eq!(u, None);
        assert_eq!(co, None);
    }

    #[test]
    fn test_parse_quality_thresholds_invalid_value() {
        let thresholds = vec!["completeness:invalid".to_string()];
        let (c, _, _) = parse_quality_thresholds(&thresholds);
        assert_eq!(c, None);
    }

    #[test]
    fn test_parse_quality_thresholds_clamped() {
        let thresholds = vec!["completeness:1.5".to_string()];
        let (c, _, _) = parse_quality_thresholds(&thresholds);
        assert_eq!(c, Some(1.0)); // Clamped to max
    }

    #[test]
    fn test_warn_sensitive_columns_detects_password() {
        let schema = make_test_schema(vec![make_table(
            "users",
            vec![
                make_column("id"),
                make_column("password_hash"),
                make_column("email"),
            ],
        )]);

        let config = SamplingConfig::new();
        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 2, "should detect password_hash and email");
    }

    #[test]
    fn test_warn_sensitive_columns_disabled() {
        let schema = make_test_schema(vec![make_table(
            "users",
            vec![make_column("password"), make_column("ssn")],
        )]);

        let config = SamplingConfig::new().with_sensitive_warnings(false);
        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 0, "should skip detection when disabled");
    }

    #[test]
    fn test_warn_sensitive_columns_empty_patterns() {
        let schema = make_test_schema(vec![make_table(
            "users",
            vec![make_column("password")],
        )]);

        let mut config = SamplingConfig::new();
        config.sensitive_detection_patterns = Vec::new();
        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 0, "should detect nothing with empty patterns");
    }

    #[test]
    fn test_warn_sensitive_columns_no_matches() {
        let schema = make_test_schema(vec![make_table(
            "products",
            vec![
                make_column("id"),
                make_column("name"),
                make_column("price"),
            ],
        )]);

        let config = SamplingConfig::new();
        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 0, "should detect nothing in non-sensitive columns");
    }

    #[test]
    fn test_warn_sensitive_columns_custom_pattern() {
        let schema = make_test_schema(vec![make_table(
            "tokens",
            vec![make_column("api_key"), make_column("name")],
        )]);

        let config = SamplingConfig::new().add_sensitive_pattern(SensitivePattern::new(
            "(?i)(api_key|secret_key)",
            "API key detected",
        ));

        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 1, "should detect api_key column");
    }

    #[test]
    fn test_warn_sensitive_columns_case_insensitive() {
        let schema = make_test_schema(vec![make_table(
            "users",
            vec![
                make_column("PASSWORD"),
                make_column("Email_Address"),
                make_column("SSN"),
            ],
        )]);

        let config = SamplingConfig::new();
        let count = warn_sensitive_columns(&schema, &config);
        assert_eq!(count, 3, "should detect PASSWORD, Email_Address, and SSN");
    }

    #[test]
    fn test_sampling_config_from_cli_defaults() {
        let config = SamplingConfig::new().with_sample_size(100);
        assert_eq!(config.sample_size, 100);
        assert_eq!(config.throttle_ms, None);
    }

    #[test]
    fn test_sampling_config_from_cli_with_throttle() {
        let config = SamplingConfig::new()
            .with_sample_size(50)
            .with_throttle_ms(200);
        assert_eq!(config.sample_size, 50);
        assert_eq!(config.throttle_ms, Some(200));
    }
}
