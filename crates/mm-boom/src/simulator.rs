use mm_core::{
    estimate_explosion_time, io::load_lightcurves_dir, LightCurve, MockSkymap, SkyPosition,
};
use rand::SeedableRng;
use std::error::Error;
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

/// Simulate BOOM alerts by reading ZTF CSV files
pub struct BoomSimulator {
    lightcurves: Vec<LightCurve>,
    current_index: usize,
    delay_ms: u64,
    skymap: Option<MockSkymap>,
    rng: rand::rngs::StdRng,
}

impl BoomSimulator {
    /// Load ZTF light curves from directory
    pub fn from_directory<P: AsRef<Path>>(dir: P, delay_ms: u64) -> Result<Self, Box<dyn Error>> {
        Self::from_directory_with_skymap(dir, delay_ms, None)
    }

    /// Load ZTF light curves with optional GW skymap for realistic spatial distribution
    pub fn from_directory_with_skymap<P: AsRef<Path>>(
        dir: P,
        delay_ms: u64,
        skymap: Option<MockSkymap>,
    ) -> Result<Self, Box<dyn Error>> {
        info!("Loading ZTF light curves from: {}", dir.as_ref().display());

        let lightcurves = load_lightcurves_dir(dir)?;

        info!("Loaded {} light curves for simulation", lightcurves.len());

        if let Some(ref skymap) = skymap {
            info!(
                "Using GW skymap centered at RA={:.2}, Dec={:.2}, 90% radius={:.2} deg",
                skymap.center_ra, skymap.center_dec, skymap.radius_90
            );
        }

        Ok(Self {
            lightcurves,
            current_index: 0,
            delay_ms,
            skymap,
            rng: rand::rngs::StdRng::seed_from_u64(42), // Fixed seed for reproducibility
        })
    }

    /// Get next light curve
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&LightCurve> {
        if self.current_index < self.lightcurves.len() {
            let lc = &self.lightcurves[self.current_index];
            self.current_index += 1;
            Some(lc)
        } else {
            None
        }
    }

    /// Stream light curves with delay
    pub async fn stream<F>(&mut self, mut handler: F) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(&LightCurve, &SkyPosition, f64) -> Result<(), Box<dyn Error>>,
    {
        info!(
            "Starting simulation with {} light curves",
            self.lightcurves.len()
        );

        let mut processed = 0;

        // Reset index
        self.current_index = 0;

        let n_lcs = self.lightcurves.len();
        for idx in 0..n_lcs {
            let lc = &self.lightcurves[idx];

            // Generate position from skymap if available, otherwise use placeholder
            let position = if let Some(ref skymap) = self.skymap {
                skymap.sample_position(&mut self.rng)
            } else {
                extract_position_from_lightcurve(lc)
            };

            // Estimate explosion time from light curve
            let explosion_time_gps = estimate_explosion_time(lc).unwrap_or_else(|| {
                // Fallback: use first detection time
                if !lc.measurements.is_empty() {
                    lc.measurements[0].to_gps_time()
                } else {
                    0.0
                }
            });

            match handler(lc, &position, explosion_time_gps) {
                Ok(_) => {
                    processed += 1;
                    if processed % 100 == 0 {
                        info!("Simulated {} alerts", processed);
                    }
                }
                Err(e) => {
                    warn!("Handler error for {}: {}", lc.object_id, e);
                }
            }

            if self.delay_ms > 0 {
                sleep(Duration::from_millis(self.delay_ms)).await;
            }
        }

        info!("Simulation complete: {} alerts processed", processed);

        Ok(())
    }

    /// Get total number of light curves
    pub fn len(&self) -> usize {
        self.lightcurves.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.lightcurves.is_empty()
    }
}

/// Extract sky position from light curve
/// This is a placeholder - in reality, positions would come from alert metadata
fn extract_position_from_lightcurve(lc: &LightCurve) -> SkyPosition {
    // For now, use a hash of the object ID to generate deterministic RA/Dec
    // This ensures the same object always has the same position
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    lc.object_id.hash(&mut hasher);
    let hash = hasher.finish();

    // Map hash to RA [0, 360] and Dec [-90, 90]
    let ra = (hash % 36000) as f64 / 100.0;
    let dec = ((hash / 36000) % 18000) as f64 / 100.0 - 90.0;

    SkyPosition::new(ra, dec, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_extraction() {
        let lc = LightCurve::new("ZTF24aaabcd".to_string());
        let pos1 = extract_position_from_lightcurve(&lc);
        let pos2 = extract_position_from_lightcurve(&lc);

        // Same object should always get same position
        assert_eq!(pos1.ra, pos2.ra);
        assert_eq!(pos1.dec, pos2.dec);

        // Position should be within valid ranges
        assert!(pos1.ra >= 0.0 && pos1.ra <= 360.0);
        assert!(pos1.dec >= -90.0 && pos1.dec <= 90.0);
    }
}
