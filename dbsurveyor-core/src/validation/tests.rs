//! Comprehensive tests for JSON Schema validation functionality.
//!
//! These tests verify that the JSON Schema validation correctly identifies
//! valid and invalid schema structures, security violations, and format
//! version compatibility issues.

use super::*;
use crate::models::*;
use serde_json::json;

/// Setup function to ensure validator is initialized for all tests
fn setup() {
    let _ = initialize_schema_validator();
}

#[test]
fn test_schema_initialization_success() {
    assert!(initialize_schema_validator().is_ok());
}

#[test]
fn test_valid_minimal_schema_passes() {
    setup();

    let valid_schema = json!({
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
    });

    assert!(validate_schema_output(&valid_schema).is_ok());
}

#[test]
fn test_valid_complex_schema_passes() {
    setup();

    let complex_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "production_db",
            "version": "13.7",
            "size_bytes": 1073741824,
            "encoding": "UTF8",
            "collation": "en_US.UTF-8",
            "owner": "dbadmin",
            "is_system_database": false,
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "users",
            "schema": "public",
            "columns": [{
                "name": "id",
                "data_type": {"Integer": {"bits": 32, "signed": true}},
                "is_nullable": false,
                "is_primary_key": true,
                "is_auto_increment": true,
                "ordinal_position": 1
            }, {
                "name": "email",
                "data_type": {"String": {"max_length": 255}},
                "is_nullable": false,
                "is_primary_key": false,
                "is_auto_increment": false,
                "ordinal_position": 2
            }],
            "primary_key": {
                "name": "users_pkey",
                "columns": ["id"]
            },
            "foreign_keys": [],
            "indexes": [],
            "constraints": [],
            "row_count": 1000
        }],
        "views": [],
        "indexes": [],
        "constraints": [],
        "procedures": [],
        "functions": [],
        "triggers": [],
        "custom_types": [],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 2500,
            "collector_version": "1.0.0",
            "warnings": ["Large table detected"]
        }
    });

    assert!(validate_schema_output(&complex_schema).is_ok());
}

#[test]
fn test_missing_required_field_fails() {
    setup();

    let invalid_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full"
            // Missing required collection_status
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&invalid_schema);
    assert!(result.is_err());

    if let Err(ValidationError::ValidationFailed { errors, .. }) = result {
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("collection_status")));
    } else {
        panic!("Expected ValidationFailed error");
    }
}

#[test]
fn test_invalid_data_type_fails() {
    setup();

    let invalid_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "test_table",
            "columns": [{
                "name": "invalid_col",
                "data_type": {"InvalidType": {}}, // Invalid data type
                "is_nullable": false,
                "ordinal_position": 1
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&invalid_schema);
    assert!(result.is_err());
}

#[test]
fn test_unsupported_version_fails() {
    setup();

    let invalid_version = json!({
        "format_version": "2.0", // Unsupported version
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&invalid_version);
    assert!(matches!(
        result,
        Err(ValidationError::UnsupportedVersion { .. })
    ));

    if let Err(ValidationError::UnsupportedVersion { version, supported }) = result {
        assert_eq!(version, "2.0");
        assert!(supported.contains(&"1.0".to_string()));
    }
}

#[test]
fn test_missing_format_version_fails() {
    setup();

    let no_version = json!({
        // Missing format_version
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&no_version);
    assert!(result.is_err());
}

#[test]
fn test_credential_field_names_fail() {
    setup();

    let schema_with_credential_field = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "users",
            "columns": [{
                "name": "password_hash", // This should trigger security validation
                "data_type": {"String": {"max_length": 255}},
                "is_nullable": false,
                "ordinal_position": 1
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_credential_field);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_secret_field_names_fail() {
    setup();

    let schema_with_secret_field = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "config",
            "columns": [{
                "name": "api_secret", // This should trigger security validation
                "data_type": {"String": {"max_length": 500}},
                "is_nullable": true,
                "ordinal_position": 1
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_secret_field);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_connection_string_in_default_value_fails() {
    setup();

    let schema_with_connection_string = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "config",
            "columns": [{
                "name": "db_url",
                "data_type": {"String": {"max_length": 500}},
                "is_nullable": false,
                "ordinal_position": 1,
                "default_value": "postgres://user:secret@localhost/db" // This should fail
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_connection_string);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_mysql_connection_string_fails() {
    setup();

    let schema_with_mysql_connection = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "settings",
            "columns": [{
                "name": "connection",
                "data_type": {"String": {"max_length": 1000}},
                "is_nullable": false,
                "ordinal_position": 1,
                "comment": "mysql://root:password@localhost:3306/mydb" // Should fail
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_mysql_connection);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_password_pattern_in_string_fails() {
    setup();

    let schema_with_password_pattern = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "logs",
            "columns": [{
                "name": "message",
                "data_type": {"String": {"max_length": 2000}},
                "is_nullable": true,
                "ordinal_position": 1,
                "comment": "Log entry with password=secret123" // Should fail
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_password_pattern);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_api_key_pattern_fails() {
    setup();

    let schema_with_api_key = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "config",
            "columns": [{
                "name": "settings",
                "data_type": {"String": {"max_length": 1000}},
                "is_nullable": true,
                "ordinal_position": 1,
                "default_value": "api_key=sk_live_abc123def456" // Should fail
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    let result = validate_schema_output(&schema_with_api_key);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_validate_and_parse_schema_success() {
    setup();

    let json_str = r#"{
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
    }"#;

    let result = validate_and_parse_schema(json_str);
    assert!(result.is_ok());

    let schema = result.unwrap();
    assert_eq!(schema.database_info.name, "test_db");
    assert_eq!(schema.format_version, "1.0");
    assert_eq!(schema.tables.len(), 0);
}

#[test]
fn test_validate_and_parse_invalid_json_fails() {
    setup();

    let invalid_json = r#"{
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            // Missing comma and invalid JSON
        }
    }"#;

    let result = validate_and_parse_schema(invalid_json);
    assert!(matches!(result, Err(ValidationError::JsonParsing { .. })));
}

#[test]
fn test_validate_and_parse_security_violation_fails() {
    setup();

    let json_with_credentials = r#"{
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "users",
            "columns": [{
                "name": "password_field",
                "data_type": {"String": {"max_length": 255}},
                "is_nullable": false,
                "ordinal_position": 1
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    }"#;

    let result = validate_and_parse_schema(json_with_credentials);
    assert!(matches!(
        result,
        Err(ValidationError::SecurityViolation { .. })
    ));
}

#[test]
fn test_get_schema_definition_success() {
    let schema_def = get_schema_definition();
    assert!(schema_def.is_ok());

    let schema = schema_def.unwrap();
    assert!(schema.get("$schema").is_some());
    assert!(schema.get("title").is_some());
    assert!(schema.get("properties").is_some());
    assert!(schema.get("properties").is_some());
}

#[test]
fn test_real_database_schema_validation() {
    setup();

    // Create a realistic database schema using the actual models
    let mut db_info = DatabaseInfo::new("production_db".to_string());
    db_info.version = Some("13.7".to_string());
    db_info.size_bytes = Some(1073741824);
    db_info.encoding = Some("UTF8".to_string());
    db_info.collation = Some("en_US.UTF-8".to_string());
    db_info.access_level = AccessLevel::Full;
    db_info.collection_status = CollectionStatus::Success;

    let mut schema = DatabaseSchema::new(db_info);

    // Add a sample table
    let table = Table {
        name: "users".to_string(),
        schema: Some("public".to_string()),
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                is_nullable: false,
                is_primary_key: true,
                is_auto_increment: true,
                default_value: None,
                comment: None,
                ordinal_position: 1,
            },
            Column {
                name: "email".to_string(),
                data_type: UnifiedDataType::String {
                    max_length: Some(255),
                },
                is_nullable: false,
                is_primary_key: false,
                is_auto_increment: false,
                default_value: None,
                comment: None,
                ordinal_position: 2,
            },
        ],
        primary_key: Some(PrimaryKey {
            name: Some("users_pkey".to_string()),
            columns: vec!["id".to_string()],
        }),
        foreign_keys: vec![],
        indexes: vec![],
        constraints: vec![],
        comment: None,
        row_count: Some(1000),
    };

    schema.tables.push(table);

    // Serialize and validate
    let json_value = serde_json::to_value(&schema).unwrap();
    let result = validate_schema_output(&json_value);

    assert!(
        result.is_ok(),
        "Real database schema should pass validation"
    );
}

#[test]
fn test_foreign_key_validation() {
    setup();

    let schema_with_fk = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "orders",
            "columns": [{
                "name": "id",
                "data_type": {"Integer": {"bits": 32, "signed": true}},
                "is_nullable": false,
                "ordinal_position": 1
            }, {
                "name": "user_id",
                "data_type": {"Integer": {"bits": 32, "signed": true}},
                "is_nullable": false,
                "ordinal_position": 2
            }],
            "foreign_keys": [{
                "name": "fk_orders_user",
                "columns": ["user_id"],
                "referenced_table": "users",
                "referenced_schema": "public",
                "referenced_columns": ["id"],
                "on_delete": "Cascade",
                "on_update": "Restrict"
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    assert!(validate_schema_output(&schema_with_fk).is_ok());
}

#[test]
fn test_array_data_type_validation() {
    setup();

    let schema_with_array = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "documents",
            "columns": [{
                "name": "tags",
                "data_type": {
                    "Array": {
                        "element_type": {"String": {"max_length": 50}}
                    }
                },
                "is_nullable": true,
                "ordinal_position": 1
            }]
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    assert!(validate_schema_output(&schema_with_array).is_ok());
}

#[test]
fn test_custom_data_type_validation() {
    setup();

    let schema_with_custom_type = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "products",
            "columns": [{
                "name": "status",
                "data_type": {
                    "Custom": {
                        "type_name": "product_status_enum"
                    }
                },
                "is_nullable": false,
                "ordinal_position": 1
            }]
        }],
        "custom_types": [{
            "name": "product_status_enum",
            "schema": "public",
            "definition": "CREATE TYPE product_status_enum AS ENUM ('active', 'inactive', 'discontinued')",
            "category": "Enum"
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    assert!(validate_schema_output(&schema_with_custom_type).is_ok());
}

#[test]
fn test_sampling_strategy_validation() {
    setup();

    let schema_with_samples = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "tables": [{
            "name": "users",
            "columns": [{
                "name": "id",
                "data_type": {"Integer": {"bits": 32, "signed": true}},
                "is_nullable": false,
                "ordinal_position": 1
            }]
        }],
        "samples": [{
            "table_name": "users",
            "schema_name": "public",
            "rows": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"}
            ],
            "sample_size": 2,
            "total_rows": 1000,
            "sampling_strategy": {"MostRecent": {"limit": 10}},
            "collected_at": "2024-01-15T10:30:00Z",
            "warnings": []
        }],
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });

    assert!(validate_schema_output(&schema_with_samples).is_ok());
}

#[test]
fn test_collection_status_variants() {
    setup();

    // Test Success status
    let success_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "Full",
            "collection_status": "Success"
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 1500,
            "collector_version": "1.0.0"
        }
    });
    assert!(validate_schema_output(&success_schema).is_ok());

    // Test Failed status
    let failed_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "test_db",
            "access_level": "None",
            "collection_status": {
                "Failed": {
                    "error": "Connection timeout"
                }
            }
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 30000,
            "collector_version": "1.0.0"
        }
    });
    assert!(validate_schema_output(&failed_schema).is_ok());

    // Test Skipped status
    let skipped_schema = json!({
        "format_version": "1.0",
        "database_info": {
            "name": "system_db",
            "access_level": "Limited",
            "collection_status": {
                "Skipped": {
                    "reason": "System database excluded"
                }
            }
        },
        "collection_metadata": {
            "collected_at": "2024-01-15T10:30:00Z",
            "collection_duration_ms": 100,
            "collector_version": "1.0.0"
        }
    });
    assert!(validate_schema_output(&skipped_schema).is_ok());
}
