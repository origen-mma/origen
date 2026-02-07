use crate::SkyPosition;
use std::f64::consts::PI;

/// Mock GW skymap for simulation
///
/// Generates realistic spatial distributions for testing.
/// Real GW skymaps would be HEALPix probability maps.
pub struct MockSkymap {
    /// Center of the skymap (RA, Dec)
    pub center_ra: f64,
    pub center_dec: f64,

    /// 50% credible region radius (degrees)
    pub radius_50: f64,

    /// 90% credible region radius (degrees)
    pub radius_90: f64,
}

impl MockSkymap {
    /// Create a new mock skymap with given center and credible regions
    pub fn new(center_ra: f64, center_dec: f64, radius_50: f64, radius_90: f64) -> Self {
        Self {
            center_ra,
            center_dec,
            radius_50,
            radius_90,
        }
    }

    /// Create a mock skymap with typical neutron star merger parameters
    /// Based on GW170817: ~30 deg² 90% credible region
    pub fn typical_ns_merger(center_ra: f64, center_dec: f64) -> Self {
        // 30 deg² → radius ≈ 3 degrees
        Self::new(center_ra, center_dec, 1.5, 3.0)
    }

    /// Create a mock skymap with large localization (poor localization)
    /// Typical for single-detector or low-SNR events
    pub fn poor_localization(center_ra: f64, center_dec: f64) -> Self {
        // ~1000 deg² → radius ≈ 18 degrees
        Self::new(center_ra, center_dec, 9.0, 18.0)
    }

    /// Sample a position from this skymap
    ///
    /// Returns a position distributed such that:
    /// - 50% of samples fall within radius_50
    /// - 90% of samples fall within radius_90
    pub fn sample_position(&self, rng: &mut impl rand::Rng) -> SkyPosition {
        // Sample from Rayleigh distribution to get realistic 2D offset
        // Use inverse CDF sampling
        let u = rng.gen::<f64>();

        // For 50% probability, use radius_50
        // For 90% probability, use radius_90
        // Interpolate between them using a Rayleigh-like distribution

        let radius_deg = if u < 0.5 {
            // Sample within 50% CR
            let u_scaled = u / 0.5; // Scale to [0, 1]
            self.radius_50 * (-2.0 * (1.0 - u_scaled).ln()).sqrt()
        } else if u < 0.9 {
            // Sample between 50% and 90% CR
            let u_scaled = (u - 0.5) / 0.4; // Scale to [0, 1]
            let r_min = self.radius_50;
            let r_max = self.radius_90;
            r_min + (r_max - r_min) * (-2.0 * (1.0 - u_scaled).ln()).sqrt()
        } else {
            // Sample outside 90% CR (10% of the time)
            let u_scaled = (u - 0.9) / 0.1;
            self.radius_90 * (1.0 + u_scaled * 2.0) // Extend beyond 90%
        };

        // Sample random angle
        let angle = rng.gen::<f64>() * 2.0 * PI;

        // Convert offset to RA/Dec
        // For small angles, this is approximately:
        // RA offset ≈ radius * cos(angle) / cos(dec)
        // Dec offset ≈ radius * sin(angle)

        let dec_offset = radius_deg * angle.sin();
        let ra_offset = radius_deg * angle.cos() / self.center_dec.to_radians().cos();

        let ra = (self.center_ra + ra_offset).rem_euclid(360.0);
        let dec = (self.center_dec + dec_offset).clamp(-90.0, 90.0);

        SkyPosition::new(ra, dec, 2.0) // ZTF position uncertainty
    }

    /// Get the center position
    pub fn center(&self) -> SkyPosition {
        SkyPosition::new(self.center_ra, self.center_dec, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_mock_skymap_sampling() {
        let skymap = MockSkymap::typical_ns_merger(180.0, 45.0);
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let mut inside_50 = 0;
        let mut inside_90 = 0;
        let n_samples = 10000;

        for _ in 0..n_samples {
            let pos = skymap.sample_position(&mut rng);
            let sep = skymap.center().angular_separation(&pos);

            if sep <= skymap.radius_50 {
                inside_50 += 1;
            }
            if sep <= skymap.radius_90 {
                inside_90 += 1;
            }
        }

        let frac_50 = inside_50 as f64 / n_samples as f64;
        let frac_90 = inside_90 as f64 / n_samples as f64;

        // Allow some tolerance
        assert!(
            (frac_50 - 0.5).abs() < 0.1,
            "Expected ~50% inside 50% CR, got {:.1}%",
            frac_50 * 100.0
        );
        assert!(
            (frac_90 - 0.9).abs() < 0.1,
            "Expected ~90% inside 90% CR, got {:.1}%",
            frac_90 * 100.0
        );
    }
}
