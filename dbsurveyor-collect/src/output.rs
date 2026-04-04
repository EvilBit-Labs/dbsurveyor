//! Schema output serialization (JSON, compressed, encrypted).

use crate::Cli;
use dbsurveyor_core::Result;
use std::path::PathBuf;
use tracing::info;

/// Saves schema to file with optional compression and encryption.
pub(crate) async fn save_schema(
    schema: &dbsurveyor_core::models::DatabaseSchema,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<()> {
    // Convert to Value for validation
    let json_value = serde_json::to_value(schema).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON serialization", e)
    })?;

    dbsurveyor_core::validate_schema_output(&json_value).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("Schema validation failed", e)
    })?;

    info!("[OK]Output validation passed");

    if cli.encrypt {
        #[cfg(feature = "encryption")]
        {
            let json_data = serde_json::to_string_pretty(&json_value).map_err(|e| {
                dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON formatting", e)
            })?;
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
            let json_data = serde_json::to_string_pretty(&json_value).map_err(|e| {
                dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON formatting", e)
            })?;
            save_compressed(&json_data, output_path).await
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Compression not available. Compile with --features compression",
            ))
        }
    } else {
        save_json_streaming(&json_value, output_path)
    }
}

/// Streams JSON data directly to file via `BufWriter`, avoiding an intermediate `String`.
fn save_json_streaming(json_value: &serde_json::Value, output_path: &PathBuf) -> Result<()> {
    let file = std::fs::File::create(output_path).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to create {}", output_path.display()),
            source: e,
        }
    })?;
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(writer, json_value).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON streaming write", e)
    })?;
    Ok(())
}

/// Saves compressed JSON data.
#[cfg(feature = "compression")]
async fn save_compressed(json_data: &str, output_path: &PathBuf) -> Result<()> {
    let json_bytes = json_data.as_bytes().to_vec();
    let compressed_data = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        use std::io::Write;
        let mut encoder = zstd::Encoder::new(Vec::new(), 3)?;
        encoder.write_all(&json_bytes)?;
        encoder.finish()
    })
    .await
    .map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Compression task failed: {}",
            e
        ))
    })?
    .map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!("Compression failed: {}", e))
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
    use dbsurveyor_core::security::encryption::encrypt_data_async;
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

    if password.len() < 8 {
        return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
            "Encryption password must be at least 8 characters",
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

    let encrypted = encrypt_data_async(json_data.as_bytes(), &password).await?;
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
