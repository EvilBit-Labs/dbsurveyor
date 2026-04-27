//! Unit-level integration tests for the `dbsurveyor` postprocessor.
//!
//! These tests verify schema loading, validation, and analysis behavior
//! by writing temporary schema files and invoking the binary.

use std::io::Write;
use std::process::Command;

/// Returns the path to the compiled `dbsurveyor` binary.
fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_dbsurveyor")
}

/// Returns a minimal valid schema JSON string.
fn minimal_valid_schema() -> &'static str {
    r#"{
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [],
        "views": [],
        "indexes": [],
        "constraints": [],
        "procedures": [],
        "functions": [],
        "triggers": [],
        "custom_types": [],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0",
            "warnings": []
        }
    }"#
}

#[test]
fn test_validate_valid_schema_file() {
    let mut tmp =
        tempfile::NamedTempFile::with_suffix(".json").expect("failed to create temp file");
    tmp.write_all(minimal_valid_schema().as_bytes())
        .expect("failed to write schema");
    tmp.flush().expect("failed to flush");

    let output = Command::new(bin_path())
        .args(["validate", tmp.path().to_str().expect("non-UTF8 path")])
        .output()
        .expect("failed to execute dbsurveyor validate");

    assert!(
        output.status.success(),
        "validate should succeed for a valid schema file: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[OK]"),
        "validate output should indicate success"
    );
    assert!(
        stdout.contains("test_db"),
        "validate output should show database name"
    );
}

#[test]
fn test_validate_invalid_json_file() {
    let mut tmp =
        tempfile::NamedTempFile::with_suffix(".json").expect("failed to create temp file");
    tmp.write_all(b"{ invalid json }").expect("failed to write");
    tmp.flush().expect("failed to flush");

    let output = Command::new(bin_path())
        .args(["validate", tmp.path().to_str().expect("non-UTF8 path")])
        .output()
        .expect("failed to execute dbsurveyor validate");

    assert!(
        !output.status.success(),
        "validate should fail for invalid JSON"
    );
}

#[test]
fn test_validate_incomplete_schema() {
    let mut tmp =
        tempfile::NamedTempFile::with_suffix(".json").expect("failed to create temp file");
    // Valid JSON but missing required fields
    tmp.write_all(br#"{"format_version": "1.0"}"#)
        .expect("failed to write");
    tmp.flush().expect("failed to flush");

    let output = Command::new(bin_path())
        .args(["validate", tmp.path().to_str().expect("non-UTF8 path")])
        .output()
        .expect("failed to execute dbsurveyor validate");

    assert!(
        !output.status.success(),
        "validate should fail for incomplete schema"
    );
}

#[cfg(feature = "experimental")]
#[test]
fn test_analyze_valid_schema() {
    let mut tmp =
        tempfile::NamedTempFile::with_suffix(".json").expect("failed to create temp file");
    tmp.write_all(minimal_valid_schema().as_bytes())
        .expect("failed to write schema");
    tmp.flush().expect("failed to flush");

    let output = Command::new(bin_path())
        .args(["analyze", tmp.path().to_str().expect("non-UTF8 path")])
        .output()
        .expect("failed to execute dbsurveyor analyze");

    assert!(
        output.status.success(),
        "analyze should succeed for a valid schema: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Schema Analysis"),
        "analyze output should contain analysis header"
    );
    assert!(
        stdout.contains("Tables:"),
        "analyze output should list table count"
    );
}

#[cfg(feature = "experimental")]
#[test]
fn test_analyze_detailed_flag() {
    let mut tmp =
        tempfile::NamedTempFile::with_suffix(".json").expect("failed to create temp file");
    tmp.write_all(minimal_valid_schema().as_bytes())
        .expect("failed to write schema");
    tmp.flush().expect("failed to flush");

    let output = Command::new(bin_path())
        .args([
            "analyze",
            "--detailed",
            tmp.path().to_str().expect("non-UTF8 path"),
        ])
        .output()
        .expect("failed to execute dbsurveyor analyze --detailed");

    assert!(
        output.status.success(),
        "analyze --detailed should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Detailed Analysis"),
        "detailed output should contain detailed section"
    );
}
