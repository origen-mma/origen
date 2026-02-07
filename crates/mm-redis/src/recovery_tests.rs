/// Integration tests for state recovery from Redis
///
/// These tests demonstrate that the application can:
/// 1. Persist events to Redis
/// 2. Restart and recover the state
/// 3. Continue processing with recovered state
use super::*;
use serde::{Deserialize, Serialize};

// Mock event types for testing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestGWEvent {
    simulation_id: u32,
    gpstime: f64,
    snr: f32,
}

impl Versionable for TestGWEvent {
    fn schema_name() -> &'static str {
        "TestGWEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestGRBEvent {
    simulation_id: u32,
    detection_time: f64,
    instrument: String,
}

impl Versionable for TestGRBEvent {
    fn schema_name() -> &'static str {
        "TestGRBEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestOpticalAlert {
    object_id: String,
    mjd: f64,
    ra: f64,
    dec: f64,
}

impl Versionable for TestOpticalAlert {
    fn schema_name() -> &'static str {
        "TestOpticalAlert"
    }
}

// Helper functions for floating-point tolerant comparisons
fn assert_gw_event_eq(left: &TestGWEvent, right: &TestGWEvent) {
    assert_eq!(left.simulation_id, right.simulation_id);
    assert!((left.gpstime - right.gpstime).abs() < 1e-6, "gpstime mismatch: {} vs {}", left.gpstime, right.gpstime);
    assert!((left.snr - right.snr).abs() < 1e-6, "snr mismatch: {} vs {}", left.snr, right.snr);
}

fn assert_grb_event_eq(left: &TestGRBEvent, right: &TestGRBEvent) {
    assert_eq!(left.simulation_id, right.simulation_id);
    assert!((left.detection_time - right.detection_time).abs() < 1e-6, "detection_time mismatch: {} vs {}", left.detection_time, right.detection_time);
    assert_eq!(left.instrument, right.instrument);
}

fn assert_optical_alert_eq(left: &TestOpticalAlert, right: &TestOpticalAlert) {
    assert_eq!(left.object_id, right.object_id);
    assert!((left.mjd - right.mjd).abs() < 1e-6, "mjd mismatch: {} vs {}", left.mjd, right.mjd);
    assert!((left.ra - right.ra).abs() < 1e-6, "ra mismatch: {} vs {}", left.ra, right.ra);
    assert!((left.dec - right.dec).abs() < 1e-6, "dec mismatch: {} vs {}", left.dec, right.dec);
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_basic_state_recovery() {
    // Clean setup
    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect to Redis");

    // Clear test data
    store
        .delete_keys(&["gw_events", "event:gw:100", "event:gw:101"])
        .await
        .unwrap();

    // === PHASE 1: Initial service run - store events ===

    // Use current Unix time for test events
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Simulate receiving two GW events
    let event1 = TestGWEvent {
        simulation_id: 100,
        gpstime: now - 3600.0, // 1 hour ago
        snr: 24.5,
    };
    let event2 = TestGWEvent {
        simulation_id: 101,
        gpstime: now - 1800.0, // 30 minutes ago
        snr: 18.3,
    };

    // Persist events (2 hour TTL)
    store
        .store("event:gw:100", event1.clone(), 7200)
        .await
        .expect("Failed to store event1");
    store
        .zadd("gw_events", event1.gpstime, "100".to_string())
        .await
        .expect("Failed to zadd event1");

    store
        .store("event:gw:101", event2.clone(), 7200)
        .await
        .expect("Failed to store event2");
    store
        .zadd("gw_events", event2.gpstime, "101".to_string())
        .await
        .expect("Failed to zadd event2");

    // Verify events are in Redis
    let stored1: Option<TestGWEvent> = store.get("event:gw:100").await.unwrap();
    assert_eq!(stored1, Some(event1.clone()));

    // === PHASE 2: Service restart - recover state ===

    // Create a NEW store instance (simulating restart)
    let mut new_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to reconnect to Redis");

    // Recover all GW events from last 2 hours
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let min_time = now - 7200.0;

    let recovered_ids = new_store
        .zrangebyscore("gw_events", min_time, f64::MAX)
        .await
        .unwrap();
    assert_eq!(recovered_ids.len(), 2);
    assert!(recovered_ids.contains(&"100".to_string()));
    assert!(recovered_ids.contains(&"101".to_string()));

    // Retrieve recovered events
    let mut recovered_events = Vec::new();
    for id in recovered_ids {
        let key = format!("event:gw:{}", id);
        if let Some(event) = new_store.get::<TestGWEvent>(&key).await.unwrap() {
            recovered_events.push(event);
        }
    }

    assert_eq!(recovered_events.len(), 2);
    assert!(recovered_events.contains(&event1));
    assert!(recovered_events.contains(&event2));

    // Cleanup
    new_store
        .delete_keys(&["gw_events", "event:gw:100", "event:gw:101"])
        .await
        .unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_multi_messenger_state_recovery() {
    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect to Redis");

    // Clear test data
    store
        .delete_keys(&[
            "gw_events",
            "grb_events",
            "optical_events",
            "event:gw:200",
            "event:grb:200",
            "event:optical:ZTF24test",
        ])
        .await
        .unwrap();

    // === PHASE 1: Store correlated events ===

    // Use current Unix time for test events
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // GW event
    let gw_event = TestGWEvent {
        simulation_id: 200,
        gpstime: now - 3600.0, // 1 hour ago
        snr: 30.2,
    };
    store
        .store("event:gw:200", gw_event.clone(), 7200)
        .await
        .unwrap();
    store
        .zadd("gw_events", gw_event.gpstime, "200".to_string())
        .await
        .unwrap();

    // GRB event (correlated by simulation_id)
    let grb_event = TestGRBEvent {
        simulation_id: 200,
        detection_time: now - 3597.5, // 2.5 seconds after GW
        instrument: "Fermi-GBM".to_string(),
    };
    store
        .store("event:grb:200", grb_event.clone(), 7200)
        .await
        .unwrap();
    store
        .zadd("grb_events", grb_event.detection_time, "200".to_string())
        .await
        .unwrap();

    // Optical alert (within 1 day)
    let optical_alert = TestOpticalAlert {
        object_id: "ZTF24test".to_string(),
        mjd: now - 1800.0, // 30 minutes ago
        ra: 123.456,
        dec: -45.123,
    };
    store
        .store("event:optical:ZTF24test", optical_alert.clone(), 86400)
        .await
        .unwrap();
    store
        .zadd("optical_events", optical_alert.mjd, "ZTF24test".to_string())
        .await
        .unwrap();

    // === PHASE 2: Service restart - recover all event types ===

    let mut new_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to reconnect");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let min_time = now - 7200.0;

    // Recover GW events
    let gw_ids = new_store
        .zrangebyscore("gw_events", min_time, f64::MAX)
        .await
        .unwrap();
    assert_eq!(gw_ids.len(), 1);
    let recovered_gw: Option<TestGWEvent> = new_store.get("event:gw:200").await.unwrap();
    assert!(recovered_gw.is_some());
    assert_gw_event_eq(recovered_gw.as_ref().unwrap(), &gw_event);

    // Recover GRB events
    let grb_ids = new_store
        .zrangebyscore("grb_events", min_time, f64::MAX)
        .await
        .unwrap();
    assert_eq!(grb_ids.len(), 1);
    let recovered_grb: Option<TestGRBEvent> = new_store.get("event:grb:200").await.unwrap();
    assert!(recovered_grb.is_some());
    assert_grb_event_eq(recovered_grb.as_ref().unwrap(), &grb_event);

    // Recover optical alerts (MJD range)
    let mjd_min = (min_time / 86400.0) + 40587.0; // Unix epoch in MJD
    let optical_ids = new_store
        .zrangebyscore("optical_events", mjd_min, f64::MAX)
        .await
        .unwrap();
    assert_eq!(optical_ids.len(), 1);
    let recovered_optical: Option<TestOpticalAlert> =
        new_store.get("event:optical:ZTF24test").await.unwrap();
    assert!(recovered_optical.is_some());
    assert_optical_alert_eq(recovered_optical.as_ref().unwrap(), &optical_alert);

    // === PHASE 3: Verify correlation can still be computed with recovered state ===
    // This demonstrates that correlations work after restart
    assert_eq!(
        recovered_gw.unwrap().simulation_id,
        recovered_grb.unwrap().simulation_id
    );

    // Cleanup
    new_store
        .delete_keys(&[
            "gw_events",
            "grb_events",
            "optical_events",
            "event:gw:200",
            "event:grb:200",
            "event:optical:ZTF24test",
        ])
        .await
        .unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_partial_recovery_with_expired_events() {
    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect");

    // Clear test data
    store
        .delete_keys(&["gw_events", "event:gw:300", "event:gw:301"])
        .await
        .unwrap();

    // Use current Unix time for test events
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Store event with short TTL (will expire)
    let expired_event = TestGWEvent {
        simulation_id: 300,
        gpstime: now - 3600.0,
        snr: 15.0,
    };
    store
        .store("event:gw:300", expired_event.clone(), 1) // 1 second TTL
        .await
        .unwrap();
    store
        .zadd("gw_events", expired_event.gpstime, "300".to_string())
        .await
        .unwrap();

    // Store event with long TTL (will persist)
    let persisted_event = TestGWEvent {
        simulation_id: 301,
        gpstime: now - 1800.0,
        snr: 22.0,
    };
    store
        .store("event:gw:301", persisted_event.clone(), 7200)
        .await
        .unwrap();
    store
        .zadd("gw_events", persisted_event.gpstime, "301".to_string())
        .await
        .unwrap();

    // Wait for first event to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // === Recover state ===
    let mut new_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to reconnect");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let min_time = now - 7200.0;

    // Get all IDs from sorted set
    let all_ids = new_store
        .zrangebyscore("gw_events", min_time, f64::MAX)
        .await
        .unwrap();

    // Both IDs might still be in sorted set (not cleaned up automatically)
    // But only the non-expired one will have a retrievable value
    let mut recovered_events = Vec::new();
    for id in all_ids {
        let key = format!("event:gw:{}", id);
        if let Some(event) = new_store.get::<TestGWEvent>(&key).await.unwrap() {
            recovered_events.push(event);
        }
    }

    // Only the persisted event should be recovered
    assert_eq!(recovered_events.len(), 1);
    assert_gw_event_eq(&recovered_events[0], &persisted_event);

    // Cleanup
    new_store
        .delete_keys(&["gw_events", "event:gw:300", "event:gw:301"])
        .await
        .unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_recovery_time_window_filtering() {
    let mut store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect");

    // Clear test data
    store
        .delete_keys(&["gw_events", "event:gw:400", "event:gw:401", "event:gw:402"])
        .await
        .unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Event from 3 hours ago (outside 2-hour window)
    let old_event = TestGWEvent {
        simulation_id: 400,
        gpstime: now - 10800.0, // 3 hours ago
        snr: 10.0,
    };

    // Event from 1 hour ago (inside 2-hour window)
    let recent_event = TestGWEvent {
        simulation_id: 401,
        gpstime: now - 3600.0, // 1 hour ago
        snr: 20.0,
    };

    // Event from 5 minutes ago (inside 2-hour window)
    let newest_event = TestGWEvent {
        simulation_id: 402,
        gpstime: now - 300.0, // 5 minutes ago
        snr: 25.0,
    };

    // Store all events
    for event in [&old_event, &recent_event, &newest_event] {
        let key = format!("event:gw:{}", event.simulation_id);
        store.store(&key, event.clone(), 14400).await.unwrap(); // 4 hour TTL
        store
            .zadd("gw_events", event.gpstime, event.simulation_id.to_string())
            .await
            .unwrap();
    }

    // === Recover with 2-hour lookback window ===
    let mut new_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to reconnect");

    let lookback_seconds = 7200.0; // 2 hours
    let min_time = now - lookback_seconds;

    let recovered_ids = new_store
        .zrangebyscore("gw_events", min_time, f64::MAX)
        .await
        .unwrap();

    // Should only get events within 2-hour window
    assert_eq!(recovered_ids.len(), 2);
    assert!(recovered_ids.contains(&"401".to_string()));
    assert!(recovered_ids.contains(&"402".to_string()));
    assert!(!recovered_ids.contains(&"400".to_string()));

    // Cleanup
    new_store
        .delete_keys(&["gw_events", "event:gw:400", "event:gw:401", "event:gw:402"])
        .await
        .unwrap();
}
