use serde::{Deserialize, Serialize};

/// Optical transient alert from ZTF/LSST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalAlert {
    /// Unique object identifier (e.g., "ZTF25aaaalin")
    pub object_id: String,

    /// Detection time (Modified Julian Date)
    pub mjd: f64,

    /// Right ascension (degrees)
    pub ra: f64,

    /// Declination (degrees)
    pub dec: f64,

    /// Survey source
    pub survey: Survey,

    /// Latest magnitude (if available)
    pub magnitude: Option<f32>,

    /// Magnitude error
    pub mag_err: Option<f32>,

    /// Filter band
    pub filter: String,

    /// Light curve data
    pub light_curve: Vec<PhotometryPoint>,

    /// BOOM filter results (if available)
    pub filters_passed: Vec<String>,

    /// Classification scores (if available)
    pub classifications: Vec<Classification>,
}

impl OpticalAlert {
    /// Get GPS time from MJD
    pub fn gps_time(&self) -> f64 {
        // Convert MJD to GPS time
        // GPS epoch: January 6, 1980 00:00:00 UTC = MJD 44244.0
        // Unix epoch: January 1, 1970 00:00:00 UTC = MJD 40587.0
        // GPS epoch in Unix time: 315964800 seconds

        let mjd_gps_epoch = 44244.0;
        let days_since_gps_epoch = self.mjd - mjd_gps_epoch;

        days_since_gps_epoch * 86400.0
    }

    /// Get latest detection time
    pub fn latest_detection_mjd(&self) -> f64 {
        self.light_curve
            .iter()
            .map(|p| p.mjd)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(self.mjd)
    }

    /// Get peak magnitude
    pub fn peak_magnitude(&self) -> Option<f32> {
        // Convert flux to magnitude
        self.light_curve
            .iter()
            .filter_map(|p| flux_to_magnitude(p.flux))
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Check if transient is rising
    pub fn is_rising(&self) -> bool {
        if self.light_curve.len() < 2 {
            return false;
        }

        // Compare first and last detections
        let first_flux = self.light_curve.first().unwrap().flux;
        let last_flux = self.light_curve.last().unwrap().flux;

        last_flux > first_flux * 1.5 // Rising if flux increased by 50%
    }
}

/// Convert flux (nJy or similar) to AB magnitude
pub fn flux_to_magnitude(flux: f64) -> Option<f32> {
    if flux <= 0.0 {
        return None;
    }

    // AB magnitude: m = -2.5 * log10(flux) + zeropoint
    // For flux in nJy (nanoJansky), zeropoint = 31.4
    let mag = -2.5 * flux.log10() + 31.4;
    Some(mag as f32)
}

/// Photometry point in light curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotometryPoint {
    pub mjd: f64,
    pub flux: f64,
    pub flux_err: f64,
    pub filter: String,
}

/// Survey source
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Survey {
    ZTF,
    LSST,
    DECam,
    ATLAS,
}

impl Survey {
    /// Get typical position uncertainty in arcseconds
    pub fn position_uncertainty(&self) -> f64 {
        match self {
            Survey::ZTF => 0.5,   // ~0.5 arcsec
            Survey::LSST => 0.1,  // ~0.1 arcsec
            Survey::DECam => 0.3, // ~0.3 arcsec
            Survey::ATLAS => 1.0, // ~1.0 arcsec
        }
    }
}

/// Classification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub classifier: String, // "acai", "btsbot", etc.
    pub class_name: String, // "SN Ia", "Kilonova", etc.
    pub score: f64,         // 0-1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mjd_to_gps() {
        let alert = OpticalAlert {
            object_id: "ZTF25test".to_string(),
            mjd: 60675.0, // ~2025
            ra: 123.45,
            dec: 67.89,
            survey: Survey::ZTF,
            magnitude: Some(18.5),
            mag_err: Some(0.1),
            filter: "g".to_string(),
            light_curve: vec![],
            filters_passed: vec![],
            classifications: vec![],
        };

        let gps_time = alert.gps_time();

        // GPS time should be positive and large (> 1 billion seconds)
        assert!(gps_time > 1.0e9);
    }

    #[test]
    fn test_flux_to_magnitude() {
        let flux = 100.0; // nJy
        let mag = flux_to_magnitude(flux).unwrap();

        // AB mag = -2.5*log10(100) + 31.4 = -2.5*2 + 31.4 = 26.4
        assert!((mag - 26.4).abs() < 0.1);
    }

    #[test]
    fn test_is_rising() {
        let mut alert = OpticalAlert {
            object_id: "ZTF25test".to_string(),
            mjd: 60675.0,
            ra: 123.45,
            dec: 67.89,
            survey: Survey::ZTF,
            magnitude: Some(18.5),
            mag_err: Some(0.1),
            filter: "g".to_string(),
            light_curve: vec![
                PhotometryPoint {
                    mjd: 60675.0,
                    flux: 10.0,
                    flux_err: 1.0,
                    filter: "g".to_string(),
                },
                PhotometryPoint {
                    mjd: 60676.0,
                    flux: 20.0, // Doubled
                    flux_err: 2.0,
                    filter: "g".to_string(),
                },
            ],
            filters_passed: vec![],
            classifications: vec![],
        };

        assert!(alert.is_rising());

        // Fading transient
        alert.light_curve[1].flux = 5.0;
        assert!(!alert.is_rising());
    }
}
