use serde::{Deserialize, Serialize};

/// Photometric measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photometry {
    /// Modified Julian Date (MJD)
    pub mjd: f64,

    /// Flux (microJansky)
    pub flux: f64,

    /// Flux error (microJansky)
    pub flux_err: f64,

    /// Filter/band (e.g., "g", "r", "i" for ZTF)
    pub filter: String,
}

impl Photometry {
    /// Create a new photometry point
    pub fn new(mjd: f64, flux: f64, flux_err: f64, filter: String) -> Self {
        Self {
            mjd,
            flux,
            flux_err,
            filter,
        }
    }

    /// Convert MJD to GPS time
    /// MJD 0 = Nov 17, 1858 00:00 UTC
    /// GPS epoch = Jan 6, 1980 00:00 UTC
    /// Difference = 44244 days = 3820713600 seconds (accounting for leap seconds)
    pub fn to_gps_time(&self) -> f64 {
        // MJD to Unix: MJD 40587 = Jan 1, 1970 00:00 UTC
        let unix_epoch_mjd = 40587.0;
        let unix_seconds = (self.mjd - unix_epoch_mjd) * 86400.0;

        // Unix to GPS: GPS epoch is 315964800 seconds after Unix epoch
        // Subtract 18 leap seconds (as of 2024)
        unix_seconds - 315964800.0 + 18.0
    }

    /// Calculate magnitude from flux
    /// mag = -2.5 * log10(flux / flux_0)
    /// Using ZTF zero point: flux_0 = 10^(23.9/2.5) microJy
    pub fn magnitude(&self) -> Option<f64> {
        if self.flux > 0.0 {
            let zp = 23.9; // ZTF zero point (AB magnitudes)
            Some(zp - 2.5 * (self.flux / 1e6).log10())
        } else {
            None
        }
    }

    /// Signal-to-noise ratio
    pub fn snr(&self) -> f64 {
        if self.flux_err > 0.0 {
            self.flux / self.flux_err
        } else {
            0.0
        }
    }
}

/// Light curve (time series of photometry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightCurve {
    /// Object identifier (e.g., "ZTF25aaaalin")
    pub object_id: String,

    /// Photometric measurements
    pub measurements: Vec<Photometry>,
}

impl LightCurve {
    /// Create a new light curve
    pub fn new(object_id: String) -> Self {
        Self {
            object_id,
            measurements: Vec::new(),
        }
    }

    /// Add a measurement
    pub fn add_measurement(&mut self, phot: Photometry) {
        self.measurements.push(phot);
    }

    /// Sort measurements by time (MJD)
    pub fn sort_by_time(&mut self) {
        self.measurements.sort_by(|a, b| {
            a.mjd.partial_cmp(&b.mjd).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Get measurements in a specific filter
    pub fn filter_band(&self, filter: &str) -> Vec<&Photometry> {
        self.measurements
            .iter()
            .filter(|p| p.filter == filter)
            .collect()
    }

    /// Get time range (MJD)
    pub fn time_range(&self) -> Option<(f64, f64)> {
        if self.measurements.is_empty() {
            return None;
        }

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;

        for m in &self.measurements {
            min = min.min(m.mjd);
            max = max.max(m.mjd);
        }

        Some((min, max))
    }

    /// Get peak flux
    pub fn peak_flux(&self) -> Option<(f64, &Photometry)> {
        self.measurements
            .iter()
            .max_by(|a, b| a.flux.partial_cmp(&b.flux).unwrap_or(std::cmp::Ordering::Equal))
            .map(|p| (p.flux, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_photometry_snr() {
        let phot = Photometry::new(60675.1, 100.0, 5.0, "g".to_string());
        assert_eq!(phot.snr(), 20.0);
    }

    #[test]
    fn test_lightcurve_filter() {
        let mut lc = LightCurve::new("ZTF25test".to_string());
        lc.add_measurement(Photometry::new(60675.1, 100.0, 5.0, "g".to_string()));
        lc.add_measurement(Photometry::new(60675.2, 150.0, 10.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60675.3, 120.0, 8.0, "g".to_string()));

        let g_band = lc.filter_band("g");
        assert_eq!(g_band.len(), 2);
    }

    #[test]
    fn test_mjd_to_gps() {
        // Known test case: MJD 58484 = GPS time ~1230336018
        let phot = Photometry::new(58484.0, 100.0, 5.0, "g".to_string());
        let gps = phot.to_gps_time();

        // Check it's in reasonable range (within a few seconds)
        assert!((gps - 1230336018.0).abs() < 10.0);
    }
}
