//! MongoDB data sampling integration tests using testcontainers.

#![cfg(feature = "mongodb")]

use dbsurveyor_core::adapters::config::SamplingConfig;
use dbsurveyor_core::adapters::mongodb::MongoAdapter;
use dbsurveyor_core::models::SamplingStrategy;
use mongodb::bson::{doc, oid::ObjectId};
use testcontainers_modules::mongo::Mongo;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Helper to set up test data for sampling tests
async fn setup_sampling_test_data(connection_string: &str, num_docs: usize) {
    let client = mongodb::Client::with_uri_str(connection_string)
        .await
        .expect("Failed to connect to MongoDB");

    let db = client.database("testdb");
    let collection = db.collection::<mongodb::bson::Document>("items");

    let mut docs = Vec::with_capacity(num_docs);
    for i in 0..num_docs {
        docs.push(doc! {
            "_id": ObjectId::new(),
            "index": i as i32,
            "name": format!("Item {}", i),
            "value": (i as f64) * 1.5,
            "created_at": mongodb::bson::DateTime::now(),
        });
        // Small delay to ensure ObjectIds have different timestamps
        if i % 10 == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    collection
        .insert_many(docs)
        .await
        .expect("Failed to insert documents");
}

#[tokio::test]
async fn test_mongodb_sample_collection() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Set up test data with 50 documents
    setup_sampling_test_data(&connection_string, 50).await;

    // Create adapter and sample
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_collection("testdb", "items", &config)
        .await
        .expect("Failed to sample collection");

    // Verify sample
    assert_eq!(sample.table_name, "items");
    assert_eq!(sample.schema_name, Some("testdb".to_string()));
    assert_eq!(sample.sample_size, 10);
    assert_eq!(sample.rows.len(), 10);

    // Should be MostRecent strategy (using _id ordering)
    assert!(
        matches!(
            sample.sampling_strategy,
            SamplingStrategy::MostRecent { limit: 10 }
        ),
        "Expected MostRecent strategy"
    );

    // Verify rows are JSON objects
    for row in &sample.rows {
        assert!(row.is_object(), "Row should be an object");
        assert!(row.get("_id").is_some(), "Row should have _id");
        assert!(row.get("name").is_some(), "Row should have name");
        assert!(row.get("index").is_some(), "Row should have index");
    }
}

#[tokio::test]
async fn test_mongodb_sample_with_timestamp_ordering() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create documents with createdAt field
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    let collection = db.collection::<mongodb::bson::Document>("events");

    let mut docs = Vec::new();
    for i in 0..20 {
        docs.push(doc! {
            "_id": ObjectId::new(),
            "event_type": format!("event_{}", i),
            "createdAt": mongodb::bson::DateTime::now(),
        });
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    collection
        .insert_many(docs)
        .await
        .expect("Failed to insert");

    // Create adapter and detect ordering strategy
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let strategy = adapter
        .detect_ordering_strategy("testdb", "events")
        .await
        .expect("Failed to detect strategy");

    // Should detect createdAt as timestamp ordering
    match strategy {
        dbsurveyor_core::models::OrderingStrategy::Timestamp { column, .. } => {
            assert_eq!(column, "createdAt", "Should use createdAt for ordering");
        }
        _ => panic!("Expected Timestamp ordering strategy, got {:?}", strategy),
    }

    // Sample and verify
    let config = SamplingConfig::new().with_sample_size(5);
    let sample = adapter
        .sample_collection("testdb", "events", &config)
        .await
        .expect("Failed to sample");

    assert_eq!(sample.sample_size, 5);
}

#[tokio::test]
async fn test_mongodb_sample_small_collection() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create a small collection with only 3 documents
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    let collection = db.collection::<mongodb::bson::Document>("small");

    collection
        .insert_many(vec![
            doc! { "_id": ObjectId::new(), "value": 1 },
            doc! { "_id": ObjectId::new(), "value": 2 },
            doc! { "_id": ObjectId::new(), "value": 3 },
        ])
        .await
        .expect("Failed to insert");

    // Create adapter and sample more than available
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_collection("testdb", "small", &config)
        .await
        .expect("Failed to sample");

    // Should return only 3 rows (all available)
    assert_eq!(sample.sample_size, 3);
    assert_eq!(sample.rows.len(), 3);
}

#[tokio::test]
async fn test_mongodb_sample_empty_collection() {
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

    // Create adapter and sample
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_collection("testdb", "empty", &config)
        .await
        .expect("Failed to sample");

    // Should return 0 rows
    assert_eq!(sample.sample_size, 0);
    assert!(sample.rows.is_empty());
}

#[tokio::test]
async fn test_mongodb_sample_with_throttle() {
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
    setup_sampling_test_data(&connection_string, 20).await;

    // Create adapter and sample with throttle
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let config = SamplingConfig::new()
        .with_sample_size(5)
        .with_throttle_ms(50);

    let start = std::time::Instant::now();
    let sample = adapter
        .sample_collection("testdb", "items", &config)
        .await
        .expect("Failed to sample");
    let elapsed = start.elapsed();

    // Verify sample
    assert_eq!(sample.sample_size, 5);

    // Verify throttle was applied (should take at least 50ms)
    assert!(
        elapsed.as_millis() >= 50,
        "Throttle should add at least 50ms delay, but took {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn test_mongodb_sample_nested_documents() {
    let container = Mongo::default()
        .start()
        .await
        .expect("Failed to start MongoDB container");

    let port = container
        .get_host_port_ipv4(27017)
        .await
        .expect("Failed to get MongoDB port");

    let connection_string = format!("mongodb://localhost:{}/testdb", port);

    // Create documents with nested structure
    let client = mongodb::Client::with_uri_str(&connection_string)
        .await
        .expect("Failed to connect");

    let db = client.database("testdb");
    let collection = db.collection::<mongodb::bson::Document>("nested");

    collection
        .insert_many(vec![
            doc! {
                "_id": ObjectId::new(),
                "user": {
                    "name": "Alice",
                    "address": {
                        "city": "New York",
                        "country": "USA"
                    }
                }
            },
            doc! {
                "_id": ObjectId::new(),
                "user": {
                    "name": "Bob",
                    "address": {
                        "city": "London",
                        "country": "UK"
                    }
                }
            },
        ])
        .await
        .expect("Failed to insert");

    // Create adapter and sample
    let adapter = MongoAdapter::new(&connection_string)
        .await
        .expect("Failed to create adapter");

    let config = SamplingConfig::new().with_sample_size(2);

    let sample = adapter
        .sample_collection("testdb", "nested", &config)
        .await
        .expect("Failed to sample");

    // Verify nested structure is preserved in JSON
    for row in &sample.rows {
        assert!(row.get("user").is_some(), "Should have user field");
        let user = row.get("user").unwrap();
        assert!(user.get("name").is_some(), "Should have user.name");
        assert!(user.get("address").is_some(), "Should have user.address");
        let address = user.get("address").unwrap();
        assert!(
            address.get("city").is_some(),
            "Should have user.address.city"
        );
    }
}
