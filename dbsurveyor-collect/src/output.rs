//! File output operations for schema collection results.
//!
//! Handles writing schema data to files with optional compression and encryption.

use dbsurveyor_core::Result;
use std::path::PathBuf;

use crate::Cli;

/// Saves schema to file with optional compression and encryption.
pub async fn save_schema(
    schema: &dbsurveyor_core::models::DatabaseSchema,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<()> {
    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(schema).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON serialization", e)
    })?;

    // Validate output against JSON Schema before saving
    let json_value: serde_json::Value = serde_json::from_str(&json_data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed(
            "JSON parsing for validation",
            e,
        )
    })?;

    dbsurveyor_core::validate_schema_output(&json_value).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("Schema validation failed", e)
    })?;

    tracing::info!("Output validation passed");

    if cli.encrypt {
        #[cfg(feature = "encryption")]
        {
            save_encrypted(&json_data, output_path).await
        }
        #[cfg(not(feature = "encryption"))]
        {
            Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Encryption not available. Compile with --features encryption",
            ))
        }
    } else if cli.compress {
        #[cfg(feature = "compression")]
        {
            save_compressed(&json_data, output_path).await
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Compression not available. Compile with --features compression",
            ))
        }
    } else {
        save_json(&json_data, output_path).await
    }
}

/// Saves JSON data to file.
pub async fn save_json(json_data: &str, output_path: &PathBuf) -> Result<()> {
    tokio::fs::write(output_path, json_data)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write to {}", output_path.display()),
            source: e,
        })?;
    Ok(())
}

/// Saves compressed JSON data.
#[cfg(feature = "compression")]
async fn save_compressed(json_data: &str, output_path: &PathBuf) -> Result<()> {
    use std::io::Write;

    let mut encoder = zstd::Encoder::new(Vec::new(), 3).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to create compressor: {}",
            e
        ))
    })?;

    encoder.write_all(json_data.as_bytes()).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Compression failed: {}",
            e
        ))
    })?;

    let compressed_data = encoder.finish().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Compression finalization failed: {}",
            e
        ))
    })?;

    tokio::fs::write(output_path, compressed_data)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!(
                "Failed to write compressed file to {}",
                output_path.display()
            ),
            source: e,
        })?;

    Ok(())
}

/// Saves encrypted JSON data.
#[cfg(feature = "encryption")]
async fn save_encrypted(json_data: &str, output_path: &PathBuf) -> Result<()> {
    use dbsurveyor_core::security::encryption::encrypt_data;
    use std::io::{self, Write};

    // Get password from user
    print!("Enter encryption password: ");
    io::stdout().flush().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to flush stdout before reading password: {}",
            e
        ))
    })?;
    let password = rpassword::read_password().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to read password: {}",
            e
        ))
    })?;

    if password.is_empty() {
        return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
            "Password cannot be empty",
        ));
    }

    // Confirm password to prevent typos
    print!("Confirm encryption password: ");
    io::stdout().flush().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to flush stdout before reading password confirmation: {}",
            e
        ))
    })?;
    let password_confirm = rpassword::read_password().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to read password confirmation: {}",
            e
        ))
    })?;

    if password != password_confirm {
        return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
            "Passwords do not match",
        ));
    }

    let encrypted = encrypt_data(json_data.as_bytes(), &password)?;
    let encrypted_json = serde_json::to_string_pretty(&encrypted).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("Encryption serialization", e)
    })?;

    tokio::fs::write(output_path, encrypted_json)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!(
                "Failed to write encrypted file to {}",
                output_path.display()
            ),
            source: e,
        })?;

    Ok(())
}
