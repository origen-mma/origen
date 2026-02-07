use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// GPS time representation
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GpsTime {
    /// GPS seconds since GPS epoch (Jan 6, 1980)
    pub seconds: f64,
}

impl GpsTime {
    /// Create from GPS seconds
    pub fn from_seconds(seconds: f64) -> Self {
        Self { seconds }
    }

    /// Convert to Unix timestamp
    pub fn to_unix_timestamp(&self) -> f64 {
        // GPS epoch is 315964800 seconds after Unix epoch
        // Accounting for leap seconds (18 as of 2024)
        self.seconds + 315964800.0 - 18.0
    }

    /// Create from Unix timestamp
    pub fn from_unix_timestamp(unix_ts: f64) -> Self {
        Self {
            seconds: unix_ts - 315964800.0 + 18.0,
        }
    }

    /// Convert to DateTime
    pub fn to_datetime(&self) -> DateTime<Utc> {
        let unix_ts = self.to_unix_timestamp();
        let secs = unix_ts.floor() as i64;
        let nsecs = ((unix_ts - secs as f64) * 1e9) as u32;
        DateTime::from_timestamp(secs, nsecs).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gps_time_conversion() {
        let gps = GpsTime::from_seconds(1412546713.52);
        let unix_ts = gps.to_unix_timestamp();
        let gps2 = GpsTime::from_unix_timestamp(unix_ts);

        assert!((gps.seconds - gps2.seconds).abs() < 1e-6);
    }
}
