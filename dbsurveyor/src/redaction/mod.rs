//! Sample-data redaction for the postprocessor.

use clap::ValueEnum;
use dbsurveyor_core::models::TableSample;
use serde_json::Value;

const REDACTED_VALUE: &str = "[REDACTED]";

/// Data redaction mode for postprocessor sample rendering.
#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum RedactionMode {
    /// No redaction (show all data)
    None,
    /// Minimal redaction (only obvious sensitive fields)
    Minimal,
    /// Balanced redaction (recommended default)
    Balanced,
    /// Conservative redaction (maximum privacy)
    Conservative,
}

/// A table sample with row values redacted for output rendering.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RedactedTableSample {
    pub(crate) table_name: String,
    pub(crate) schema_name: Option<String>,
    pub(crate) rows: Vec<Value>,
    pub(crate) mode_applied: RedactionMode,
    pub(crate) warnings: Vec<String>,
}

/// Redacts sample rows without mutating source samples.
pub(crate) struct Redactor {
    mode: RedactionMode,
}

impl Redactor {
    pub(crate) fn new(mode: RedactionMode) -> Self {
        Self { mode }
    }

    pub(crate) fn redact(&self, samples: &[TableSample]) -> Vec<RedactedTableSample> {
        samples
            .iter()
            .map(|sample| RedactedTableSample {
                table_name: sample.table_name.clone(),
                schema_name: sample.schema_name.clone(),
                rows: sample
                    .rows
                    .iter()
                    .map(|row| redact_value(row, None, &self.mode))
                    .collect(),
                mode_applied: self.mode.clone(),
                warnings: sample.warnings.clone(),
            })
            .collect()
    }
}

fn redact_value(value: &Value, key: Option<&str>, mode: &RedactionMode) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(field_name, field_value)| {
                    (
                        field_name.clone(),
                        redact_value(field_value, Some(field_name), mode),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_value(item, key, mode))
                .collect(),
        ),
        Value::String(text) => {
            if should_redact_string(key, text, mode) {
                Value::String(REDACTED_VALUE.to_string())
            } else {
                Value::String(text.clone())
            }
        }
        other => other.clone(),
    }
}

fn should_redact_string(key: Option<&str>, value: &str, mode: &RedactionMode) -> bool {
    match mode {
        RedactionMode::None => false,
        RedactionMode::Minimal => matches_key(key, MINIMAL_PATTERNS),
        RedactionMode::Balanced => {
            matches_key(key, MINIMAL_PATTERNS) || matches_key(key, BALANCED_PATTERNS)
        }
        RedactionMode::Conservative => {
            !is_conservative_safe_key(key) && !looks_like_timestamp(value)
        }
    }
}

const MINIMAL_PATTERNS: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "key",
    "private_key",
    "passwd",
];

const BALANCED_PATTERNS: &[&str] = &[
    "email",
    "ssn",
    "phone",
    "dob",
    "birth",
    "credit_card",
    "card_number",
    "cvv",
    "sin",
];

const CONSERVATIVE_SAFE_FIELDS: &[&str] = &[
    "id",
    "created_at",
    "updated_at",
    "timestamp",
    "date",
    "time",
];

fn matches_key(key: Option<&str>, patterns: &[&str]) -> bool {
    let Some(key) = key else {
        return false;
    };
    let normalized = key.to_ascii_lowercase();
    patterns.iter().any(|pattern| normalized.contains(pattern))
}

fn is_conservative_safe_key(key: Option<&str>) -> bool {
    let Some(key) = key else {
        return false;
    };
    let normalized = key.to_ascii_lowercase();
    CONSERVATIVE_SAFE_FIELDS.contains(&normalized.as_str())
        || normalized.ends_with("_id")
        || normalized.ends_with("_at")
}

fn looks_like_timestamp(value: &str) -> bool {
    value.contains('T') || value.contains('-') || value.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbsurveyor_core::models::{SamplingStrategy, TableSample};
    use serde_json::json;

    fn sample_fixture() -> TableSample {
        TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: vec![json!({
                "id": 42,
                "username": "alice",
                "password": "hunter2",
                "email": "alice@example.com",
                "ssn": "123-45-6789",
                "description": "operator notes",
                "created_at": "2025-01-01T00:00:00Z"
            })],
            sample_size: 1,
            total_rows: Some(1),
            sampling_strategy: SamplingStrategy::MostRecent { limit: 1 },
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
            sample_status: None,
        }
    }

    fn redacted_count(samples: &[RedactedTableSample]) -> usize {
        fn count_value(value: &Value) -> usize {
            match value {
                Value::String(text) if text == REDACTED_VALUE => 1,
                Value::Array(items) => items.iter().map(count_value).sum(),
                Value::Object(map) => map.values().map(count_value).sum(),
                _ => 0,
            }
        }

        samples
            .iter()
            .flat_map(|sample| sample.rows.iter())
            .map(count_value)
            .sum()
    }

    #[test]
    fn test_none_mode_pass_through() {
        let original = sample_fixture();
        let redacted = Redactor::new(RedactionMode::None).redact(std::slice::from_ref(&original));

        assert_eq!(redacted[0].rows, original.rows);
    }

    #[test]
    fn test_minimal_masks_password_not_username() {
        let redacted = Redactor::new(RedactionMode::Minimal).redact(&[sample_fixture()]);
        let row = redacted[0].rows[0].as_object().expect("row object");

        assert_eq!(row["password"], Value::String(REDACTED_VALUE.to_string()));
        assert_eq!(row["username"], Value::String("alice".to_string()));
    }

    #[test]
    fn test_balanced_masks_email_and_ssn_but_not_user_id() {
        let redacted = Redactor::new(RedactionMode::Balanced).redact(&[sample_fixture()]);
        let row = redacted[0].rows[0].as_object().expect("row object");

        assert_eq!(row["email"], Value::String(REDACTED_VALUE.to_string()));
        assert_eq!(row["ssn"], Value::String(REDACTED_VALUE.to_string()));
        assert_eq!(row["id"], json!(42));
    }

    #[test]
    fn test_conservative_masks_description_but_not_numeric_id() {
        let redacted = Redactor::new(RedactionMode::Conservative).redact(&[sample_fixture()]);
        let row = redacted[0].rows[0].as_object().expect("row object");

        assert_eq!(
            row["description"],
            Value::String(REDACTED_VALUE.to_string())
        );
        assert_eq!(row["id"], json!(42));
    }

    #[test]
    fn test_progressive_redaction_contract() {
        let samples = [sample_fixture()];
        let none = Redactor::new(RedactionMode::None).redact(&samples);
        let minimal = Redactor::new(RedactionMode::Minimal).redact(&samples);
        let balanced = Redactor::new(RedactionMode::Balanced).redact(&samples);
        let conservative = Redactor::new(RedactionMode::Conservative).redact(&samples);

        assert!(redacted_count(&conservative) >= redacted_count(&balanced));
        assert!(redacted_count(&balanced) >= redacted_count(&minimal));
        assert!(redacted_count(&minimal) >= redacted_count(&none));
    }

    #[test]
    fn test_source_table_sample_not_mutated() {
        let original = sample_fixture();
        let before = original.rows.clone();
        let _ = Redactor::new(RedactionMode::Balanced).redact(std::slice::from_ref(&original));

        assert_eq!(original.rows, before);
    }
}
