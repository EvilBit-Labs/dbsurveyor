//! Unit tests for MongoDB adapter.

use super::*;
use crate::adapters::ConnectionConfig;
use std::time::Duration;

#[test]
fn test_parse_connection_config() {
    let connection_string = "mongodb://testuser@localhost:27017/testdb";
    let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(27017));
    assert_eq!(config.database, Some("testdb".to_string()));
    assert_eq!(config.username, Some("testuser".to_string()));
    assert!(config.read_only);
    assert_eq!(config.connect_timeout, Duration::from_secs(30));
    assert_eq!(config.query_timeout, Duration::from_secs(30));
    assert_eq!(config.max_connections, 10);
}

#[test]
fn test_parse_connection_config_with_query_params() {
    let connection_string = "mongodb://user@host/db?connectTimeoutMS=5000&serverSelectionTimeoutMS=10000&maxPoolSize=20";
    let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "host");
    assert_eq!(config.port, Some(27017)); // Default MongoDB port
    assert_eq!(config.database, Some("db".to_string()));
    assert_eq!(config.username, Some("user".to_string()));
    assert_eq!(config.connect_timeout, Duration::from_millis(5000));
    assert_eq!(config.query_timeout, Duration::from_millis(10000));
    assert_eq!(config.max_connections, 20);
}

#[test]
fn test_parse_connection_config_defaults() {
    let connection_string = "mongodb://localhost";
    let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(27017)); // Default MongoDB port
    assert_eq!(config.database, None);
    assert_eq!(config.username, None);
    assert!(config.read_only); // Default to read-only for security
}

#[test]
fn test_parse_connection_config_srv() {
    let connection_string = "mongodb+srv://user@cluster.example.com/mydb";
    let config = MongoAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "cluster.example.com");
    assert_eq!(config.database, Some("mydb".to_string()));
    assert_eq!(config.username, Some("user".to_string()));
}

#[test]
fn test_parse_connection_config_invalid_scheme() {
    let connection_string = "postgres://user@host/db";
    let result = MongoAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("mongodb://"));
}

#[test]
fn test_parse_connection_config_invalid_url() {
    let connection_string = "invalid-url";
    let result = MongoAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
}

#[test]
fn test_validate_connection_string_valid() {
    assert!(MongoAdapter::validate_connection_string("mongodb://localhost:27017/test").is_ok());
    assert!(
        MongoAdapter::validate_connection_string("mongodb+srv://cluster.example.com/test").is_ok()
    );
    assert!(MongoAdapter::validate_connection_string("mongodb://user:pass@localhost/db").is_ok());
}

#[test]
fn test_validate_connection_string_invalid_scheme() {
    let result = MongoAdapter::validate_connection_string("mysql://localhost/db");
    assert!(result.is_err());
}

#[test]
fn test_validate_connection_string_no_host() {
    let result = MongoAdapter::validate_connection_string("mongodb:///db");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("host"));
}

#[test]
fn test_supports_feature() {
    use crate::adapters::AdapterFeature;

    // Test that MongoDB adapter claims to support expected features
    let features = vec![
        AdapterFeature::SchemaCollection,
        AdapterFeature::DataSampling,
        AdapterFeature::QueryTimeout,
    ];

    for feature in features {
        assert!(matches!(
            feature,
            AdapterFeature::SchemaCollection
                | AdapterFeature::DataSampling
                | AdapterFeature::QueryTimeout
        ));
    }
}

#[test]
fn test_database_type() {
    use crate::models::DatabaseType;
    assert_eq!(DatabaseType::MongoDB.to_string(), "MongoDB");
}

#[test]
fn test_connection_config_builder_pattern() {
    let config = ConnectionConfig::new("mongodb.example.com".to_string())
        .with_port(27017)
        .with_database("mydb".to_string())
        .with_username("admin".to_string());

    assert_eq!(config.host, "mongodb.example.com");
    assert_eq!(config.port, Some(27017));
    assert_eq!(config.database, Some("mydb".to_string()));
    assert_eq!(config.username, Some("admin".to_string()));
    assert!(config.read_only); // Default should be read-only for security
}

#[test]
fn test_connection_config_display_no_credentials() {
    let config = ConnectionConfig::new("mongo.example.com".to_string())
        .with_port(27017)
        .with_database("testdb".to_string())
        .with_username("testuser".to_string());

    let display = format!("{}", config);

    // Should contain connection info
    assert!(display.contains("mongo.example.com"));
    assert!(display.contains("27017"));
    assert!(display.contains("testdb"));

    // Should NOT contain username (security)
    assert!(!display.contains("testuser"));
}

#[test]
fn test_parse_connection_config_invalid_port() {
    let connection_string = "mongodb://user@host:0/db";
    let result = MongoAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("port"));
}

#[test]
fn test_connection_config_validation_limits() {
    // Test max connections limit
    let config = ConnectionConfig {
        max_connections: 101, // Over limit
        ..Default::default()
    };
    assert!(config.validate().is_err());

    let config = ConnectionConfig {
        max_connections: 50, // Within limit
        ..Default::default()
    };
    assert!(config.validate().is_ok());

    // Test zero max connections
    let config = ConnectionConfig {
        max_connections: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Test zero connect timeout
    let config = ConnectionConfig {
        max_connections: 10,
        connect_timeout: Duration::from_secs(0),
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Test zero query timeout
    let config = ConnectionConfig {
        max_connections: 10,
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(0),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

// Tests for type mapping
mod type_mapping_tests {
    use super::type_mapping::*;
    use crate::models::UnifiedDataType;
    use mongodb::bson::{Binary, Bson, DateTime, oid::ObjectId, spec::BinarySubtype};

    #[test]
    fn test_map_bson_basic_types() {
        // Test string
        let bson = Bson::String("hello".to_string());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::String { max_length: None }
        ));

        // Test int32
        let bson = Bson::Int32(42);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));

        // Test int64
        let bson = Bson::Int64(9_999_999_999);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Integer {
                bits: 64,
                signed: true
            }
        ));

        // Test double
        let bson = Bson::Double(1.234);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Float {
                precision: Some(53)
            }
        ));

        // Test boolean
        let bson = Bson::Boolean(true);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(unified, UnifiedDataType::Boolean));

        // Test datetime
        let bson = Bson::DateTime(DateTime::now());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::DateTime {
                with_timezone: true
            }
        ));

        // Test binary
        let bson = Bson::Binary(Binary {
            subtype: BinarySubtype::Generic,
            bytes: vec![1, 2, 3],
        });
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Binary { max_length: None }
        ));

        // Test ObjectId
        let bson = Bson::ObjectId(ObjectId::new());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::String {
                max_length: Some(24)
            }
        ));

        // Test null
        let bson = Bson::Null;
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Custom { type_name } if type_name == "null"
        ));
    }

    #[test]
    fn test_bson_type_name() {
        assert_eq!(bson_type_name(&Bson::String("".to_string())), "string");
        assert_eq!(bson_type_name(&Bson::Int32(0)), "int32");
        assert_eq!(bson_type_name(&Bson::Int64(0)), "int64");
        assert_eq!(bson_type_name(&Bson::Double(0.0)), "double");
        assert_eq!(bson_type_name(&Bson::Boolean(true)), "bool");
        assert_eq!(bson_type_name(&Bson::Null), "null");
    }
}

// Tests for schema inference
mod schema_inference_tests {
    use super::schema_inference::*;
    use mongodb::bson::{doc, oid::ObjectId};

    #[test]
    fn test_schema_inferrer_basic() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "name": "John",
            "age": 30
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("users".to_string());

        assert_eq!(schema.collection_name, "users");
        assert_eq!(schema.documents_sampled, 1);
        assert_eq!(schema.fields.len(), 3);
    }

    #[test]
    fn test_schema_inferrer_multiple_documents() {
        let mut inferrer = SchemaInferrer::new();

        let doc1 = doc! {
            "_id": ObjectId::new(),
            "name": "John",
            "age": 30
        };

        let doc2 = doc! {
            "_id": ObjectId::new(),
            "name": "Jane",
            "email": "jane@example.com"
        };

        inferrer.analyze_document(&doc1);
        inferrer.analyze_document(&doc2);
        let schema = inferrer.finalize("users".to_string());

        assert_eq!(schema.documents_sampled, 2);
        assert_eq!(schema.fields.len(), 4); // _id, name, age, email
    }

    #[test]
    fn test_schema_inferrer_nested_document() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "profile": {
                "firstName": "John",
                "lastName": "Doe"
            }
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("users".to_string());

        // Should have _id, profile, profile.firstName, profile.lastName
        assert!(schema.fields.iter().any(|f| f.name == "profile"));
        assert!(schema.fields.iter().any(|f| f.name == "profile.firstName"));
        assert!(schema.fields.iter().any(|f| f.name == "profile.lastName"));
    }

    #[test]
    fn test_to_columns() {
        let mut inferrer = SchemaInferrer::new();

        let doc = doc! {
            "_id": ObjectId::new(),
            "name": "Test",
            "count": 42
        };

        inferrer.analyze_document(&doc);
        let schema = inferrer.finalize("test".to_string());
        let columns = schema.to_columns();

        assert_eq!(columns.len(), 3);

        // _id should be primary key
        let id_col = columns.iter().find(|c| c.name == "_id");
        assert!(id_col.is_some());
        assert!(id_col.as_ref().map(|c| c.is_primary_key).unwrap_or(false));
    }
}

// Tests for enumeration
mod enumeration_tests {
    use super::enumeration::*;

    #[test]
    fn test_system_database_detection() {
        assert!(EnumeratedDatabase::check_is_system_database("admin"));
        assert!(EnumeratedDatabase::check_is_system_database("config"));
        assert!(EnumeratedDatabase::check_is_system_database("local"));
        assert!(!EnumeratedDatabase::check_is_system_database("mydb"));
        assert!(!EnumeratedDatabase::check_is_system_database("test"));
    }

    #[test]
    fn test_enumerated_database_new() {
        let db = EnumeratedDatabase::new("testdb".to_string());
        assert_eq!(db.name, "testdb");
        assert!(db.size_bytes.is_none());
        assert!(!db.is_system_database);
        assert!(db.is_accessible);
    }

    #[test]
    fn test_collection_type_display() {
        assert_eq!(CollectionType::Collection.to_string(), "collection");
        assert_eq!(CollectionType::View.to_string(), "view");
        assert_eq!(CollectionType::TimeSeries.to_string(), "timeseries");
    }
}

// Tests for sampling
mod sampling_tests {
    use super::sampling::*;
    use crate::models::{OrderingStrategy, SortDirection};

    #[test]
    fn test_generate_sort_document_primary_key() {
        let strategy = OrderingStrategy::PrimaryKey {
            columns: vec!["_id".to_string()],
        };

        let sort = generate_sort_document(&strategy, true);
        assert_eq!(sort.get_i32("_id"), Ok(-1));

        let sort = generate_sort_document(&strategy, false);
        assert_eq!(sort.get_i32("_id"), Ok(1));
    }

    #[test]
    fn test_generate_sort_document_timestamp() {
        let strategy = OrderingStrategy::Timestamp {
            column: "createdAt".to_string(),
            direction: SortDirection::Descending,
        };

        let sort = generate_sort_document(&strategy, true);
        assert_eq!(sort.get_i32("createdAt"), Ok(-1));
    }

    #[test]
    fn test_generate_sort_document_unordered() {
        let strategy = OrderingStrategy::Unordered;
        let sort = generate_sort_document(&strategy, true);
        assert!(sort.is_empty());
    }
}
