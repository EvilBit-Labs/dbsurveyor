//! Database schema and metadata collection tool
//!
//! This tool connects to databases to collect schema information,
//! table structures, relationships, and metadata for offline analysis.
//!
//! # Security
//!
//! Database credentials are never exposed on the command line or in help text.
//! Credentials are obtained through secure methods in order of preference:
//! 1. Environment variables (`DATABASE_URL`)
//! 2. Configuration file (`--database-url-file`)
//!
//! This prevents credential leakage via process lists, shell history, or logs.

use clap::Parser;
use dbsurveyor_collect::{Cli, execute_cli};

fn main() {
    let cli = Cli::parse();

    match execute_cli(&cli) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}
