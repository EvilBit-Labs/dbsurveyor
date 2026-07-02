//! Schema output serialization (JSON, compressed, encrypted).
//!
//! All writers are atomic: data is written to a temporary file in the target
//! directory and renamed into place, so an interrupted run never leaves a
//! truncated or corrupt output file. When `--compress` or `--encrypt` is
//! given, the output path is normalized to carry the matching extension
//! (`.zst` or `.enc`) so the postprocessor's extension-based format
//! detection can load the file.

use crate::Cli;
use dbsurveyor_core::Result;
use dbsurveyor_core::error::DbSurveyorError;
use std::path::{Path, PathBuf};
use tracing::info;

/// Environment variable consulted for a non-interactive encryption password.
#[cfg(feature = "encryption")]
const PASSWORD_ENV_VAR: &str = "DBSURVEYOR_ENCRYPTION_PASSWORD";

/// Saves schema to file with optional compression and encryption.
///
/// Returns the path the schema was actually written to, which may differ
/// from `output_path` when an extension is appended for the selected format.
pub(crate) async fn save_schema(
    schema: &dbsurveyor_core::models::DatabaseSchema,
    output_path: &Path,
    cli: &Cli,
) -> Result<PathBuf> {
    let json_value = serde_json::to_value(schema)
        .map_err(|e| DbSurveyorError::collection_failed("JSON serialization", e))?;
    save_json_value(&json_value, output_path, cli, true).await
}

/// Saves a multi-database server schema to file.
///
/// Returns the path the schema was actually written to, which may differ
/// from `output_path` when an extension is appended for the selected format.
pub(crate) async fn save_server_schema(
    schema: &dbsurveyor_core::models::DatabaseServerSchema,
    output_path: &Path,
    cli: &Cli,
) -> Result<PathBuf> {
    let json_value = serde_json::to_value(schema)
        .map_err(|e| DbSurveyorError::collection_failed("JSON serialization", e))?;
    save_json_value(&json_value, output_path, cli, false).await
}

async fn save_json_value(
    json_value: &serde_json::Value,
    output_path: &Path,
    cli: &Cli,
    validate_schema: bool,
) -> Result<PathBuf> {
    // Convert to Value for validation
    if validate_schema {
        dbsurveyor_core::validate_schema_output(json_value)
            .map_err(|e| DbSurveyorError::collection_failed("Schema validation failed", e))?;

        info!("[OK]Output validation passed");
    } else {
        info!("[OK]Output serialization prepared for multi-database schema");
    }

    let output_path = effective_output_path(output_path, cli.compress, cli.encrypt);

    if cli.encrypt && cli.compress {
        #[cfg(all(feature = "encryption", feature = "compression"))]
        {
            let json_data = to_pretty_json(json_value)?;
            let compressed = compress_bytes(json_data.into_bytes()).await?;
            save_encrypted(compressed, &output_path).await?;
        }
        #[cfg(not(all(feature = "encryption", feature = "compression")))]
        {
            return Err(DbSurveyorError::configuration(
                "Combined compression and encryption not available. Compile with --features compression,encryption",
            ));
        }
    } else if cli.encrypt {
        #[cfg(feature = "encryption")]
        {
            let json_data = to_pretty_json(json_value)?;
            save_encrypted(json_data.into_bytes(), &output_path).await?;
        }
        #[cfg(not(feature = "encryption"))]
        {
            return Err(DbSurveyorError::configuration(
                "Encryption not available. Compile with --features encryption",
            ));
        }
    } else if cli.compress {
        #[cfg(feature = "compression")]
        {
            let json_data = to_pretty_json(json_value)?;
            let compressed = compress_bytes(json_data.into_bytes()).await?;
            write_atomic(&output_path, compressed).await?;
        }
        #[cfg(not(feature = "compression"))]
        {
            return Err(DbSurveyorError::configuration(
                "Compression not available. Compile with --features compression",
            ));
        }
    } else {
        save_json_streaming(json_value, &output_path)?;
    }

    Ok(output_path)
}

/// Resolves the actual output path for the selected format.
///
/// Appends `.enc` (encrypted, including combined compressed+encrypted
/// output) or `.zst` (compressed) when the configured path does not already
/// end with that extension. The postprocessor detects the file format from
/// the final extension, so writing compressed or encrypted bytes to a
/// `.json`-named file would produce an unloadable output.
fn effective_output_path(output_path: &Path, compress: bool, encrypt: bool) -> PathBuf {
    let target_ext = if encrypt {
        "enc"
    } else if compress {
        "zst"
    } else {
        return output_path.to_path_buf();
    };

    let already_matches = output_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == target_ext);
    if already_matches {
        return output_path.to_path_buf();
    }

    let mut adjusted = output_path.as_os_str().to_os_string();
    adjusted.push(".");
    adjusted.push(target_ext);
    let adjusted = PathBuf::from(adjusted);
    info!(
        "Output path adjusted to {} to match the selected output format",
        adjusted.display()
    );
    adjusted
}

/// Serializes a JSON value to a pretty-printed string.
#[cfg(any(feature = "encryption", feature = "compression"))]
fn to_pretty_json(json_value: &serde_json::Value) -> Result<String> {
    serde_json::to_string_pretty(json_value)
        .map_err(|e| DbSurveyorError::collection_failed("JSON formatting", e))
}

/// Returns the directory a file will be created in, defaulting to the
/// current directory for bare file names.
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// Streams JSON data to the output file via `BufWriter`, avoiding an
/// intermediate `String`. The write is atomic: data is streamed into a
/// temporary file in the target directory and renamed into place.
fn save_json_streaming(json_value: &serde_json::Value, output_path: &Path) -> Result<()> {
    use std::io::Write;

    let tmp = tempfile::NamedTempFile::new_in(parent_dir(output_path)).map_err(|e| {
        DbSurveyorError::Io {
            context: format!(
                "Failed to create temporary file for {}",
                output_path.display()
            ),
            source: e,
        }
    })?;

    let mut writer = std::io::BufWriter::new(tmp.as_file());
    serde_json::to_writer_pretty(&mut writer, json_value)
        .map_err(|e| DbSurveyorError::collection_failed("JSON streaming write", e))?;
    writer.flush().map_err(|e| DbSurveyorError::Io {
        context: format!("Failed to flush {}", output_path.display()),
        source: e,
    })?;
    drop(writer);

    persist_temp_file(tmp, output_path)
}

/// Writes bytes to `output_path` atomically via a temporary file in the
/// same directory. Runs on the blocking thread pool.
#[cfg(any(feature = "encryption", feature = "compression"))]
async fn write_atomic(output_path: &Path, data: Vec<u8>) -> Result<()> {
    let path = output_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Write;

        let mut tmp = tempfile::NamedTempFile::new_in(parent_dir(&path)).map_err(|e| {
            DbSurveyorError::Io {
                context: format!("Failed to create temporary file for {}", path.display()),
                source: e,
            }
        })?;
        tmp.write_all(&data).map_err(|e| DbSurveyorError::Io {
            context: format!("Failed to write {}", path.display()),
            source: e,
        })?;
        persist_temp_file(tmp, &path)
    })
    .await
    .map_err(|e| DbSurveyorError::collection_failed("Atomic write task failed", e))?
}

/// Syncs a temporary file to disk and renames it over the target path.
fn persist_temp_file(tmp: tempfile::NamedTempFile, output_path: &Path) -> Result<()> {
    tmp.as_file().sync_all().map_err(|e| DbSurveyorError::Io {
        context: format!("Failed to sync {}", output_path.display()),
        source: e,
    })?;
    tmp.persist(output_path).map_err(|e| DbSurveyorError::Io {
        context: format!("Failed to persist {}", output_path.display()),
        source: e.error,
    })?;
    Ok(())
}

/// Compresses bytes with Zstandard on the blocking thread pool.
#[cfg(feature = "compression")]
async fn compress_bytes(data: Vec<u8>) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        use std::io::Write;
        let mut encoder = zstd::Encoder::new(Vec::new(), 3)?;
        encoder.write_all(&data)?;
        encoder.finish()
    })
    .await
    .map_err(|e| DbSurveyorError::collection_failed("Compression task failed", e))?
    .map_err(|e| DbSurveyorError::Io {
        context: "Compression failed".to_string(),
        source: e,
    })
}

/// Encrypts the payload and writes it to the output file atomically.
///
/// The payload is either pretty-printed JSON or (for combined mode)
/// zstd-compressed JSON; the postprocessor detects compression inside the
/// decrypted payload via the zstd frame magic.
#[cfg(feature = "encryption")]
async fn save_encrypted(payload: Vec<u8>, output_path: &Path) -> Result<()> {
    use dbsurveyor_core::security::encryption::encrypt_data_async;

    let password = obtain_encryption_password()?;
    let encrypted = encrypt_data_async(&payload, &password).await?;
    let encrypted_json = serde_json::to_string_pretty(&encrypted)
        .map_err(|e| DbSurveyorError::collection_failed("Encryption serialization", e))?;

    write_atomic(output_path, encrypted_json.into_bytes()).await
}

/// Obtains the encryption password from `DBSURVEYOR_ENCRYPTION_PASSWORD`
/// or interactively (with confirmation) when the variable is not set.
#[cfg(feature = "encryption")]
fn obtain_encryption_password() -> Result<String> {
    if let Ok(password) = std::env::var(PASSWORD_ENV_VAR) {
        validate_password(&password)?;
        return Ok(password);
    }

    let password = prompt_password("Enter encryption password: ")?;
    validate_password(&password)?;

    // Confirm password to prevent typos
    let password_confirm = prompt_password("Confirm encryption password: ")?;
    if password != password_confirm {
        return Err(DbSurveyorError::configuration("Passwords do not match"));
    }

    Ok(password)
}

/// Reads a password from the terminal without echoing it.
#[cfg(feature = "encryption")]
fn prompt_password(prompt: &str) -> Result<String> {
    use std::io::{self, Write};

    print!("{prompt}");
    io::stdout().flush().map_err(|e| {
        DbSurveyorError::configuration(format!(
            "Failed to flush stdout before reading password: {}",
            e
        ))
    })?;
    rpassword::read_password()
        .map_err(|e| DbSurveyorError::configuration(format!("Failed to read password: {}", e)))
}

/// Enforces minimum password requirements for encrypted outputs.
#[cfg(feature = "encryption")]
fn validate_password(password: &str) -> Result<()> {
    if password.is_empty() {
        return Err(DbSurveyorError::configuration("Password cannot be empty"));
    }

    if password.len() < 8 {
        return Err(DbSurveyorError::configuration(
            "Encryption password must be at least 8 characters",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_path_unchanged_for_plain_json() {
        let path = Path::new("schema.dbsurveyor.json");
        assert_eq!(
            effective_output_path(path, false, false),
            PathBuf::from("schema.dbsurveyor.json")
        );
    }

    #[test]
    fn effective_path_appends_zst_for_compression() {
        let path = Path::new("schema.dbsurveyor.json");
        assert_eq!(
            effective_output_path(path, true, false),
            PathBuf::from("schema.dbsurveyor.json.zst")
        );
    }

    #[test]
    fn effective_path_keeps_existing_zst_extension() {
        let path = Path::new("schema.dbsurveyor.json.zst");
        assert_eq!(
            effective_output_path(path, true, false),
            PathBuf::from("schema.dbsurveyor.json.zst")
        );
    }

    #[test]
    fn effective_path_appends_enc_for_encryption() {
        let path = Path::new("schema.dbsurveyor.json");
        assert_eq!(
            effective_output_path(path, false, true),
            PathBuf::from("schema.dbsurveyor.json.enc")
        );
    }

    #[test]
    fn effective_path_uses_enc_for_combined_output() {
        let path = Path::new("schema.dbsurveyor.json");
        assert_eq!(
            effective_output_path(path, true, true),
            PathBuf::from("schema.dbsurveyor.json.enc")
        );
    }

    #[test]
    fn effective_path_keeps_existing_enc_extension() {
        let path = Path::new("results.enc");
        assert_eq!(
            effective_output_path(path, false, true),
            PathBuf::from("results.enc")
        );
    }

    #[test]
    fn save_json_streaming_writes_atomically_and_overwrites() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("schema.json");

        let first = serde_json::json!({"value": 1});
        save_json_streaming(&first, &path).expect("first write failed");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("failed to read output"))
                .expect("output is not valid JSON");
        assert_eq!(written, first);

        let second = serde_json::json!({"value": 2});
        save_json_streaming(&second, &path).expect("overwrite failed");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("failed to read output"))
                .expect("output is not valid JSON");
        assert_eq!(written, second);

        // No stray temporary files may remain after successful writes.
        let entries = std::fs::read_dir(dir.path())
            .expect("failed to list temp dir")
            .count();
        assert_eq!(entries, 1, "temporary files leaked into output directory");
    }

    #[cfg(feature = "compression")]
    #[tokio::test]
    async fn compress_bytes_round_trips_through_zstd() {
        let input = br#"{"format_version":"1.0"}"#.to_vec();
        let compressed = compress_bytes(input.clone())
            .await
            .expect("compression failed");
        let decompressed = zstd::decode_all(compressed.as_slice()).expect("decompression failed");
        assert_eq!(decompressed, input);
    }

    #[cfg(feature = "compression")]
    #[tokio::test]
    async fn write_atomic_overwrites_existing_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("schema.json.zst");

        write_atomic(&path, b"first".to_vec())
            .await
            .expect("first write failed");
        write_atomic(&path, b"second".to_vec())
            .await
            .expect("overwrite failed");

        let contents = std::fs::read(&path).expect("failed to read output");
        assert_eq!(contents, b"second");
    }

    #[cfg(all(feature = "compression", feature = "encryption"))]
    #[tokio::test]
    async fn combined_payload_round_trips_through_compress_and_encrypt() {
        use dbsurveyor_core::security::encryption::{decrypt_data_async, encrypt_data_async};

        let json = br#"{"format_version":"1.0","tables":[]}"#.to_vec();
        let compressed = compress_bytes(json.clone())
            .await
            .expect("compression failed");
        let encrypted = encrypt_data_async(&compressed, "test-password-123")
            .await
            .expect("encryption failed");

        let decrypted = decrypt_data_async(encrypted, "test-password-123")
            .await
            .expect("decryption failed");
        // Combined payloads must be detectable via the zstd frame magic.
        assert!(decrypted.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]));
        let decompressed = zstd::decode_all(decrypted.as_slice()).expect("decompression failed");
        assert_eq!(decompressed, json);
    }
}
