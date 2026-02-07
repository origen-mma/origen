# mm-redis: Redis State Persistence for Multi-Messenger Events

This crate provides schema-versioned Redis storage for multi-messenger astronomy events, enabling the correlator service to survive restarts and handle schema evolution gracefully.

## Features

- **Schema Versioning**: All events are wrapped with version metadata for safe schema evolution
- **Automatic TTL**: Events expire automatically (2 hours for GW/GRB, 1 day for optical)
- **Time-Range Queries**: Efficient sorted set queries for temporal correlation
- **Graceful Degradation**: Corrupted data is logged and deleted, not surfaced as errors
- **Non-Blocking**: All persistence happens asynchronously via `tokio::spawn`

## Architecture

```
┌─────────────────────────────────────────────────┐
│          RedisStoredEvent<T>                    │
│  {                                              │
│    version: 1,                                  │
│    schema: "GWEvent",                           │
│    stored_at: 1707274123.45,                    │
│    data: { /* actual event */ }                 │
│  }                                              │
└─────────────────────────────────────────────────┘
                    │
                    ▼
        ┌───────────────────────┐
        │   Redis Key-Value     │
        │  event:gw:12345       │
        │  TTL: 7200s          │
        └───────────────────────┘
                    │
                    ▼
        ┌───────────────────────┐
        │   Redis Sorted Set    │
        │   gw_events           │
        │   {                   │
        │     1412546713.52: 12345  │
        │     1412553891.01: 12346  │
        │   }                   │
        └───────────────────────┘
```

## Usage

### 1. Implement `Versionable` for your event type

```rust
use mm_redis::Versionable;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GWEvent {
    simulation_id: u32,
    gpstime: f64,
    snr: f32,
}

impl Versionable for GWEvent {
    fn schema_name() -> &'static str {
        "GWEvent"
    }
}
```

### 2. Create RedisStateStore

```rust
use mm_redis::RedisStateStore;

let mut store = RedisStateStore::new("redis://127.0.0.1:6379").await?;
```

### 3. Store events

```rust
let event = GWEvent {
    simulation_id: 12345,
    gpstime: 1412546713.52,
    snr: 24.5,
};

// Store with 2-hour TTL
store.store("event:gw:12345", event, 7200).await?;

// Add to sorted set for time-range queries
store.zadd("gw_events", 1412546713.52, "12345".to_string()).await?;
```

### 4. Retrieve events

```rust
// Get single event
let event: Option<GWEvent> = store.get("event:gw:12345").await?;

// Get all events in time range
let events: Vec<GWEvent> = store.get_events_in_range(
    "gw_events",           // sorted set key
    "event:gw",            // key prefix
    1412546700.0,          // min GPS time
    1412546800.0,          // max GPS time
).await?;
```

## Integration with mm-correlator-service

The correlator service uses `Arc<Mutex<RedisStateStore>>` to share the Redis connection across async tasks:

```rust
// Initialize Redis (optional - gracefully degrades if unavailable)
let redis_store = match RedisStateStore::new("redis://127.0.0.1:6379").await {
    Ok(store) => {
        info!("✅ Connected to Redis for state persistence");
        Some(Arc::new(Mutex::new(store)))
    }
    Err(e) => {
        warn!("⚠️  Redis unavailable ({}), running without persistence", e);
        None
    }
};

// Pass to correlator state
let mut state = CorrelatorState::new(
    time_window_grb,
    time_window_optical,
    redis_store
);
```

Events are persisted asynchronously without blocking the Kafka consumer:

```rust
if let Some(redis) = self.redis_store.clone() {
    let event_clone = event.clone();
    tokio::spawn(async move {
        let key = format!("event:gw:{}", sim_id);
        let ttl = 7200; // 2 hours
        let mut store = redis.lock().await;
        if let Err(e) = store.store(&key, event_clone, ttl).await {
            error!("Failed to persist GW event {} to Redis: {}", sim_id, e);
        } else {
            let _ = store.zadd("gw_events", gps_time, sim_id.to_string()).await;
        }
    });
}
```

## Orphan Rule Workaround for External Types

To persist types from other crates (like `OpticalAlert` from `mm-core`), use a newtype wrapper:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpticalAlertWrapper(OpticalAlert);

impl Versionable for OpticalAlertWrapper {
    fn schema_name() -> &'static str {
        "OpticalAlert"
    }
}

// Usage
let wrapped = OpticalAlertWrapper(alert.clone());
store.store("event:optical:ZTF24abc", wrapped, 86400).await?;
```

## Schema Evolution

When event schemas change:

1. **Additive changes** (new optional fields): Use `#[serde(default)]`
   ```rust
   #[derive(Serialize, Deserialize)]
   struct GWEvent {
       pub gpstime: f64,
       #[serde(default)]  // Backward compatible
       pub distance: Option<f64>,
   }
   ```

2. **Breaking changes**: Increment `CURRENT_SCHEMA_VERSION` and add migration logic
   ```rust
   pub const CURRENT_SCHEMA_VERSION: u32 = 2;

   impl GWEvent {
       pub fn migrate_from_v1(old: GWEventV1) -> Self {
           Self {
               gpstime: old.gpstime,
               distance: None,  // Provide default
           }
       }
   }
   ```

3. **Graceful degradation**: The store automatically deletes corrupted data
   ```rust
   match self.get::<GWEvent>(&key).await {
       Ok(Some(event)) => events.push(event),
       Ok(None) => warn!("Key not found"),
       Err(e) => {
           error!("Failed to retrieve event {}: {}", key, e);
           let _ = self.delete(&key).await; // Clean up corruption
       }
   }
   ```

## Redis Configuration

For production, use Redis with AOF (Append-Only File) persistence:

```yaml
# docker-compose.yml
services:
  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes --appendfsync everysec
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data

volumes:
  redis-data:
```

## Testing

Integration tests require a running Redis instance:

```bash
# Start Redis
docker run -d -p 6379:6379 redis:7-alpine

# Run tests
cargo test -p mm-redis -- --ignored
```

## Performance Characteristics

- **Write latency**: ~1-2ms per event (async, non-blocking)
- **Read latency**: ~0.5-1ms per event
- **Time-range query**: O(log N + M) where M is number of matches
- **Memory overhead**: ~500 bytes per event (JSON + metadata)

## Error Handling

All Redis errors are wrapped in `RedisStoreError`:

```rust
pub enum RedisStoreError {
    Connection(RedisError),
    Serialization(String),
    Deserialization(String),
    SchemaMismatch { expected: String, actual: String },
    UnsupportedVersion(u32),
}
```

The correlator service logs errors but continues processing:

```rust
if let Err(e) = store.store(&key, event, ttl).await {
    error!("Failed to persist event: {}", e);
    // Event is still processed in memory
}
```

## License

MIT
