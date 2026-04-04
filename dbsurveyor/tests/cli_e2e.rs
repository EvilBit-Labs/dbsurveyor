//! End-to-end tests for the `dbsurveyor` postprocessor binary.
//!
//! These tests invoke the compiled binary via `std::process::Command`
//! and verify exit codes and output for basic CLI interactions.

use std::process::Command;

/// Returns the path to the compiled `dbsurveyor` binary.
fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_dbsurveyor")
}

#[test]
fn test_postprocessor_help() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("failed to execute dbsurveyor --help");

    assert!(output.status.success(), "expected exit 0 for --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dbsurveyor"),
        "help output should mention binary name"
    );
    assert!(
        stdout.contains("schema documentation"),
        "help output should contain description"
    );
}

#[test]
fn test_postprocessor_version() {
    let output = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("failed to execute dbsurveyor --version");

    assert!(output.status.success(), "expected exit 0 for --version");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dbsurveyor"),
        "version output should mention binary name"
    );
}

#[test]
fn test_postprocessor_no_input() {
    let output = Command::new(bin_path())
        .output()
        .expect("failed to execute dbsurveyor with no args");

    // Should fail because no input file is provided
    assert!(
        !output.status.success(),
        "expected non-zero exit when no input file is provided"
    );
}

#[test]
fn test_postprocessor_validate_missing_file() {
    let output = Command::new(bin_path())
        .args(["validate", "nonexistent_file.json"])
        .output()
        .expect("failed to execute dbsurveyor validate");

    assert!(
        !output.status.success(),
        "expected non-zero exit for missing input file"
    );
}

#[test]
fn test_postprocessor_invalid_args() {
    let output = Command::new(bin_path())
        .arg("--nonexistent-flag")
        .output()
        .expect("failed to execute dbsurveyor with bad args");

    assert!(
        !output.status.success(),
        "expected non-zero exit for invalid args"
    );
}
