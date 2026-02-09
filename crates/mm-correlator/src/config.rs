use mm_core::LightCurveFilterConfig;

/// Configuration for superevent correlation
/// Based on RAVEN's proven parameters
#[derive(Debug, Clone)]
pub struct CorrelatorConfig {
    /// Time window before GW trigger (seconds)
    /// Default: -1.0 (look back 1 second)
    pub time_window_before: f64,

    /// Time window after GW trigger (seconds)
    /// Default: 86400.0 (look forward 1 day)
    pub time_window_after: f64,

    /// Spatial matching threshold (degrees)
    /// Default: 5.0 degrees
    pub spatial_threshold: f64,

    /// Background rate for optical alerts (per year)
    /// Default: 1.0 (1 alert per year)
    pub background_rate: f64,

    /// False Alarm Rate threshold for significance
    /// Default: 1.0 / 30.0 (1 per month)
    pub far_threshold: f64,

    /// Trials factor (number of filter bands tested)
    /// Default: 7.0 (for ugrizy + reference)
    pub trials_factor: f64,

    /// Maximum age before cleanup (seconds)
    /// Default: 604800.0 (1 week)
    pub max_superevent_age: f64,

    /// Light curve feature-based filtering configuration
    pub lc_filter: LightCurveFilterConfig,
}

impl Default for CorrelatorConfig {
    fn default() -> Self {
        Self {
            time_window_before: -1.0,     // -1 second
            time_window_after: 86400.0,   // +1 day
            spatial_threshold: 5.0,       // 5 degrees
            background_rate: 1.0,         // 1/year
            far_threshold: 1.0 / 30.0,    // 1/month
            trials_factor: 7.0,           // 7 bands
            max_superevent_age: 604800.0, // 1 week
            lc_filter: LightCurveFilterConfig::default(),
        }
    }
}

impl CorrelatorConfig {
    /// Create RAVEN-compatible configuration
    pub fn raven() -> Self {
        Self::default()
    }

    /// Create test configuration with shorter windows
    pub fn test() -> Self {
        Self {
            time_window_before: -1.0,
            time_window_after: 3600.0,  // 1 hour instead of 1 day
            spatial_threshold: 10.0,    // More permissive for testing
            max_superevent_age: 3600.0, // 1 hour
            ..Self::default()
        }
    }

    /// Create configuration with light curve filtering disabled
    pub fn without_lc_filter() -> Self {
        let mut config = Self::default();
        config.lc_filter.enable = false;
        config
    }
}
