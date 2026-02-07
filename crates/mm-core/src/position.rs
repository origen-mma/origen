use serde::{Deserialize, Serialize};

/// Sky position with uncertainty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkyPosition {
    /// Right ascension (degrees)
    pub ra: f64,
    /// Declination (degrees)
    pub dec: f64,
    /// Position uncertainty (arcseconds)
    pub uncertainty: f64,
}

impl SkyPosition {
    /// Create a new sky position
    pub fn new(ra: f64, dec: f64, uncertainty: f64) -> Self {
        Self {
            ra,
            dec,
            uncertainty,
        }
    }

    /// Calculate angular separation to another position (degrees)
    pub fn angular_separation(&self, other: &SkyPosition) -> f64 {
        use std::f64::consts::PI;

        let ra1 = self.ra * PI / 180.0;
        let dec1 = self.dec * PI / 180.0;
        let ra2 = other.ra * PI / 180.0;
        let dec2 = other.dec * PI / 180.0;

        let cos_sep = dec1.sin() * dec2.sin() + dec1.cos() * dec2.cos() * (ra1 - ra2).cos();

        cos_sep.acos() * 180.0 / PI
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angular_separation() {
        let pos1 = SkyPosition::new(0.0, 0.0, 1.0);
        let pos2 = SkyPosition::new(1.0, 0.0, 1.0);

        let sep = pos1.angular_separation(&pos2);
        assert!((sep - 1.0).abs() < 1e-10);
    }
}
