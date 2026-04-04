//! End-to-end tests for the `dbsurveyor-collect` binary.
//!
//! These tests invoke the compiled binary via `std::process::Command`
//! and verify exit codes and output for basic CLI interactions.

use std::process::Command;

/// Returns the path to the compiled `dbsurveyor-collect` binary.
fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_dbsurveyor-collect")
}

#[test]
fn test_collect_help() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("failed to execute dbsurveyor-collect --help");

    assert!(output.status.success(), "expected exit 0 for --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dbsurveyor-collect"),
        "help output should mention binary name"
    );
    assert!(
        stdout.contains("DBSurveyor Collector"),
        "help output should contain description"
    );
}

#[test]
fn test_collect_version() {
    let output = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("failed to execute dbsurveyor-collect --version");

    assert!(output.status.success(), "expected exit 0 for --version");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dbsurveyor-collect"),
        "version output should mention binary name"
    );
}

#[test]
fn test_collect_invalid_args() {
    let output = Command::new(bin_path())
        .arg("--nonexistent-flag")
        .output()
        .expect("failed to execute dbsurveyor-collect with bad args");

    assert!(
        !output.status.success(),
        "expected non-zero exit for invalid args"
    );
}

#[test]
fn test_collect_list_subcommand() {
    let output = Command::new(bin_path())
        .arg("list")
        .output()
        .expect("failed to execute dbsurveyor-collect list");

    assert!(
        output.status.success(),
        "expected exit 0 for list subcommand"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Supported Database Types"),
        "list output should mention supported databases"
    );
}
