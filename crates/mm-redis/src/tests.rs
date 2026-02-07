use super::*;

// Integration tests requiring Redis
// To run: docker run -d -p 6379:6379 redis:7-alpine
// Then: cargo test -p mm-redis -- --ignored

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_redis_connection() {
    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect to Redis");

    store.ping().await.expect("Redis ping failed");
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_store_and_retrieve() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestEvent {
        id: u32,
        value: String,
    }

    impl Versionable for TestEvent {
        fn schema_name() -> &'static str {
            "TestEvent"
        }
    }

    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect");

    let event = TestEvent {
        id: 42,
        value: "test".to_string(),
    };

    // Store with 10 second TTL
    store
        .store("test:event:42", event.clone(), 10)
        .await
        .expect("Failed to store");

    // Retrieve
    let retrieved: Option<TestEvent> = store
        .get("test:event:42")
        .await
        .expect("Failed to get");

    assert_eq!(retrieved, Some(event));

    // Cleanup
    store.delete("test:event:42").await.expect("Failed to delete");
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_time_range_queries() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TimeEvent {
        id: u32,
        time: f64,
    }

    impl Versionable for TimeEvent {
        fn schema_name() -> &'static str {
            "TimeEvent"
        }
    }

    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect");

    // Store events at different times
    for i in 0..5 {
        let event = TimeEvent {
            id: i,
            time: 1000.0 + (i as f64 * 10.0),
        };

        let key = format!("test:time:{}", i);
        store.store(&key, event.clone(), 60).await.expect("Failed to store");

        // Add to sorted set
        store
            .zadd("test:time_set", event.time, i.to_string())
            .await
            .expect("Failed to zadd");
    }

    // Query range
    let events: Vec<TimeEvent> = store
        .get_events_in_range("test:time_set", "test:time", 1010.0, 1030.0)
        .await
        .expect("Failed to get range");

    // Should get events 1, 2, 3 (times 1010, 1020, 1030)
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].id, 1);
    assert_eq!(events[2].id, 3);

    // Cleanup
    for i in 0..5 {
        let _ = store.delete(&format!("test:time:{}", i)).await;
    }
    let _: () = redis::cmd("DEL")
        .arg("test:time_set")
        .query_async(&mut store.conn_manager)
        .await
        .unwrap();
}
