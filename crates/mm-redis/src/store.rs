use redis::{aio::ConnectionManager, AsyncCommands, Client};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::schema::{RedisStoredEvent, Versionable};

/// Redis state store for multi-messenger events
pub struct RedisStateStore {
    _client: Client,
    pub(crate) conn_manager: ConnectionManager,
}

#[derive(Debug, Error)]
pub enum RedisStoreError {
    #[error("Redis connection error: {0}")]
    Connection(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Event not found: {0}")]
    NotFound(String),
}

impl RedisStateStore {
    /// Create a new Redis store connection
    pub async fn new(redis_url: &str) -> Result<Self, RedisStoreError> {
        info!("Connecting to Redis at {}", redis_url);
        let client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(client.clone()).await?;
        info!("Successfully connected to Redis");

        Ok(Self {
            _client: client,
            conn_manager,
        })
    }

    /// Store an event with TTL (Time To Live)
    ///
    /// # Arguments
    /// * `key` - Redis key (e.g., "event:gw:123")
    /// * `event` - Event to store
    /// * `ttl_seconds` - Expiration time in seconds
    pub async fn store<T>(
        &mut self,
        key: &str,
        event: T,
        ttl_seconds: u64,
    ) -> Result<(), RedisStoreError>
    where
        T: Versionable,
    {
        let versioned = event.to_versioned();
        let json = versioned.to_json()?;

        let _: () = self.conn_manager.set_ex(key, json, ttl_seconds).await?;

        debug!("Stored {} with TTL {}s", key, ttl_seconds);
        Ok(())
    }

    /// Retrieve an event by key
    pub async fn get<T>(&mut self, key: &str) -> Result<Option<T>, RedisStoreError>
    where
        T: Versionable,
    {
        let json: Option<String> = self.conn_manager.get(key).await?;

        match json {
            Some(json) => {
                match RedisStoredEvent::<serde_json::Value>::from_json(&json) {
                    Ok(stored) => match T::from_versioned(stored) {
                        Ok(event) => {
                            debug!("Retrieved {}", key);
                            Ok(Some(event))
                        }
                        Err(e) => {
                            error!("Failed to deserialize {}: {}", key, e);
                            // Delete corrupted data
                            let _: () = self.conn_manager.del(key).await.unwrap_or(());
                            Ok(None)
                        }
                    },
                    Err(e) => {
                        error!("Failed to parse stored event {}: {}", key, e);
                        let _: () = self.conn_manager.del(key).await.unwrap_or(());
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// Add event to sorted set for time-based queries
    ///
    /// # Arguments
    /// * `set_key` - Sorted set key (e.g., "gw_events")
    /// * `score` - Sort score (typically GPS time or MJD)
    /// * `member` - Set member (typically event ID)
    pub async fn zadd<M>(
        &mut self,
        set_key: &str,
        score: f64,
        member: M,
    ) -> Result<(), RedisStoreError>
    where
        M: redis::ToRedisArgs + Send + Sync,
    {
        let _: () = self.conn_manager.zadd(set_key, member, score).await?;
        Ok(())
    }

    /// Get events from sorted set by score range
    ///
    /// # Arguments
    /// * `set_key` - Sorted set key
    /// * `min_score` - Minimum score (inclusive)
    /// * `max_score` - Maximum score (inclusive)
    ///
    /// Returns list of member IDs
    pub async fn zrangebyscore(
        &mut self,
        set_key: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<String>, RedisStoreError> {
        let members = self
            .conn_manager
            .zrangebyscore(set_key, min_score, max_score)
            .await?;
        Ok(members)
    }

    /// Remove events from sorted set by score range (cleanup old events)
    pub async fn zremrangebyscore(
        &mut self,
        set_key: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<u64, RedisStoreError> {
        let count: u64 = redis::cmd("ZREMRANGEBYSCORE")
            .arg(set_key)
            .arg(min_score)
            .arg(max_score)
            .query_async(&mut self.conn_manager)
            .await?;
        Ok(count)
    }

    /// Get all events in time range with full data
    ///
    /// This is a convenience method that combines zrangebyscore + get operations
    pub async fn get_events_in_range<T>(
        &mut self,
        set_key: &str,
        key_prefix: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<T>, RedisStoreError>
    where
        T: Versionable,
    {
        // Get IDs in range
        let ids = self.zrangebyscore(set_key, min_score, max_score).await?;

        // Fetch full events
        let mut events = Vec::new();
        let mut failed = 0;

        for id in ids {
            let key = format!("{}:{}", key_prefix, id);
            match self.get::<T>(&key).await? {
                Some(event) => events.push(event),
                None => {
                    failed += 1;
                    // Remove stale reference from sorted set
                    let _: () = self.conn_manager.zrem(set_key, &id).await.unwrap_or(());
                }
            }
        }

        if failed > 0 {
            warn!(
                "Skipped {} events from {} due to deserialization errors",
                failed, set_key
            );
        }

        debug!(
            "Retrieved {} events from {} in range [{}, {}]",
            events.len(),
            set_key,
            min_score,
            max_score
        );

        Ok(events)
    }

    /// Delete an event
    pub async fn delete(&mut self, key: &str) -> Result<(), RedisStoreError> {
        let _: () = self.conn_manager.del(key).await?;
        debug!("Deleted {}", key);
        Ok(())
    }

    /// Delete multiple keys at once
    pub async fn delete_keys(&mut self, keys: &[&str]) -> Result<(), RedisStoreError> {
        if keys.is_empty() {
            return Ok(());
        }
        let _: () = redis::cmd("DEL")
            .arg(keys)
            .query_async(&mut self.conn_manager)
            .await?;
        debug!("Deleted {} keys", keys.len());
        Ok(())
    }

    /// Check if Redis connection is alive
    pub async fn ping(&mut self) -> Result<(), RedisStoreError> {
        let pong: String = redis::cmd("PING")
            .query_async(&mut self.conn_manager)
            .await?;
        if pong == "PONG" {
            Ok(())
        } else {
            Err(RedisStoreError::Connection(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Unexpected PING response",
            ))))
        }
    }

    /// Get Redis info statistics
    pub async fn get_stats(&mut self) -> Result<HashMap<String, String>, RedisStoreError> {
        let info: String = redis::cmd("INFO")
            .arg("stats")
            .query_async(&mut self.conn_manager)
            .await?;

        let stats: HashMap<String, String> = info
            .lines()
            .filter(|line| line.contains(':'))
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].trim().to_string()))
                } else {
                    None
                }
            })
            .collect();

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestEvent {
        pub id: u32,
        pub time: f64,
        #[serde(default)]
        pub name: Option<String>,
    }

    impl Versionable for TestEvent {
        fn schema_name() -> &'static str {
            "TestEvent"
        }
    }

    // Note: These tests require Redis to be running
    // Run with: docker run -d -p 6379:6379 redis:7-alpine
}
