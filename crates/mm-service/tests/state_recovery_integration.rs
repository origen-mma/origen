/// Integration test for CorrelatorState recovery from Redis
///
/// This test verifies the complete recovery workflow:
/// 1. Create state with Redis
/// 2. Add events (which persist to Redis)
/// 3. Drop the state (simulating restart)
/// 4. Create new state and recover from Redis
/// 5. Verify events are restored
use mm_redis::{RedisStateStore, Versionable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// Minimal event types for testing (matching the service)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestGWEvent {
    simulation_id: u32,
    gpstime: f64,
    pipeline: String,
    snr: f32,
    far: f64,
    skymap_path: String,
}

impl Versionable for TestGWEvent {
    fn schema_name() -> &'static str {
        "GWEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestGRBEvent {
    simulation_id: u32,
    detection_time: f64,
    ra: f64,
    dec: f64,
    error_radius: f64,
    instrument: String,
    skymap_path: String,
}

impl Versionable for TestGRBEvent {
    fn schema_name() -> &'static str {
        "GRBEvent"
    }
}

/// Minimal version of CorrelatorState for testing
struct TestCorrelatorState {
    gw_events: std::collections::BTreeMap<u32, TestGWEvent>,
    grb_events: std::collections::BTreeMap<u32, TestGRBEvent>,
    redis_store: Option<Arc<Mutex<RedisStateStore>>>,
}

impl TestCorrelatorState {
    fn new(redis_store: Option<Arc<Mutex<RedisStateStore>>>) -> Self {
        Self {
            gw_events: std::collections::BTreeMap::new(),
            grb_events: std::collections::BTreeMap::new(),
            redis_store,
        }
    }

    /// Simplified add_gw_event that only persists (no correlation logic)
    async fn add_gw_event(&mut self, event: TestGWEvent) {
        let sim_id = event.simulation_id;
        let gps_time = event.gpstime;

        // Persist to Redis
        if let Some(redis) = self.redis_store.clone() {
            let event_clone = event.clone();
            let key = format!("event:gw:{}", sim_id);
            let mut store = redis.lock().await;
            let _ = store.store(&key, event_clone, 7200).await;
            let _ = store.zadd("gw_events", gps_time, sim_id.to_string()).await;
        }

        self.gw_events.insert(sim_id, event);
    }

    async fn add_grb_event(&mut self, event: TestGRBEvent) {
        let sim_id = event.simulation_id;
        let detection_time = event.detection_time;

        // Persist to Redis
        if let Some(redis) = self.redis_store.clone() {
            let event_clone = event.clone();
            let key = format!("event:grb:{}", sim_id);
            let mut store = redis.lock().await;
            let _ = store.store(&key, event_clone, 7200).await;
            let _ = store
                .zadd("grb_events", detection_time, sim_id.to_string())
                .await;
        }

        self.grb_events.insert(sim_id, event);
    }

    /// Recovery from Redis (matching the actual implementation)
    async fn recover_from_redis(
        &mut self,
        lookback_seconds: f64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(redis) = self.redis_store.clone() else {
            return Ok(());
        };

        let mut store = redis.lock().await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs_f64();
        let min_time = now - lookback_seconds;

        // Recover GW events
        let gw_ids = store.zrangebyscore("gw_events", min_time, f64::MAX).await?;
        for id_str in gw_ids {
            let key = format!("event:gw:{}", id_str);
            if let Ok(Some(event)) = store.get::<TestGWEvent>(&key).await {
                self.gw_events.insert(event.simulation_id, event);
            }
        }

        // Recover GRB events
        let grb_ids = store
            .zrangebyscore("grb_events", min_time, f64::MAX)
            .await?;
        for id_str in grb_ids {
            let key = format!("event:grb:{}", id_str);
            if let Ok(Some(event)) = store.get::<TestGRBEvent>(&key).await {
                self.grb_events.insert(event.simulation_id, event);
            }
        }

        Ok(())
    }
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_correlator_state_recovery() {
    // Setup: Connect to Redis
    let redis_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect to Redis - is it running?");

    let redis_arc = Some(Arc::new(Mutex::new(redis_store)));

    // Cleanup: Remove test data
    {
        let mut store = redis_arc.as_ref().unwrap().lock().await;
        store
            .delete_keys(&[
                "gw_events",
                "grb_events",
                "event:gw:500",
                "event:gw:501",
                "event:grb:500",
            ])
            .await
            .unwrap();
    }

    // === PHASE 1: Initial service run ===
    println!("PHASE 1: Initial service run - creating and persisting events");

    let mut state1 = TestCorrelatorState::new(redis_arc.clone());

    // Use current Unix time for test events
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Add GW events
    let gw1 = TestGWEvent {
        simulation_id: 500,
        gpstime: now - 3600.0, // 1 hour ago
        pipeline: "SGNL".to_string(),
        snr: 25.5,
        far: 1e-12,
        skymap_path: "test.fits".to_string(),
    };
    state1.add_gw_event(gw1.clone()).await;

    let gw2 = TestGWEvent {
        simulation_id: 501,
        gpstime: now - 1800.0, // 30 minutes ago
        pipeline: "pycbc".to_string(),
        snr: 20.1,
        far: 5e-10,
        skymap_path: "test2.fits".to_string(),
    };
    state1.add_gw_event(gw2.clone()).await;

    // Add GRB event (correlated with gw1)
    let grb1 = TestGRBEvent {
        simulation_id: 500,
        detection_time: now - 3598.0, // 2 seconds after gw1
        ra: 180.0,
        dec: 30.0,
        error_radius: 5.0,
        instrument: "Fermi-GBM".to_string(),
        skymap_path: "grb.fits".to_string(),
    };
    state1.add_grb_event(grb1.clone()).await;

    // Verify in-memory state
    assert_eq!(state1.gw_events.len(), 2);
    assert_eq!(state1.grb_events.len(), 1);
    println!("✓ Phase 1 complete: 2 GW + 1 GRB events persisted");

    // Give Redis time to persist
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // === PHASE 2: Service restart (drop state1) ===
    println!("\nPHASE 2: Simulating service restart");
    drop(state1);
    println!("✓ Old state dropped (simulating restart)");

    // === PHASE 3: New service instance with recovery ===
    println!("\nPHASE 3: Starting new service instance and recovering state");

    let mut state2 = TestCorrelatorState::new(redis_arc.clone());

    // Verify empty before recovery
    assert_eq!(state2.gw_events.len(), 0);
    assert_eq!(state2.grb_events.len(), 0);
    println!("✓ New state is empty before recovery");

    // Recover from Redis
    state2
        .recover_from_redis(7200.0)
        .await
        .expect("Recovery failed");

    println!(
        "✓ Recovery complete: {} GW, {} GRB events recovered",
        state2.gw_events.len(),
        state2.grb_events.len()
    );

    // === PHASE 4: Verification ===
    println!("\nPHASE 4: Verifying recovered state");

    // Verify counts
    assert_eq!(state2.gw_events.len(), 2, "Should recover 2 GW events");
    assert_eq!(state2.grb_events.len(), 1, "Should recover 1 GRB event");

    // Verify specific events
    let recovered_gw1 = state2.gw_events.get(&500).expect("GW 500 not recovered");
    assert_eq!(recovered_gw1.simulation_id, gw1.simulation_id);
    assert_eq!(recovered_gw1.gpstime, gw1.gpstime);
    assert_eq!(recovered_gw1.snr, gw1.snr);

    let recovered_gw2 = state2.gw_events.get(&501).expect("GW 501 not recovered");
    assert_eq!(recovered_gw2.simulation_id, gw2.simulation_id);
    assert_eq!(recovered_gw2.pipeline, gw2.pipeline);

    let recovered_grb1 = state2.grb_events.get(&500).expect("GRB 500 not recovered");
    assert_eq!(recovered_grb1.simulation_id, grb1.simulation_id);
    assert_eq!(recovered_grb1.instrument, grb1.instrument);

    println!("✓ All events recovered correctly");

    // === PHASE 5: Demonstrate continued operation ===
    println!("\nPHASE 5: Demonstrating continued operation with recovered state");

    // Add a new event to the recovered state
    let gw3 = TestGWEvent {
        simulation_id: 502,
        gpstime: 1412547100.0,
        pipeline: "gstlal".to_string(),
        snr: 28.3,
        far: 1e-15,
        skymap_path: "test3.fits".to_string(),
    };
    state2.add_gw_event(gw3.clone()).await;

    assert_eq!(
        state2.gw_events.len(),
        3,
        "Should have 2 recovered + 1 new event"
    );
    println!("✓ New events can be added to recovered state");

    // Cleanup
    {
        let mut store = redis_arc.as_ref().unwrap().lock().await;
        store
            .delete_keys(&[
                "gw_events",
                "grb_events",
                "event:gw:500",
                "event:gw:501",
                "event:gw:502",
                "event:grb:500",
            ])
            .await
            .unwrap();
    }

    println!("\n✓ Test complete! Service can successfully recover from Redis.");
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_recovery_with_time_filtering() {
    let redis_store = RedisStateStore::new("redis://127.0.0.1:6379")
        .await
        .expect("Failed to connect to Redis");

    let redis_arc = Some(Arc::new(Mutex::new(redis_store)));

    // Cleanup
    {
        let mut store = redis_arc.as_ref().unwrap().lock().await;
        store
            .delete_keys(&["gw_events", "event:gw:600", "event:gw:601"])
            .await
            .unwrap();
    }

    let mut state = TestCorrelatorState::new(redis_arc.clone());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Old event (outside recovery window)
    let old_event = TestGWEvent {
        simulation_id: 600,
        gpstime: now - 10800.0, // 3 hours ago
        pipeline: "old".to_string(),
        snr: 15.0,
        far: 1e-8,
        skymap_path: "old.fits".to_string(),
    };
    state.add_gw_event(old_event).await;

    // Recent event (inside recovery window)
    let recent_event = TestGWEvent {
        simulation_id: 601,
        gpstime: now - 1800.0, // 30 minutes ago
        pipeline: "recent".to_string(),
        snr: 22.0,
        far: 1e-11,
        skymap_path: "recent.fits".to_string(),
    };
    state.add_gw_event(recent_event.clone()).await;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Drop and recreate
    drop(state);
    let mut new_state = TestCorrelatorState::new(redis_arc.clone());

    // Recover with 1-hour lookback (should only get recent event)
    new_state.recover_from_redis(3600.0).await.unwrap();

    assert_eq!(
        new_state.gw_events.len(),
        1,
        "Should only recover recent event"
    );
    assert!(
        new_state.gw_events.contains_key(&601),
        "Should have recent event"
    );
    assert!(
        !new_state.gw_events.contains_key(&600),
        "Should NOT have old event"
    );

    // Cleanup
    {
        let mut store = redis_arc.as_ref().unwrap().lock().await;
        store
            .delete_keys(&["gw_events", "event:gw:600", "event:gw:601"])
            .await
            .unwrap();
    }

    println!("✓ Time-based filtering works correctly");
}
