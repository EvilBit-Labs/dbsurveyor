//! Consistency analysis for data quality assessment.
//!
//! This module analyzes data type consistency and format pattern adherence
//! to identify potential data quality issues.

use crate::models::TableSample;

use super::models::{ConsistencyMetrics, FormatViolation, TypeInconsistency};

/// JSON value type classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum JsonType {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

impl JsonType {
    fn from_value(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonType::Null,
            serde_json::Value::Bool(_) => JsonType::Boolean,
            serde_json::Value::Number(_) => JsonType::Number,
            serde_json::Value::String(_) => JsonType::String,
            serde_json::Value::Array(_) => JsonType::Array,
            serde_json::Value::Object(_) => JsonType::Object,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            JsonType::Null => "null",
            JsonType::Boolean => "boolean",
            JsonType::Number => "number",
            JsonType::String => "string",
            JsonType::Array => "array",
            JsonType::Object => "object",
        }
    }
}

/// Known format patterns for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FormatPattern {
    Email,
    Uuid,
    IsoDate,
    IsoDateTime,
}

impl FormatPattern {
    fn name(&self) -> &'static str {
        match self {
            FormatPattern::Email => "email",
            FormatPattern::Uuid => "uuid",
            FormatPattern::IsoDate => "iso_date",
            FormatPattern::IsoDateTime => "iso_datetime",
        }
    }

    /// Heuristic pattern matching without regex for performance.
    ///
    /// These checks are intentionally loose -- they detect "looks like" patterns
    /// for data quality classification, not strict format validation (e.g., the
    /// email check is not RFC 5322 compliant). This trade-off favors speed and
    /// low overhead in the collection pipeline.
    fn matches(&self, value: &str) -> bool {
        match self {
            FormatPattern::Email => value.contains('@') && value.contains('.'),
            FormatPattern::Uuid => {
                // Validate 8-4-4-4-12 segment layout
                value.len() == 36
                    && value.as_bytes().get(8) == Some(&b'-')
                    && value.as_bytes().get(13) == Some(&b'-')
                    && value.as_bytes().get(18) == Some(&b'-')
                    && value.as_bytes().get(23) == Some(&b'-')
                    && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            }
            FormatPattern::IsoDate => {
                value.len() == 10
                    && value.chars().nth(4) == Some('-')
                    && value.chars().nth(7) == Some('-')
            }
            FormatPattern::IsoDateTime => {
                value.len() >= 19 && value.contains('T') && value.contains(':')
            }
        }
    }
}

/// Detects the format pattern of a string value.
fn detect_format(value: &str) -> Option<FormatPattern> {
    // Order matters - check more specific patterns first
    if FormatPattern::Uuid.matches(value) {
        return Some(FormatPattern::Uuid);
    }
    if FormatPattern::IsoDateTime.matches(value) {
        return Some(FormatPattern::IsoDateTime);
    }
    if FormatPattern::IsoDate.matches(value) {
        return Some(FormatPattern::IsoDate);
    }
    if FormatPattern::Email.matches(value) {
        return Some(FormatPattern::Email);
    }
    None
}

/// Analyzes consistency of sampled data.
///
/// Consistency measures type uniformity within columns and adherence
/// to detected format patterns.
pub fn analyze_consistency(sample: &TableSample) -> ConsistencyMetrics {
    let column_names = match sample.column_names() {
        Some(names) => names,
        None => return ConsistencyMetrics::default(),
    };

    let mut type_inconsistencies: Vec<TypeInconsistency> = Vec::new();
    let mut format_violations: Vec<FormatViolation> = Vec::new();

    let total_rows = sample.rows.len();

    // Analyze each column
    for column_name in &column_names {
        // Track type distribution (excluding nulls)
        let mut type_counts: std::collections::HashMap<JsonType, u64> =
            std::collections::HashMap::new();

        // Track format patterns for string columns
        let mut format_counts: std::collections::HashMap<Option<FormatPattern>, u64> =
            std::collections::HashMap::new();

        for row in &sample.rows {
            if let Some(value) = row.as_object().and_then(|obj| obj.get(column_name)) {
                let json_type = JsonType::from_value(value);

                // Don't count nulls in type distribution
                if json_type != JsonType::Null {
                    *type_counts.entry(json_type.clone()).or_insert(0) += 1;
                }

                // Check string format patterns
                if let serde_json::Value::String(s) = value
                    && !s.is_empty()
                {
                    let format = detect_format(s);
                    *format_counts.entry(format).or_insert(0) += 1;
                }
            }
        }

        // Check for type inconsistency (more than one non-null type)
        if type_counts.len() > 1 {
            let (dominant_type, _) = type_counts.iter().max_by_key(|(_, count)| *count).unwrap();

            let other_types: Vec<String> = type_counts
                .keys()
                .filter(|t| *t != dominant_type)
                .map(|t| t.name().to_string())
                .collect();

            let inconsistent_count: u64 = type_counts
                .iter()
                .filter(|(t, _)| *t != dominant_type)
                .map(|(_, count)| *count)
                .sum();

            type_inconsistencies.push(TypeInconsistency {
                column_name: column_name.clone(),
                expected_type: dominant_type.name().to_string(),
                found_types: other_types,
                inconsistent_count,
            });
        }

        // Check for format violations (if a dominant format pattern exists)
        let format_values: Vec<_> = format_counts.iter().filter(|(f, _)| f.is_some()).collect();

        if !format_values.is_empty() {
            // Find the dominant format
            if let Some((Some(dominant_format), dominant_count)) =
                format_values.iter().max_by_key(|(_, count)| *count)
            {
                // If the dominant format covers majority of values, check for violations
                let total_formatted: u64 = format_counts.values().sum();
                if **dominant_count as f64 / total_formatted as f64 > 0.5 {
                    // Count values that don't match the dominant format
                    let violation_count: u64 = format_counts
                        .iter()
                        .filter(|(f, _)| f.as_ref() != Some(dominant_format))
                        .map(|(_, count)| *count)
                        .sum();

                    if violation_count > 0 {
                        format_violations.push(FormatViolation {
                            column_name: column_name.clone(),
                            expected_format: dominant_format.name().to_string(),
                            violation_count,
                        });
                    }
                }
            }
        }
    }

    // Calculate overall consistency score
    let total_cells = (total_rows * column_names.len()) as f64;
    let type_inconsistent_count: u64 = type_inconsistencies
        .iter()
        .map(|t| t.inconsistent_count)
        .sum();
    let format_violation_count: u64 = format_violations.iter().map(|f| f.violation_count).sum();

    let score = if total_cells > 0.0 {
        let inconsistent = (type_inconsistent_count + format_violation_count) as f64;
        (1.0 - inconsistent / total_cells).max(0.0)
    } else {
        1.0
    };

    ConsistencyMetrics {
        score,
        type_inconsistencies,
        format_violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SamplingStrategy;
    use serde_json::json;

    fn create_sample(rows: Vec<serde_json::Value>) -> TableSample {
        TableSample {
            table_name: "test_table".to_string(),
            schema_name: Some("public".to_string()),
            rows,
            sample_size: 10,
            total_rows: Some(100),
            sampling_strategy: SamplingStrategy::MostRecent { limit: 10 },
            collected_at: chrono::Utc::now(),
            warnings: vec![],
        }
    }

    #[test]
    fn test_consistency_uniform_types() {
        let rows = vec![
            json!({"id": 1, "name": "Alice", "active": true}),
            json!({"id": 2, "name": "Bob", "active": false}),
            json!({"id": 3, "name": "Charlie", "active": true}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        assert!((metrics.score - 1.0).abs() < 0.001);
        assert!(metrics.type_inconsistencies.is_empty());
    }

    #[test]
    fn test_consistency_mixed_types() {
        let rows = vec![
            json!({"id": 1, "value": 100}),
            json!({"id": 2, "value": "two hundred"}), // string instead of number
            json!({"id": 3, "value": 300}),
            json!({"id": 4, "value": 400}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        assert!(metrics.score < 1.0);
        assert_eq!(metrics.type_inconsistencies.len(), 1);

        let inconsistency = &metrics.type_inconsistencies[0];
        assert_eq!(inconsistency.column_name, "value");
        assert_eq!(inconsistency.expected_type, "number");
        assert!(inconsistency.found_types.contains(&"string".to_string()));
        assert_eq!(inconsistency.inconsistent_count, 1);
    }

    #[test]
    fn test_consistency_nulls_ignored_for_type_check() {
        // Nulls should not count as a separate type
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": null}),
            json!({"id": 3, "name": "Charlie"}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        // No type inconsistency because null is not counted
        assert!(metrics.type_inconsistencies.is_empty());
    }

    #[test]
    fn test_consistency_email_format() {
        let rows = vec![
            json!({"id": 1, "email": "alice@example.com"}),
            json!({"id": 2, "email": "bob@example.com"}),
            json!({"id": 3, "email": "not-an-email"}), // violation
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        assert_eq!(metrics.format_violations.len(), 1);
        let violation = &metrics.format_violations[0];
        assert_eq!(violation.column_name, "email");
        assert_eq!(violation.expected_format, "email");
        assert_eq!(violation.violation_count, 1);
    }

    #[test]
    fn test_consistency_uuid_format() {
        let rows = vec![
            json!({"id": "550e8400-e29b-41d4-a716-446655440000"}),
            json!({"id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"}),
            json!({"id": "not-a-uuid"}), // violation
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        assert_eq!(metrics.format_violations.len(), 1);
        let violation = &metrics.format_violations[0];
        assert_eq!(violation.expected_format, "uuid");
    }

    #[test]
    fn test_consistency_iso_date_format() {
        let rows = vec![
            json!({"date": "2024-01-15"}),
            json!({"date": "2024-02-20"}),
            json!({"date": "01/15/2024"}), // violation - different format
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        assert_eq!(metrics.format_violations.len(), 1);
        let violation = &metrics.format_violations[0];
        assert_eq!(violation.expected_format, "iso_date");
    }

    #[test]
    fn test_consistency_empty_sample() {
        let metrics = analyze_consistency(&create_sample(vec![]));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.type_inconsistencies.is_empty());
        assert!(metrics.format_violations.is_empty());
    }

    #[test]
    fn test_consistency_non_object_row() {
        // First row is not an object - should return default metrics
        let rows = vec![json!([1, 2, 3]), json!([4, 5, 6])];

        let metrics = analyze_consistency(&create_sample(rows));

        assert_eq!(metrics.score, 1.0);
        assert!(metrics.type_inconsistencies.is_empty());
        assert!(metrics.format_violations.is_empty());
    }

    #[test]
    fn test_consistency_all_nulls_in_column() {
        // All nulls should not cause type inconsistency (nulls are ignored)
        let rows = vec![
            json!({"value": null}),
            json!({"value": null}),
            json!({"value": null}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        // No type inconsistency because nulls are not counted
        assert!(metrics.type_inconsistencies.is_empty());
    }

    #[test]
    fn test_consistency_empty_strings_no_format() {
        // Empty strings should not be checked for format
        let rows = vec![
            json!({"email": ""}),
            json!({"email": ""}),
            json!({"email": "user@example.com"}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        // Only one email to detect format from, so no dominant format
        // No format violations expected
        assert!(metrics.format_violations.is_empty());
    }

    #[test]
    fn test_consistency_multiple_type_mismatches() {
        // Multiple different types in one column
        let rows = vec![
            json!({"data": 123}),
            json!({"data": "string"}),
            json!({"data": true}),
            json!({"data": 456}),
            json!({"data": 789}),
        ];

        let metrics = analyze_consistency(&create_sample(rows));

        // Should detect inconsistency - numbers are dominant
        assert!(!metrics.type_inconsistencies.is_empty());
        let inconsistency = &metrics.type_inconsistencies[0];
        assert_eq!(inconsistency.expected_type, "number");
        // Should have both string and boolean as other types
        assert!(inconsistency.found_types.contains(&"string".to_string()));
        assert!(inconsistency.found_types.contains(&"boolean".to_string()));
    }

    #[test]
    fn test_detect_format_priority() {
        // UUID should be detected before other patterns
        assert_eq!(
            detect_format("550e8400-e29b-41d4-a716-446655440000"),
            Some(FormatPattern::Uuid)
        );

        // ISO datetime before ISO date
        assert_eq!(
            detect_format("2024-01-15T10:30:00"),
            Some(FormatPattern::IsoDateTime)
        );

        // ISO date
        assert_eq!(detect_format("2024-01-15"), Some(FormatPattern::IsoDate));

        // Email
        assert_eq!(
            detect_format("user@example.com"),
            Some(FormatPattern::Email)
        );

        // No pattern
        assert_eq!(detect_format("random text"), None);
    }
}
