//! Criterion benchmarks for dbsurveyor-core.
//!
//! Covers JSON schema validation, type mapping, regex pattern matching,
//! and quality analysis on in-memory data.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use regex::Regex;
use serde_json::json;

use dbsurveyor_core::models::{DatabaseInfo, DatabaseSchema, SamplingStrategy, TableSample};
use dbsurveyor_core::validation::validate_and_parse_schema;
use dbsurveyor_core::{QualityAnalyzer, QualityConfig, SamplingConfig, SensitivePattern};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds a minimal but valid `DatabaseSchema`, serializes it to JSON.
fn build_valid_schema_json(table_count: usize, column_count: usize) -> String {
    let schema = DatabaseSchema::new(DatabaseInfo::new("bench_db".to_string()));

    // Serialize, then patch in tables via serde_json::Value so we control
    // the shape without constructing every nested struct.
    let mut value: serde_json::Value = serde_json::to_value(&schema).expect("serialize schema");

    let tables: Vec<serde_json::Value> = (0..table_count)
        .map(|t| {
            let columns: Vec<serde_json::Value> = (0..column_count)
                .map(|c| {
                    json!({
                        "name": format!("col_{c}"),
                        "data_type": "varchar",
                        "unified_type": { "String": { "max_length": 255 } },
                        "is_nullable": true,
                        "ordinal_position": c + 1
                    })
                })
                .collect();
            json!({
                "name": format!("table_{t}"),
                "schema_name": "public",
                "columns": columns,
                "indexes": [],
                "constraints": [],
                "row_count": null,
                "size_bytes": null,
                "is_system_table": false
            })
        })
        .collect();

    value["tables"] = serde_json::Value::Array(tables);
    serde_json::to_string(&value).expect("re-serialize schema")
}

/// Builds a `TableSample` with the given number of rows and columns.
fn build_table_sample(rows: usize, cols: usize) -> TableSample {
    let row_values: Vec<serde_json::Value> = (0..rows)
        .map(|r| {
            let mut map = serde_json::Map::with_capacity(cols);
            for c in 0..cols {
                let val = if r % 10 == 0 {
                    serde_json::Value::Null
                } else if r % 7 == 0 {
                    serde_json::Value::String(String::new())
                } else {
                    serde_json::Value::String(format!("value_{r}_{c}"))
                };
                map.insert(format!("col_{c}"), val);
            }
            serde_json::Value::Object(map)
        })
        .collect();

    TableSample {
        table_name: "bench_table".to_string(),
        schema_name: Some("public".to_string()),
        rows: row_values,
        sample_size: rows as u32,
        total_rows: Some(1_000_000),
        sampling_strategy: SamplingStrategy::MostRecent { limit: rows as u32 },
        collected_at: chrono::Utc::now(),
        warnings: Vec::new(),
        sample_status: None,
    }
}

/// Compiles sensitive detection patterns from a `SamplingConfig` into
/// `(Regex, description)` pairs, mirroring the internal compilation.
fn compile_patterns(config: &SamplingConfig) -> Vec<(Regex, String)> {
    config
        .sensitive_detection_patterns
        .iter()
        .filter_map(|p| {
            Regex::new(&p.pattern)
                .ok()
                .map(|r| (r, p.description.clone()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

/// JSON schema validation throughput.
fn bench_schema_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("schema_validation");

    // Small schema: 5 tables, 8 columns each
    let small_json = build_valid_schema_json(5, 8);
    group.bench_function("small_5t_8c", |b| {
        b.iter(|| validate_and_parse_schema(black_box(&small_json)));
    });

    // Medium schema: 20 tables, 15 columns each
    let medium_json = build_valid_schema_json(20, 15);
    group.bench_function("medium_20t_15c", |b| {
        b.iter(|| validate_and_parse_schema(black_box(&medium_json)));
    });

    // Large schema: 50 tables, 30 columns each
    let large_json = build_valid_schema_json(50, 30);
    group.bench_function("large_50t_30c", |b| {
        b.iter(|| validate_and_parse_schema(black_box(&large_json)));
    });

    group.finish();
}

/// Type mapping throughput for MySQL and SQLite adapters.
fn bench_type_mapping(c: &mut Criterion) {
    let mut group = c.benchmark_group("type_mapping");

    // MySQL type mapping
    #[cfg(feature = "mysql")]
    {
        use dbsurveyor_core::adapters::mysql::map_mysql_type;

        group.bench_function("mysql_10_types", |b| {
            b.iter(|| {
                black_box(map_mysql_type("varchar", Some(255), None, None));
                black_box(map_mysql_type("int", None, None, None));
                black_box(map_mysql_type("decimal", None, Some(10), Some(2)));
                black_box(map_mysql_type("tinyint", Some(1), None, None));
                black_box(map_mysql_type("bigint unsigned", None, None, None));
                black_box(map_mysql_type("text", None, None, None));
                black_box(map_mysql_type("datetime", None, None, None));
                black_box(map_mysql_type("json", None, None, None));
                black_box(map_mysql_type("enum", None, None, None));
                black_box(map_mysql_type("double", None, Some(53), None));
            });
        });
    }

    // SQLite type mapping
    #[cfg(feature = "sqlite")]
    {
        use dbsurveyor_core::adapters::sqlite::map_sqlite_type;

        let sqlite_inputs: Vec<&str> = vec![
            "INTEGER",
            "TEXT",
            "REAL",
            "BLOB",
            "VARCHAR(255)",
            "BOOLEAN",
            "DATETIME",
            "NUMERIC(10,2)",
            "FLOAT",
            "",
        ];

        group.bench_function("sqlite_10_types", |b| {
            b.iter(|| {
                for ty in &sqlite_inputs {
                    black_box(map_sqlite_type(ty));
                }
            });
        });
    }

    group.finish();
}

/// Regex pattern compilation and matching for sensitive data detection.
fn bench_pattern_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_matching");

    // Benchmark pattern compilation (default patterns)
    group.bench_function("compile_default_patterns", |b| {
        b.iter(|| {
            black_box(SamplingConfig::default());
        });
    });

    // Benchmark pattern compilation with extra patterns
    group.bench_function("compile_with_extra_patterns", |b| {
        b.iter(|| {
            let config = SamplingConfig::default()
                .add_sensitive_pattern(SensitivePattern::new(
                    r"(?i)(credit_card|cc_num|card_number)",
                    "Credit card field",
                ))
                .add_sensitive_pattern(SensitivePattern::new(
                    r"(?i)(phone|mobile|cell)",
                    "Phone number field",
                ))
                .add_sensitive_pattern(SensitivePattern::new(
                    r"(?i)(address|street|zip|postal)",
                    "Address field",
                ))
                .add_sensitive_pattern(SensitivePattern::new(
                    r"(?i)(dob|birth_date|date_of_birth)",
                    "Date of birth field",
                ))
                .add_sensitive_pattern(SensitivePattern::new(
                    r"(?i)(token|api_key|secret)",
                    "Secret/token field",
                ));
            black_box(config);
        });
    });

    // Benchmark recompile_patterns (simulates post-deserialization)
    group.bench_function("recompile_patterns", |b| {
        let base = SamplingConfig::default();
        let serialized = serde_json::to_string(&base).expect("serialize config");

        b.iter(|| {
            let mut config: SamplingConfig =
                serde_json::from_str(&serialized).expect("deserialize config");
            config.recompile_patterns();
            black_box(config);
        });
    });

    // Benchmark pattern matching against column names
    group.bench_function("match_50_columns", |b| {
        let config = SamplingConfig::default();
        let compiled = compile_patterns(&config);
        let column_names: Vec<String> = vec![
            "id",
            "user_name",
            "email_address",
            "password_hash",
            "ssn",
            "first_name",
            "last_name",
            "created_at",
            "updated_at",
            "status",
            "balance",
            "account_type",
            "phone_number",
            "address_line1",
            "city",
            "state",
            "zip_code",
            "country",
            "notes",
            "description",
            "is_active",
            "role_id",
            "department",
            "salary",
            "hire_date",
            "manager_id",
            "project_id",
            "task_name",
            "priority",
            "due_date",
            "completed_at",
            "assigned_to",
            "reviewer",
            "approval_status",
            "version",
            "checksum",
            "file_path",
            "mime_type",
            "file_size",
            "encoding",
            "retry_count",
            "error_message",
            "log_level",
            "source_ip",
            "user_agent",
            "session_id",
            "token_hash",
            "expires_at",
            "refresh_token",
            "scope",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        b.iter(|| {
            let mut match_count: usize = 0;
            for col in &column_names {
                for (regex, _desc) in &compiled {
                    if regex.is_match(col) {
                        match_count += 1;
                    }
                }
            }
            black_box(match_count);
        });
    });

    group.finish();
}

/// Quality analysis on in-memory table samples.
fn bench_quality_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("quality_analysis");

    let config = QualityConfig::default();
    let analyzer = QualityAnalyzer::new(config);

    // Small sample: 20 rows, 5 columns
    let small_sample = build_table_sample(20, 5);
    group.bench_function("small_20r_5c", |b| {
        b.iter(|| {
            let result = analyzer.analyze(black_box(&small_sample));
            black_box(result)
        });
    });

    // Medium sample: 100 rows, 10 columns
    let medium_sample = build_table_sample(100, 10);
    group.bench_function("medium_100r_10c", |b| {
        b.iter(|| {
            let result = analyzer.analyze(black_box(&medium_sample));
            black_box(result)
        });
    });

    // Large sample: 500 rows, 20 columns
    let large_sample = build_table_sample(500, 20);
    group.bench_function("large_500r_20c", |b| {
        b.iter(|| {
            let result = analyzer.analyze(black_box(&large_sample));
            black_box(result)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_schema_validation,
    bench_type_mapping,
    bench_pattern_matching,
    bench_quality_analysis,
);
criterion_main!(benches);
