use serde::{Deserialize, Serialize};
use tracing::warn;

/// Current schema version for all stored events
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Versioned wrapper for Redis-stored events
///
/// This wrapper ensures we can handle schema evolution gracefully:
/// - Version field tracks schema changes
/// - Schema type helps with debugging
/// - Timestamp for expiration tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisStoredEvent<T> {
    /// Schema version (increment on breaking changes)
    pub version: u32,

    /// Event type name for debugging
    pub schema: String,

    /// Unix timestamp when stored
    pub stored_at: f64,

    /// The actual event data
    pub data: T,
}

impl<T> RedisStoredEvent<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Create a new versioned event with current schema version
    pub fn new(schema: impl Into<String>, data: T) -> Self {
        Self {
            version: CURRENT_SCHEMA_VERSION,
            schema: schema.into(),
            stored_at: chrono::Utc::now().timestamp() as f64,
            data,
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string with version checking
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let stored: Self = serde_json::from_str(json)?;

        // Warn if version mismatch (but still allow it)
        if stored.version != CURRENT_SCHEMA_VERSION {
            warn!(
                "Schema version mismatch: stored={}, current={}, type={}",
                stored.version, CURRENT_SCHEMA_VERSION, stored.schema
            );
        }

        Ok(stored)
    }
}

/// Helper trait to add versioning methods to event types
pub trait Versionable: Sized + Serialize + for<'de> Deserialize<'de> {
    /// Schema type name
    fn schema_name() -> &'static str;

    /// Wrap in versioned container
    fn to_versioned(self) -> RedisStoredEvent<Self> {
        RedisStoredEvent::new(Self::schema_name(), self)
    }

    /// Extract from versioned container
    fn from_versioned(
        stored: RedisStoredEvent<serde_json::Value>,
    ) -> Result<Self, serde_json::Error> {
        if stored.version != CURRENT_SCHEMA_VERSION {
            warn!(
                "Deserializing older schema version {} (current: {})",
                stored.version, CURRENT_SCHEMA_VERSION
            );
        }
        serde_json::from_value(stored.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestEvent {
        id: u32,
        #[serde(default)] // Handle missing field gracefully
        name: Option<String>,
    }

    impl Versionable for TestEvent {
        fn schema_name() -> &'static str {
            "TestEvent"
        }
    }

    #[test]
    fn test_versioned_round_trip() {
        let event = TestEvent {
            id: 42,
            name: Some("test".to_string()),
        };

        let versioned = event.clone().to_versioned();
        assert_eq!(versioned.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(versioned.schema, "TestEvent");

        let json = versioned.to_json().unwrap();
        let restored = RedisStoredEvent::<TestEvent>::from_json(&json).unwrap();

        assert_eq!(restored.data.id, event.id);
        assert_eq!(restored.data.name, event.name);
    }

    #[test]
    fn test_missing_field_with_default() {
        // Simulate old schema without 'name' field
        let json = r#"{"version":1,"schema":"TestEvent","stored_at":1234.0,"data":{"id":42}}"#;

        let restored = RedisStoredEvent::<TestEvent>::from_json(json).unwrap();
        assert_eq!(restored.data.id, 42);
        assert_eq!(restored.data.name, None); // Uses serde default
    }
}
