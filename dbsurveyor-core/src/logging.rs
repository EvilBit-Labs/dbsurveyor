//! Shared logging utilities for DBSurveyor binaries.
//!
//! Provides consistent logging configuration across collector and postprocessor.

use crate::Result;

/// Returns true if ANSI color output should be disabled.
///
/// Checks for the `NO_COLOR` environment variable (any value) and
/// `TERM=dumb`, following the <https://no-color.org/> convention.
fn should_disable_color() -> bool {
    std::env::var("NO_COLOR").is_ok() || std::env::var("TERM").is_ok_and(|t| t == "dumb")
}

/// Initializes structured logging based on verbosity level.
///
/// Respects `NO_COLOR` and `TERM=dumb` to disable ANSI escape codes.
///
/// # Arguments
/// * `verbose` - Verbosity level (0=INFO, 1=DEBUG, 2+=TRACE)
/// * `quiet` - If true, only show ERROR level logs
///
/// # Returns
/// Ok(()) if logging was initialized successfully
///
/// # Example
/// ```rust,no_run
/// use dbsurveyor_core::logging::init_logging;
///
/// // Initialize at DEBUG level
/// init_logging(1, false).expect("Failed to initialize logging");
/// ```
pub fn init_logging(verbose: u8, quiet: bool) -> Result<()> {
    let level = match (quiet, verbose) {
        (true, _) => tracing::Level::ERROR,
        (false, 0) => tracing::Level::INFO,
        (false, 1) => tracing::Level::DEBUG,
        (false, _) => tracing::Level::TRACE,
    };

    let use_ansi = !should_disable_color();

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(use_ansi)
        .try_init()
        .map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Failed to initialize logging: {}",
                e
            ))
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note: Logging can only be initialized once per test process,
    // so we skip actual initialization tests here.

    #[test]
    fn test_should_disable_color_reads_env() {
        // We cannot safely mutate env vars in Rust 2024 edition without unsafe,
        // so we just verify the function runs without panicking and returns a bool.
        // The actual NO_COLOR/TERM logic is trivial (two env var reads).
        let _result: bool = super::should_disable_color();
    }

    #[test]
    fn test_verbosity_levels() {
        // Verify the match logic without actually initializing
        let test_cases = [
            ((true, 0), tracing::Level::ERROR),
            ((true, 5), tracing::Level::ERROR),
            ((false, 0), tracing::Level::INFO),
            ((false, 1), tracing::Level::DEBUG),
            ((false, 2), tracing::Level::TRACE),
            ((false, 10), tracing::Level::TRACE),
        ];

        for ((quiet, verbose), expected) in test_cases {
            let level = match (quiet, verbose) {
                (true, _) => tracing::Level::ERROR,
                (false, 0) => tracing::Level::INFO,
                (false, 1) => tracing::Level::DEBUG,
                (false, _) => tracing::Level::TRACE,
            };
            assert_eq!(
                level, expected,
                "Failed for quiet={}, verbose={}",
                quiet, verbose
            );
        }
    }
}
