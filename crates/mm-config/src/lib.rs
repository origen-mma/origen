use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub gcn: GcnConfig,
    pub boom: BoomConfig,
    pub correlator: CorrelatorConfig,
    pub simulation: SimulationConfig,
}

/// GCN Kafka configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcnConfig {
    /// GCN Kafka client ID
    pub client_id: String,

    /// GCN Kafka client secret
    pub client_secret: String,

    /// Topics to subscribe to
    pub topics: Vec<String>,
}

/// BOOM Kafka configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoomConfig {
    /// BOOM Kafka bootstrap servers
    pub bootstrap_servers: String,

    /// SASL username
    pub sasl_username: String,

    /// SASL password
    pub sasl_password: String,

    /// Consumer group ID
    pub group_id: String,

    /// Topics to subscribe to
    pub topics: Vec<String>,
}

/// Correlator configuration (RAVEN parameters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatorConfig {
    /// Time window before GW event (seconds, typically -1.0)
    pub time_window_before: f64,

    /// Time window after GW event (seconds, typically 86400.0 = 1 day)
    pub time_window_after: f64,

    /// Spatial matching threshold (degrees)
    pub spatial_threshold: f64,

    /// False alarm rate threshold (events per year)
    pub far_threshold: f64,

    /// Background rate (alerts per year)
    pub background_rate: f64,

    /// Trials factor (number of bands)
    pub trials_factor: f64,

    /// Maximum superevent age before cleanup (seconds)
    pub max_superevent_age: f64,
}

/// Simulation mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Enable simulation mode
    pub enabled: bool,

    /// Path to ZTF CSV directory
    pub ztf_csv_dir: String,

    /// Delay between simulated alerts (milliseconds)
    pub delay_ms: u64,

    /// Directory for storing downloaded skymaps
    pub skymap_storage_dir: String,
}

impl Config {
    /// Load configuration from TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load configuration with environment variable overrides
    pub fn from_file_with_env<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let mut config = Self::from_file(path)?;

        // Override with environment variables if present
        if let Ok(val) = std::env::var("GCN_CLIENT_ID") {
            config.gcn.client_id = val;
        }
        if let Ok(val) = std::env::var("GCN_CLIENT_SECRET") {
            config.gcn.client_secret = val;
        }
        if let Ok(val) = std::env::var("BOOM_SASL_USERNAME") {
            config.boom.sasl_username = val;
        }
        if let Ok(val) = std::env::var("BOOM_SASL_PASSWORD") {
            config.boom.sasl_password = val;
        }
        if let Ok(val) = std::env::var("ZTF_CSV_DIR") {
            config.simulation.ztf_csv_dir = val;
        }

        Ok(config)
    }

    /// Create default development configuration
    pub fn development() -> Self {
        Self {
            gcn: GcnConfig {
                client_id: "CHANGE_ME".to_string(),
                client_secret: "CHANGE_ME".to_string(),
                topics: vec![
                    "igwn.gwalert".to_string(),
                    "gcn.notices.swift.bat.guano".to_string(),
                    "gcn.notices.einstein_probe.wxt.alert".to_string(),
                    "gcn.notices.icecube.lvk_nu_track_search".to_string(),
                    "gcn.notices.icecube.gold_bronze_track_alerts".to_string(),
                    "gcn.circulars".to_string(),
                ],
            },
            boom: BoomConfig {
                bootstrap_servers: "kaboom.caltech.edu:9093".to_string(),
                sasl_username: "CHANGE_ME".to_string(),
                sasl_password: "CHANGE_ME".to_string(),
                group_id: "origin".to_string(),
                topics: vec![
                    // ZTF topics
                    "babamul.ztf.no-lsst-match.stellar".to_string(),
                    "babamul.ztf.no-lsst-match.hosted".to_string(),
                    "babamul.ztf.no-lsst-match.hostless".to_string(),
                    "babamul.ztf.ztfbh-partnership.stellar".to_string(),
                    "babamul.ztf.ztfbh-partnership.hosted".to_string(),
                    "babamul.ztf.ztfbh-partnership.hostless".to_string(),
                    "babamul.ztf.ztf-partnership.stellar".to_string(),
                    "babamul.ztf.ztf-partnership.hosted".to_string(),
                    "babamul.ztf.ztf-partnership.hostless".to_string(),
                ],
            },
            correlator: CorrelatorConfig {
                time_window_before: -1.0,
                time_window_after: 86400.0,
                spatial_threshold: 5.0,
                far_threshold: 1.0 / 30.0,
                background_rate: 1.0,
                trials_factor: 7.0,
                max_superevent_age: 7200.0,
            },
            simulation: SimulationConfig {
                enabled: true,
                ztf_csv_dir: "/Users/mcoughlin/Code/ORIGIN/lightcurves_csv".to_string(),
                delay_ms: 0,
                skymap_storage_dir: "./data/skymaps".to_string(),
            },
        }
    }

    /// Save configuration to TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let toml = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        fs::write(path, toml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::development();
        assert_eq!(config.correlator.time_window_after, 86400.0);
        assert_eq!(config.correlator.spatial_threshold, 5.0);
        assert_eq!(config.boom.group_id, "origin");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::development();
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("[gcn]"));
        assert!(toml.contains("[boom]"));
        assert!(toml.contains("[correlator]"));
    }
}
