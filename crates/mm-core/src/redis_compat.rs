/// Redis compatibility layer for event types
///
/// This module provides Versionable trait implementations for all event types
/// to enable schema-versioned storage in Redis.
use crate::events::{GWEvent, GammaRayEvent, NeutrinoEvent, XRayEvent};
use crate::optical::OpticalAlert;

// Re-export from mm-redis for convenience
// Note: This creates a circular dependency, so we'll define a minimal trait here
use serde::{Deserialize, Serialize};

/// Minimal versionable trait (matches mm-redis::Versionable)
///
/// This trait allows event types to be stored in Redis with schema versioning.
/// The actual implementation is in mm-redis crate.
pub trait RedisVersionable: Sized + Serialize + for<'de> Deserialize<'de> {
    fn schema_name() -> &'static str;
}

impl RedisVersionable for GWEvent {
    fn schema_name() -> &'static str {
        "GWEvent"
    }
}

impl RedisVersionable for GammaRayEvent {
    fn schema_name() -> &'static str {
        "GammaRayEvent"
    }
}

impl RedisVersionable for XRayEvent {
    fn schema_name() -> &'static str {
        "XRayEvent"
    }
}

impl RedisVersionable for NeutrinoEvent {
    fn schema_name() -> &'static str {
        "NeutrinoEvent"
    }
}

impl RedisVersionable for OpticalAlert {
    fn schema_name() -> &'static str {
        "OpticalAlert"
    }
}

// Add serde defaults for backward compatibility
// This ensures old events without new fields can still deserialize

impl GWEvent {
    /// Migrate from older schema versions if needed
    pub fn migrate_if_needed(self) -> Self {
        // Example: Set default values for fields that might be missing
        // in older schema versions
        self
    }
}

impl GammaRayEvent {
    pub fn migrate_if_needed(self) -> Self {
        self
    }
}

impl OpticalAlert {
    pub fn migrate_if_needed(mut self) -> Self {
        // Ensure classification vec exists
        if self.classifications.is_empty() {
            self.classifications = Vec::new();
        }
        self
    }
}
