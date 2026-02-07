/// Redis state persistence for multi-messenger events
///
/// This module provides schema-versioned storage with graceful degradation
/// for handling schema evolution over time.

mod schema;
mod store;

pub use schema::{RedisStoredEvent, Versionable, CURRENT_SCHEMA_VERSION};
pub use store::{RedisStateStore, RedisStoreError};

#[cfg(test)]
mod tests;
