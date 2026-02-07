//! Tests for the database schema models.

use super::*;

#[test]
fn test_database_schema_creation() {
    let mut db_info = DatabaseInfo::new("test_db".to_string());
    db_info.version = Some("13.0".to_string());
    db_info.size_bytes = Some(1024);
    db_info.encoding = Some("UTF8".to_string());
    db_info.collation = Some("en_US.UTF-8".to_string());

    let schema = DatabaseSchema::new(db_info);
    assert_eq!(schema.format_version, "1.0");
    assert_eq!(schema.database_info.name, "test_db");
    assert_eq!(schema.object_count(), 0);
    assert_eq!(schema.sample_count(), 0);
}

#[test]
fn test_add_warning() {
    let db_info = DatabaseInfo::new("test_db".to_string());

    let mut schema = DatabaseSchema::new(db_info);
    schema.add_warning("Test warning".to_string());

    assert_eq!(schema.collection_metadata.warnings.len(), 1);
    assert_eq!(schema.collection_metadata.warnings[0], "Test warning");
}

#[test]
fn test_database_info_creation() {
    let db_info = DatabaseInfo::new("test_db".to_string());
    assert_eq!(db_info.name, "test_db");
    assert!(!db_info.is_system_database);
    assert!(matches!(db_info.access_level, AccessLevel::Full));
    assert!(matches!(
        db_info.collection_status,
        CollectionStatus::Success
    ));
}

#[test]
fn test_add_samples() {
    let db_info = DatabaseInfo::new("test_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);

    let sample = TableSample {
        table_name: "users".to_string(),
        schema_name: Some("public".to_string()),
        rows: vec![serde_json::json!({"id": 1, "name": "test"})],
        sample_size: 1,
        total_rows: Some(100),
        sampling_strategy: SamplingStrategy::MostRecent { limit: 1 },
        collected_at: chrono::Utc::now(),
        warnings: Vec::new(),
    };

    schema.add_samples(vec![sample]);
    assert_eq!(schema.sample_count(), 1);
}

#[test]
fn test_database_type_display() {
    assert_eq!(DatabaseType::PostgreSQL.to_string(), "PostgreSQL");
    assert_eq!(DatabaseType::MySQL.to_string(), "MySQL");
    assert_eq!(DatabaseType::SQLite.to_string(), "SQLite");
    assert_eq!(DatabaseType::MongoDB.to_string(), "MongoDB");
    assert_eq!(DatabaseType::SqlServer.to_string(), "SQL Server");
}

#[test]
fn test_database_schema_serde_roundtrip() {
    let db_info = DatabaseInfo::new("roundtrip_db".to_string());
    let schema = DatabaseSchema::new(db_info);

    let json = serde_json::to_string(&schema).expect("serialize");
    let deserialized: DatabaseSchema =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.format_version, "1.0");
    assert_eq!(deserialized.database_info.name, "roundtrip_db");
    assert!(deserialized.tables.is_empty());
    assert!(deserialized.samples.is_none());
}

#[test]
fn test_unified_data_type_all_variants_serde() {
    let types = vec![
        UnifiedDataType::String { max_length: Some(255) },
        UnifiedDataType::String { max_length: None },
        UnifiedDataType::Integer { bits: 32, signed: true },
        UnifiedDataType::Integer { bits: 64, signed: false },
        UnifiedDataType::Float { precision: Some(53) },
        UnifiedDataType::Float { precision: None },
        UnifiedDataType::Boolean,
        UnifiedDataType::DateTime { with_timezone: true },
        UnifiedDataType::DateTime { with_timezone: false },
        UnifiedDataType::Date,
        UnifiedDataType::Time { with_timezone: true },
        UnifiedDataType::Time { with_timezone: false },
        UnifiedDataType::Binary { max_length: Some(1024) },
        UnifiedDataType::Binary { max_length: None },
        UnifiedDataType::Json,
        UnifiedDataType::Uuid,
        UnifiedDataType::Array {
            element_type: Box::new(UnifiedDataType::Integer { bits: 32, signed: true }),
        },
        UnifiedDataType::Custom { type_name: "geometry".to_string() },
    ];

    for data_type in &types {
        let json = serde_json::to_string(data_type).expect("serialize");
        let deserialized: UnifiedDataType =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*data_type, deserialized);
    }
}

#[test]
fn test_nested_array_type_serde() {
    let nested = UnifiedDataType::Array {
        element_type: Box::new(UnifiedDataType::Array {
            element_type: Box::new(UnifiedDataType::String { max_length: None }),
        }),
    };

    let json = serde_json::to_string(&nested).expect("serialize");
    let deserialized: UnifiedDataType =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(nested, deserialized);
}

#[test]
fn test_object_count_with_objects() {
    let db_info = DatabaseInfo::new("test_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);

    schema.tables.push(Table {
        name: "users".to_string(),
        schema: Some("public".to_string()),
        columns: vec![Column {
            name: "id".to_string(),
            data_type: UnifiedDataType::Integer { bits: 32, signed: true },
            is_nullable: false,
            is_primary_key: true,
            is_auto_increment: true,
            default_value: None,
            comment: None,
            ordinal_position: 1,
        }],
        primary_key: Some(PrimaryKey {
            name: Some("users_pkey".to_string()),
            columns: vec!["id".to_string()],
        }),
        foreign_keys: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        comment: None,
        row_count: Some(100),
    });

    schema.views.push(View {
        name: "active_users".to_string(),
        schema: Some("public".to_string()),
        definition: Some("SELECT * FROM users".to_string()),
        columns: Vec::new(),
        comment: None,
    });

    schema.triggers.push(Trigger {
        name: "audit_trigger".to_string(),
        table_name: "users".to_string(),
        schema: Some("public".to_string()),
        event: TriggerEvent::Insert,
        timing: TriggerTiming::After,
        definition: None,
    });

    assert_eq!(schema.object_count(), 3);
}

#[test]
fn test_quality_metrics_management() {
    let db_info = DatabaseInfo::new("test_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);

    assert_eq!(schema.quality_metrics_count(), 0);

    schema.add_quality_metrics(Vec::new());
    assert_eq!(schema.quality_metrics_count(), 0);
}

#[test]
fn test_collection_status_variants_serde() {
    let success = CollectionStatus::Success;
    let json = serde_json::to_string(&success).expect("serialize");
    let deserialized: CollectionStatus =
        serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(deserialized, CollectionStatus::Success));

    let failed = CollectionStatus::Failed {
        error: "Connection refused".to_string(),
    };
    let json = serde_json::to_string(&failed).expect("serialize");
    let deserialized: CollectionStatus =
        serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(deserialized, CollectionStatus::Failed { .. }));

    let skipped = CollectionStatus::Skipped {
        reason: "Template database".to_string(),
    };
    let json = serde_json::to_string(&skipped).expect("serialize");
    let deserialized: CollectionStatus =
        serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(deserialized, CollectionStatus::Skipped { .. }));
}

#[test]
fn test_access_level_variants_serde() {
    for level in [AccessLevel::Full, AccessLevel::Limited, AccessLevel::None] {
        let json = serde_json::to_string(&level).expect("serialize");
        let _deserialized: AccessLevel =
            serde_json::from_str(&json).expect("deserialize");
    }
}

#[test]
fn test_referential_action_variants() {
    let actions = vec![
        ReferentialAction::Cascade,
        ReferentialAction::SetNull,
        ReferentialAction::SetDefault,
        ReferentialAction::Restrict,
        ReferentialAction::NoAction,
    ];

    for action in &actions {
        let json = serde_json::to_string(action).expect("serialize");
        let deserialized: ReferentialAction =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*action, deserialized);
    }
}

#[test]
fn test_collection_mode_variants_serde() {
    let single = CollectionMode::SingleDatabase;
    let json = serde_json::to_string(&single).expect("serialize");
    let _: CollectionMode =
        serde_json::from_str(&json).expect("deserialize");

    let multi = CollectionMode::MultiDatabase {
        discovered: 5,
        collected: 3,
        failed: 1,
    };
    let json = serde_json::to_string(&multi).expect("serialize");
    let deserialized: CollectionMode =
        serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(
        deserialized,
        CollectionMode::MultiDatabase { discovered: 5, collected: 3, failed: 1 }
    ));
}

#[test]
fn test_sampling_strategy_variants_serde() {
    let strategies = vec![
        SamplingStrategy::MostRecent { limit: 100 },
        SamplingStrategy::Random { limit: 50 },
        SamplingStrategy::None,
    ];

    for strategy in &strategies {
        let json = serde_json::to_string(strategy).expect("serialize");
        let _: SamplingStrategy =
            serde_json::from_str(&json).expect("deserialize");
    }
}

#[test]
fn test_ordering_strategy_variants_serde() {
    let strategies = vec![
        OrderingStrategy::PrimaryKey { columns: vec!["id".to_string()] },
        OrderingStrategy::Timestamp {
            column: "created_at".to_string(),
            direction: SortDirection::Descending,
        },
        OrderingStrategy::AutoIncrement { column: "seq".to_string() },
        OrderingStrategy::SystemRowId { column: "ctid".to_string() },
        OrderingStrategy::Unordered,
    ];

    for strategy in &strategies {
        let json = serde_json::to_string(strategy).expect("serialize");
        let _: OrderingStrategy =
            serde_json::from_str(&json).expect("deserialize");
    }
}

#[test]
fn test_foreign_key_construction() {
    let fk = ForeignKey {
        name: Some("fk_orders_users".to_string()),
        columns: vec!["user_id".to_string()],
        referenced_table: "users".to_string(),
        referenced_schema: Some("public".to_string()),
        referenced_columns: vec!["id".to_string()],
        on_delete: Some(ReferentialAction::Cascade),
        on_update: Some(ReferentialAction::NoAction),
    };

    let json = serde_json::to_string(&fk).expect("serialize");
    let deserialized: ForeignKey =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.name.unwrap(), "fk_orders_users");
    assert_eq!(deserialized.referenced_table, "users");
    assert_eq!(deserialized.on_delete, Some(ReferentialAction::Cascade));
}

#[test]
fn test_server_info_construction() {
    let server_info = ServerInfo {
        server_type: DatabaseType::PostgreSQL,
        version: "15.4".to_string(),
        host: "localhost".to_string(),
        port: Some(5432),
        total_databases: 5,
        collected_databases: 3,
        system_databases_excluded: 2,
        connection_user: "dbadmin".to_string(),
        has_superuser_privileges: true,
        collection_mode: CollectionMode::MultiDatabase {
            discovered: 5,
            collected: 3,
            failed: 0,
        },
    };

    let json = serde_json::to_string(&server_info).expect("serialize");
    assert!(!json.is_empty());
    let deserialized: ServerInfo =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.version, "15.4");
    assert_eq!(deserialized.total_databases, 5);
    assert!(deserialized.has_superuser_privileges);
}

#[test]
fn test_database_info_system_database() {
    let mut db_info = DatabaseInfo::new("template0".to_string());
    db_info.is_system_database = true;
    db_info.access_level = AccessLevel::None;
    db_info.collection_status = CollectionStatus::Skipped {
        reason: "System database".to_string(),
    };

    let json = serde_json::to_string(&db_info).expect("serialize");
    let deserialized: DatabaseInfo =
        serde_json::from_str(&json).expect("deserialize");

    assert!(deserialized.is_system_database);
    assert!(matches!(deserialized.access_level, AccessLevel::None));
    assert!(matches!(deserialized.collection_status, CollectionStatus::Skipped { .. }));
}

#[test]
fn test_full_schema_with_all_objects_serde() {
    let db_info = DatabaseInfo::new("full_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);

    schema.tables.push(Table {
        name: "t".to_string(),
        schema: None,
        columns: vec![Column {
            name: "c".to_string(),
            data_type: UnifiedDataType::Integer { bits: 32, signed: true },
            is_nullable: false,
            is_primary_key: false,
            is_auto_increment: false,
            default_value: Some("0".to_string()),
            comment: Some("A column".to_string()),
            ordinal_position: 1,
        }],
        primary_key: None,
        foreign_keys: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        comment: Some("A table".to_string()),
        row_count: None,
    });

    schema.custom_types.push(CustomType {
        name: "mood".to_string(),
        schema: Some("public".to_string()),
        definition: "ENUM('happy','sad')".to_string(),
        category: TypeCategory::Enum,
    });

    schema.procedures.push(Procedure {
        name: "my_proc".to_string(),
        schema: None,
        definition: None,
        parameters: vec![Parameter {
            name: "p1".to_string(),
            data_type: UnifiedDataType::String { max_length: None },
            direction: ParameterDirection::In,
            default_value: None,
        }],
        return_type: Some(UnifiedDataType::Boolean),
        language: Some("plpgsql".to_string()),
        comment: None,
    });

    let json = serde_json::to_string(&schema).expect("serialize");
    let deserialized: DatabaseSchema =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.tables.len(), 1);
    assert_eq!(deserialized.custom_types.len(), 1);
    assert_eq!(deserialized.procedures.len(), 1);
    assert_eq!(deserialized.object_count(), 3);
}

#[test]
fn test_database_type_serde_roundtrip() {
    let types = vec![
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
        DatabaseType::MongoDB,
        DatabaseType::SqlServer,
    ];

    for db_type in &types {
        let json = serde_json::to_string(db_type).expect("serialize");
        let deserialized: DatabaseType =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*db_type, deserialized);
    }
}

#[test]
fn test_index_with_columns() {
    let index = Index {
        name: "idx_users_email".to_string(),
        table_name: "users".to_string(),
        schema: Some("public".to_string()),
        columns: vec![
            IndexColumn {
                name: "email".to_string(),
                sort_order: Some(SortOrder::Ascending),
            },
            IndexColumn {
                name: "last_name".to_string(),
                sort_order: Some(SortOrder::Descending),
            },
        ],
        is_unique: true,
        is_primary: false,
        index_type: Some("btree".to_string()),
    };

    let json = serde_json::to_string(&index).expect("serialize");
    let deserialized: Index =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.columns.len(), 2);
    assert!(deserialized.is_unique);
    assert_eq!(deserialized.columns[0].sort_order, Some(SortOrder::Ascending));
    assert_eq!(deserialized.columns[1].sort_order, Some(SortOrder::Descending));
}
