//! Security tests for credential protection and data sanitization
//!
//! These tests verify that database credentials are never exposed in outputs,
//! logs, or error messages.
//!
//! NOTE: Adapter-level security tests live in dbsurveyor-core, which owns the
//! adapter implementations. Tests that previously lived here referenced a
//! duplicate adapter abstraction that has been removed.
