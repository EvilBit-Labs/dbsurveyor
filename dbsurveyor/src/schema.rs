//! Schema file loading with support for JSON, compressed, and encrypted formats.

use crate::create_spinner;
use dbsurveyor_core::{Result, models::DatabaseSchema};
use std::path::PathBuf;
use tracing::info;

/// Environment variable consulted for a non-interactive decryption password.
#[cfg(feature = "encryption")]
const PASSWORD_ENV_VAR: &str = "DBSURVEYOR_ENCRYPTION_PASSWORD";

/// Zstandard frame magic number, used to detect compressed payloads inside
/// encrypted files (combined `--compress --encrypt` collector output).
#[cfg(feature = "encryption")]
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Loads schema from file with support for different formats.
pub(crate) async fn load_schema(input_path: &PathBuf) -> Result<DatabaseSchema> {
    let spinner = create_spinner("Loading schema...");
    let result = load_schema_inner(input_path, &spinner).await;
    spinner.finish_and_clear();
    result
}

/// Inner implementation for schema loading, separated to guarantee spinner cleanup.
async fn load_schema_inner(
    input_path: &PathBuf,
    spinner: &indicatif::ProgressBar,
) -> Result<DatabaseSchema> {
    info!("Loading schema from {}", input_path.display());

    let file_content = tokio::fs::read(input_path).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to read {}", input_path.display()),
            source: e,
        }
    })?;

    // Detect file format based on extension and content
    let extension = input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    match extension {
        "enc" => {
            spinner.set_message("Decrypting...");
            #[cfg(feature = "encryption")]
            {
                load_encrypted_schema(&file_content).await
            }
            #[cfg(not(feature = "encryption"))]
            {
                Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                    "Encryption support not available. Compile with --features encryption",
                ))
            }
        }
        "zst" => {
            spinner.set_message("Decompressing...");
            #[cfg(feature = "compression")]
            {
                load_compressed_schema(&file_content).await
            }
            #[cfg(not(feature = "compression"))]
            {
                Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                    "Compression support not available. Compile with --features compression",
                ))
            }
        }
        _ => {
            spinner.set_message("Parsing JSON...");
            load_json_schema(&file_content).await
        }
    }
}

/// Loads JSON schema from bytes.
async fn load_json_schema(data: &[u8]) -> Result<DatabaseSchema> {
    let json_str = std::str::from_utf8(data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in schema file: {}",
            e
        ))
    })?;

    // Use the validation function that combines parsing, validation, and deserialization
    dbsurveyor_core::validate_and_parse_schema(json_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Schema validation failed: {}",
            e
        ))
    })
}

/// Decompresses a Zstandard payload to a UTF-8 string on the blocking
/// thread pool.
#[cfg(feature = "compression")]
async fn decompress_zstd(data: &[u8]) -> Result<String> {
    let owned_data = data.to_vec();
    tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        use std::io::Read;
        let mut decoder = zstd::Decoder::new(owned_data.as_slice())?;
        let mut buf = String::new();
        decoder.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await
    .map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Decompression task failed: {}",
            e
        ))
    })?
    .map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Decompression failed: {}",
            e
        ))
    })
}

/// Loads compressed schema.
#[cfg(feature = "compression")]
async fn load_compressed_schema(data: &[u8]) -> Result<DatabaseSchema> {
    let decompressed = decompress_zstd(data).await?;

    dbsurveyor_core::validate_and_parse_schema(&decompressed).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Decompressed schema validation failed: {}",
            e
        ))
    })
}

/// Loads encrypted schema.
#[cfg(feature = "encryption")]
async fn load_encrypted_schema(data: &[u8]) -> Result<DatabaseSchema> {
    use dbsurveyor_core::security::encryption::{EncryptedData, decrypt_data_async};
    use std::io::{self, Write};

    let json_str = std::str::from_utf8(data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in encrypted file: {}",
            e
        ))
    })?;

    let encrypted: EncryptedData = serde_json::from_str(json_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Serialization {
            context: "Failed to parse encrypted data structure".to_string(),
            source: e,
        }
    })?;

    // Get password from the environment or prompt the user
    let password = if let Ok(password) = std::env::var(PASSWORD_ENV_VAR) {
        password
    } else {
        print!("Enter decryption password: ");
        io::stdout().flush().map_err(|e| {
            dbsurveyor_core::error::DbSurveyorError::configuration(format!(
                "Failed to flush stdout before reading password: {}",
                e
            ))
        })?;
        rpassword::read_password().map_err(|e| {
            dbsurveyor_core::error::DbSurveyorError::configuration(format!(
                "Failed to read password: {}",
                e
            ))
        })?
    };

    let decrypted_data = decrypt_data_async(encrypted, &password).await?;

    // Combined collector output (--compress --encrypt) compresses the JSON
    // before encrypting it; detect the zstd frame magic and decompress.
    if decrypted_data.starts_with(&ZSTD_MAGIC) {
        #[cfg(feature = "compression")]
        {
            let decompressed = decompress_zstd(&decrypted_data).await?;
            return dbsurveyor_core::validate_and_parse_schema(&decompressed).map_err(|e| {
                dbsurveyor_core::error::DbSurveyorError::configuration(format!(
                    "Decrypted schema validation failed: {}",
                    e
                ))
            });
        }
        #[cfg(not(feature = "compression"))]
        {
            return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Encrypted payload is zstd-compressed. Compile with --features compression",
            ));
        }
    }

    let decrypted_str = std::str::from_utf8(&decrypted_data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in decrypted data: {}",
            e
        ))
    })?;

    dbsurveyor_core::validate_and_parse_schema(decrypted_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Decrypted schema validation failed: {}",
            e
        ))
    })
}
