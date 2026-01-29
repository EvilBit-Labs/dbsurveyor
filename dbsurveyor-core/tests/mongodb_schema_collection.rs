//! MongoDB schema collection integration tests using testcontainers.

#![cfg(feature = "mongodb")]

use dbsurveyor_core::adapters::DatabaseAdapter;
use dbsurveyor_core::adapters::mongodb::MongoAdapter;
use mongodb::bson::{doc, oid::ObjectId};
use testcontainers_modules::mongo::Mongo;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Helper to set up test data in MongoDB
async fn setup_test_data(connection_string: &str) {
    let client = mongodb::Client::with_uri_str(connection_string)
        .await
        .expect("Failed to connect to MongoDB");

    let db = client.database("testdb");

    // Create users collection with various data types
    let users = db.collection::<mongodb::bson::Document>("users");
    users
        .insert_many(vec![
            doc! {
                "_id": ObjectId::new(),
                "name": "Alice",
                "email": "alice@example.com",
                "age": 30,
                "active": true,
                "balance": 1234.56,
                "created_at": mongodb::bson::DateTime::now(),
                "tags": ["admin", "user"],
                "profile": {
                    "bio": "Software engineer",
                    "avatar_url": "https://example.com/alice.jpg"
                }
            },
            doc! {
                "_id": ObjectId::new(),
                "name": "Bob",
                "email": "bob@example.com",
                "age": 25,
                "active": false,
                "balance": 5678.90,
                "created_at": mongodb::bson::DateTime::now(),
                "tags": ["user"],
                "profile": {
                    "bio": "Data scientist",
                    "avatar_url": mongodb::bson::Bson::Null
                }
            },
            doc! {
                "_id": ObjectId::new(),
                "name": "Charlie",
                "email": "charlie@example.com",
                "age": 35,
                // Note: no active field to test optional fields
                "balance": 9999.99,
                "created_at": mongodb::bson::DateTime::now(),
                "tags": []
            },
        ])
        .await
        .expect("Failed to insert users");

    // Create products collection
    let products = db.collection::<mongodb::bson::Document>("products");
    products
        .insert_many(vec![
            doc! {
                "_id": ObjectId::new(),
                "name": "Widget",
                "price": 19.99,
                "quantity": 100,
                "categories": ["electronics", "gadgets"],
                "metadata": {
                    "weight": 0.5,
                    "dimensions": {
                        "length": 10,
                        "width": 5,
                        "height": 2
                    }
                }
            },
            doc! {
                "_id": ObjectId::new(),
                "name": "Gadget",
                "price": 49.99,
                "quantity": 50,
                "categories": ["electronics"],
                "metadata": {
                    "weight": 1.2,
                    "dimensions": {
                        "length": 15,
                        "width": 8,
                        "height": 4
                    }
                }
            },
        ])
        .await
        .expect("Failed to insert products");

    // Create an index on users collection
    users
        .create_index(
            mongodb::IndexModel::builder()
                .keys(doc! { "email": 1 })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .name("email_unique".to_string())
                        .build(),
                )
                .build(),
        )
        .await
        .expect("Failed to create index");
}

#[tokio::test]
async fn test_mongodb_schema_collection() {
    // Start MongoDB container
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Set up test data
    setup_test_data(&connection_string).await;

    // Create adapter and collect schema
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create MongoDB adapter");

    let schema = adapter
        .collect_schema()
        .await
        .expect("Failed to collect schema");

    // Verify database info
    assert_eq!(schema.database_info.name, "testdb");
    assert!(!schema.database_info.is_system_database);

    // Verify collections were found
    assert!(schema.tables.len() >= 2, "Expected at least 2 collections");

    // Find users collection
    let users_table = schema.tables.iter().find(|t| t.name == "users");
    assert!(users_table.is_some(), "users collection not found");
    let users = users_table.unwrap();

    // Verify columns were inferred
    assert!(!users.columns.is_empty(), "No columns inferred for users");

    // Check for _id field
    let id_col = users.columns.iter().find(|c| c.name == "_id");
    assert!(id_col.is_some(), "_id column not found");
    assert!(id_col.unwrap().is_primary_key, "_id should be primary key");

    // Check for other expected fields
    assert!(
        users.columns.iter().any(|c| c.name == "name"),
        "name column not found"
    );
    assert!(
        users.columns.iter().any(|c| c.name == "email"),
        "email column not found"
    );
    assert!(
        users.columns.iter().any(|c| c.name == "age"),
        "age column not found"
    );
    assert!(
        users.columns.iter().any(|c| c.name == "tags"),
        "tags column not found"
    );

    // Check for nested fields
    assert!(
        users.columns.iter().any(|c| c.name == "profile"),
        "profile column not found"
    );

    // Find products collection
    let products_table = schema.tables.iter().find(|t| t.name == "products");
    assert!(products_table.is_some(), "products collection not found");

    // Verify indexes were collected
    let users_indexes: Vec<_> = schema
        .indexes
        .iter()
        .filter(|i| i.table_name == "users")
        .collect();
    assert!(!users_indexes.is_empty(), "No indexes found for users");

    // Check for _id index
    assert!(
        users_indexes.iter().any(|i| i.name == "_id_"),
        "_id_ index not found"
    );

    // Check for email unique index
    assert!(
        users_indexes.iter().any(|i| i.name == "email_unique"),
        "email_unique index not found"
    );
}

#[tokio::test]
async fn test_mongodb_schema_inference_nested_documents() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create a collection with deeply nested documents
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    let nested = db.collection::<mongodb::bson::Document>("nested");
    nested
        .insert_one(doc! {
            "_id": ObjectId::new(),
            "level1": {
                "level2": {
                    "level3": {
                        "value": "deep"
                    }
                }
            }
        })
        .await
        .expect("Failed to insert");

    // Create adapter and collect schema
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let schema = adapter
        .collect_schema()
        .await
        .expect("Failed to collect schema");

    let nested_table = schema.tables.iter().find(|t| t.name == "nested");
    assert!(nested_table.is_some(), "nested collection not found");

    let nested = nested_table.unwrap();

    // Check that nested fields are discovered
    assert!(
        nested.columns.iter().any(|c| c.name == "level1"),
        "level1 not found"
    );
    assert!(
        nested.columns.iter().any(|c| c.name == "level1.level2"),
        "level1.level2 not found"
    );
    assert!(
        nested
            .columns
            .iter()
            .any(|c| c.name == "level1.level2.level3"),
        "level1.level2.level3 not found"
    );
    assert!(
        nested
            .columns
            .iter()
            .any(|c| c.name == "level1.level2.level3.value"),
        "level1.level2.level3.value not found"
    );
}

#[tokio::test]
async fn test_mongodb_schema_inference_mixed_types() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create a collection with mixed types for the same field
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    let mixed = db.collection::<mongodb::bson::Document>("mixed");
    mixed
        .insert_many(vec![
            doc! { "_id": ObjectId::new(), "value": 42 },
            doc! { "_id": ObjectId::new(), "value": "forty-two" },
            doc! { "_id": ObjectId::new(), "value": 42.0 },
        ])
        .await
        .expect("Failed to insert");

    // Create adapter and collect schema
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let schema = adapter
        .collect_schema()
        .await
        .expect("Failed to collect schema");

    let mixed_table = schema.tables.iter().find(|t| t.name == "mixed");
    assert!(mixed_table.is_some(), "mixed collection not found");

    let mixed = mixed_table.unwrap();
    let value_col = mixed.columns.iter().find(|c| c.name == "value");
    assert!(value_col.is_some(), "value column not found");

    // The comment should indicate mixed types
    let comment = value_col.unwrap().comment.as_ref();
    if let Some(c) = comment {
        assert!(c.contains("Mixed"), "Should indicate mixed types: {}", c);
    }
}

#[tokio::test]
async fn test_mongodb_empty_collection() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create an empty collection
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    db.create_collection("empty")
        .await
        .expect("Failed to create collection");

    // Create adapter and collect schema
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let schema = adapter
        .collect_schema()
        .await
        .expect("Failed to collect schema");

    let empty_table = schema.tables.iter().find(|t| t.name == "empty");
    assert!(empty_table.is_some(), "empty collection not found");

    let empty = empty_table.unwrap();
    // Empty collections should have no inferred columns
    assert!(
        empty.columns.is_empty(),
        "Empty collection should have no columns"
    );
}

#[tokio::test]
async fn test_mongodb_list_databases() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create some databases by inserting data
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    client
        .database("db1")
        .collection::<mongodb::bson::Document>("test")
        .insert_one(doc! { "key": "value" })
        .await
        .expect("Failed to insert");

    client
        .database("db2")
        .collection::<mongodb::bson::Document>("test")
        .insert_one(doc! { "key": "value" })
        .await
        .expect("Failed to insert");

    // Create adapter and list databases
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    // List without system databases
    let databases = adapter
        .list_databases(false)
        .await
        .expect("Failed to list databases");
    assert!(databases.iter().any(|d| d.name == "db1"), "db1 not found");
    assert!(databases.iter().any(|d| d.name == "db2"), "db2 not found");

    // System databases should be excluded
    assert!(
        !databases.iter().any(|d| d.name == "admin"),
        "admin should be excluded"
    );
    assert!(
        !databases.iter().any(|d| d.name == "config"),
        "config should be excluded"
    );
    assert!(
        !databases.iter().any(|d| d.name == "local"),
        "local should be excluded"
    );

    // List with system databases
    let all_databases = adapter
        .list_databases(true)
        .await
        .expect("Failed to list databases");
    assert!(
        all_databases.iter().any(|d| d.name == "admin"),
        "admin should be included"
    );
}

#[tokio::test]
async fn test_mongodb_list_collections() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Set up test data
    setup_test_data(&connection_string).await;

    // Create adapter and list collections
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let collections = adapter
        .list_collections("testdb")
        .await
        .expect("Failed to list collections");

    assert!(
        collections.iter().any(|c| c.name == "users"),
        "users collection not found"
    );
    assert!(
        collections.iter().any(|c| c.name == "products"),
        "products collection not found"
    );

    // Check collection metadata
    let users = collections.iter().find(|c| c.name == "users").unwrap();
    assert!(
        users.document_count.is_some(),
        "Document count should be available"
    );
    assert_eq!(
        users.document_count.unwrap(),
        3,
        "Users should have 3 documents"
    );
}
