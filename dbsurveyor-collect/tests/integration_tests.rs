//! End-to-end output writer tests for the collector binary.
//!
//! These tests run the compiled `dbsurveyor-collect` binary against a
//! temporary SQLite database and verify that the JSON, compressed,
//! encrypted, and combined (compressed + encrypted) output formats
//! round-trip losslessly, including automatic output extension handling.
//!
//! NOTE: Adapter integration tests live in dbsurveyor-core, which owns the
//! adapter implementations.

#![cfg(feature = "sqlite")]

use std::path::Path;
use std::process::Command;

/// Password used for encrypted output tests, provided via the
/// `DBSURVEYOR_ENCRYPTION_PASSWORD` environment variable.
#[cfg(feature = "encryption")]
const TEST_PASSWORD: &str = "integration-test-password";

/// Returns the path to the compiled `dbsurveyor-collect` binary.
fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_dbsurveyor-collect")
}

/// Creates a SQLite database with a small table and returns its URL.
async fn create_test_database(dir: &Path) -> String {
    use sqlx::sqlite::SqliteConnectOptions;

    let db_path = dir.join("test.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .expect("failed to create sqlite database");
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("failed to create users table");
    pool.close().await;

    format!("sqlite://{}", db_path.display())
}

/// Runs the collector binary with the given arguments and environment.
fn run_collector(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(bin_path());
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("failed to execute dbsurveyor-collect")
}

/// Asserts that a collector run succeeded, printing its output on failure.
fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "collector failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Asserts that collected schema JSON is versioned and contains the test table.
fn assert_schema_json(json: &serde_json::Value) {
    assert_eq!(
        json["format_version"], "1.0",
        "schema must carry the format version"
    );
    let tables = json["tables"].as_array().expect("tables must be an array");
    assert!(
        tables.iter().any(|table| table["name"] == "users"),
        "collected schema should contain the users table"
    );
}

#[tokio::test]
async fn plain_json_output_round_trips() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let url = create_test_database(dir.path()).await;
    let out = dir.path().join("schema.dbsurveyor.json");

    let output = run_collector(
        &[
            "--database-url",
            &url,
            "--output",
            out.to_str().expect("non-UTF8 path"),
        ],
        &[],
    );
    assert_success(&output);

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).expect("output file missing"))
            .expect("output is not valid JSON");
    assert_schema_json(&json);
}

#[cfg(feature = "compression")]
#[tokio::test]
async fn compressed_output_appends_extension_and_round_trips() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let url = create_test_database(dir.path()).await;
    let out = dir.path().join("schema.dbsurveyor.json");

    let output = run_collector(
        &[
            "--database-url",
            &url,
            "--output",
            out.to_str().expect("non-UTF8 path"),
            "--compress",
        ],
        &[],
    );
    assert_success(&output);

    // The writer must normalize the path so the extension matches the format.
    let compressed_path = dir.path().join("schema.dbsurveyor.json.zst");
    assert!(
        compressed_path.exists(),
        "compressed output should be written with a .zst extension"
    );
    assert!(
        !out.exists(),
        "unsuffixed output file should not be written"
    );

    let compressed = std::fs::read(&compressed_path).expect("compressed output missing");
    let decompressed = zstd::decode_all(compressed.as_slice()).expect("zstd decode failed");
    let json: serde_json::Value =
        serde_json::from_slice(&decompressed).expect("decompressed output is not valid JSON");
    assert_schema_json(&json);
}

#[cfg(feature = "encryption")]
#[tokio::test]
async fn encrypted_output_round_trips() {
    use dbsurveyor_core::security::encryption::{EncryptedData, decrypt_data};

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let url = create_test_database(dir.path()).await;
    let out = dir.path().join("schema.dbsurveyor.json");

    let output = run_collector(
        &[
            "--database-url",
            &url,
            "--output",
            out.to_str().expect("non-UTF8 path"),
            "--encrypt",
        ],
        &[("DBSURVEYOR_ENCRYPTION_PASSWORD", TEST_PASSWORD)],
    );
    assert_success(&output);

    let encrypted_path = dir.path().join("schema.dbsurveyor.json.enc");
    assert!(
        encrypted_path.exists(),
        "encrypted output should be written with a .enc extension"
    );

    let encrypted: EncryptedData =
        serde_json::from_slice(&std::fs::read(&encrypted_path).expect("encrypted output missing"))
            .expect("encrypted output is not a valid EncryptedData document");
    let decrypted = decrypt_data(&encrypted, TEST_PASSWORD).expect("decryption failed");
    let json: serde_json::Value =
        serde_json::from_slice(&decrypted).expect("decrypted output is not valid JSON");
    assert_schema_json(&json);
}

#[cfg(feature = "encryption")]
#[tokio::test]
async fn encrypted_output_rejects_short_password() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let url = create_test_database(dir.path()).await;
    let out = dir.path().join("schema.dbsurveyor.json");

    let output = run_collector(
        &[
            "--database-url",
            &url,
            "--output",
            out.to_str().expect("non-UTF8 path"),
            "--encrypt",
        ],
        &[("DBSURVEYOR_ENCRYPTION_PASSWORD", "short")],
    );
    assert!(
        !output.status.success(),
        "collector must reject passwords shorter than 8 characters"
    );
    assert!(
        !dir.path().join("schema.dbsurveyor.json.enc").exists(),
        "no output file may be written when the password is rejected"
    );
}

#[cfg(all(feature = "compression", feature = "encryption"))]
#[tokio::test]
async fn combined_compressed_encrypted_output_round_trips() {
    use dbsurveyor_core::security::encryption::{EncryptedData, decrypt_data};

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let url = create_test_database(dir.path()).await;
    let out = dir.path().join("schema.dbsurveyor.json");

    let output = run_collector(
        &[
            "--database-url",
            &url,
            "--output",
            out.to_str().expect("non-UTF8 path"),
            "--compress",
            "--encrypt",
        ],
        &[("DBSURVEYOR_ENCRYPTION_PASSWORD", TEST_PASSWORD)],
    );
    assert_success(&output);

    // Combined output is encrypted on the outside, so it uses .enc.
    let encrypted_path = dir.path().join("schema.dbsurveyor.json.enc");
    assert!(
        encrypted_path.exists(),
        "combined output should be written with a .enc extension"
    );

    let encrypted: EncryptedData =
        serde_json::from_slice(&std::fs::read(&encrypted_path).expect("combined output missing"))
            .expect("combined output is not a valid EncryptedData document");
    let decrypted = decrypt_data(&encrypted, TEST_PASSWORD).expect("decryption failed");
    assert!(
        decrypted.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]),
        "decrypted payload should be a zstd frame"
    );
    let decompressed = zstd::decode_all(decrypted.as_slice()).expect("zstd decode failed");
    let json: serde_json::Value =
        serde_json::from_slice(&decompressed).expect("decompressed output is not valid JSON");
    assert_schema_json(&json);
}
