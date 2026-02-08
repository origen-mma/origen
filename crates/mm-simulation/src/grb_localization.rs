//! GRB sky localization simulation
//!
//! Models realistic localization error regions for different GRB detection instruments.
//!
//! Different satellites have different localization capabilities:
//! - Fermi GBM: ~10° statistical + ~5° systematic (total ~15° error radius)
//! - Swift BAT: ~1-4 arcmin (0.017-0.067°)
//! - IPN triangulation: arcmin-level (requires multiple satellites)

use rand::Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// GRB localization error model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrbLocalization {
    /// True RA (degrees)
    pub true_ra: f64,

    /// True Dec (degrees)
    pub true_dec: f64,

    /// Observed RA (degrees, with error)
    pub obs_ra: f64,

    /// Observed Dec (degrees, with error)
    pub obs_dec: f64,

    /// 1-sigma error radius (degrees)
    pub error_radius: f64,

    /// Instrument that detected the GRB
    pub instrument: GrbInstrument,
}

/// GRB detection instrument
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GrbInstrument {
    /// Fermi Gamma-ray Burst Monitor (~10-15° localization)
    FermiGBM,

    /// Swift Burst Alert Telescope (~1-4 arcmin localization)
    SwiftBAT,

    /// Einstein Probe Wide-field X-ray Telescope
    EinsteinProbeWXT,

    /// IPN triangulation (requires multiple satellites)
    IPN,
}

impl GrbInstrument {
    /// Get typical localization error (1-sigma radius in degrees)
    pub fn typical_error(&self) -> f64 {
        match self {
            GrbInstrument::FermiGBM => 15.0, // ~10° statistical + ~5° systematic
            GrbInstrument::SwiftBAT => 0.05,  // ~3 arcmin typical
            GrbInstrument::EinsteinProbeWXT => 1.0, // ~1° typical
            GrbInstrument::IPN => 0.017,      // ~1 arcmin with good triangulation
        }
    }

    /// Get error range (min, max) in degrees
    pub fn error_range(&self) -> (f64, f64) {
        match self {
            GrbInstrument::FermiGBM => (10.0, 20.0),      // 10-20° depending on quality
            GrbInstrument::SwiftBAT => (0.017, 0.067),    // 1-4 arcmin
            GrbInstrument::EinsteinProbeWXT => (0.5, 2.0), // 0.5-2°
            GrbInstrument::IPN => (0.017, 0.1),           // 1-6 arcmin
        }
    }

    /// Sample a realistic error radius
    pub fn sample_error(&self, rng: &mut impl Rng) -> f64 {
        let (min_err, max_err) = self.error_range();
        min_err + rng.gen::<f64>() * (max_err - min_err)
    }
}

/// Add realistic localization error to a GRB position
///
/// # Arguments
///
/// * `true_ra` - True RA (degrees)
/// * `true_dec` - True Dec (degrees)
/// * `instrument` - Detection instrument
/// * `rng` - Random number generator
///
/// # Returns
///
/// GrbLocalization with observed position and error radius
///
/// # Algorithm
///
/// Samples error from a 2D Gaussian on the sky:
/// 1. Sample error radius from instrument-specific distribution
/// 2. Sample random direction (uniform in azimuth)
/// 3. Offset position by error in that direction
pub fn add_localization_error(
    true_ra: f64,
    true_dec: f64,
    instrument: GrbInstrument,
    rng: &mut impl Rng,
) -> GrbLocalization {
    // Sample error radius (1-sigma)
    let error_radius = instrument.sample_error(rng);

    // Sample random offset within ~1-sigma circle
    // Use Rayleigh distribution for 2D Gaussian error
    let offset_dist = Normal::new(0.0, error_radius).unwrap();
    let offset_ra = offset_dist.sample(rng);
    let offset_dec = offset_dist.sample(rng);

    // Apply offset (simple for small angles)
    // For more accurate treatment, use spherical geometry
    let obs_ra = (true_ra + offset_ra / true_dec.to_radians().cos()).rem_euclid(360.0);
    let obs_dec = (true_dec + offset_dec).clamp(-90.0, 90.0);

    GrbLocalization {
        true_ra,
        true_dec,
        obs_ra,
        obs_dec,
        error_radius,
        instrument,
    }
}

/// Generate error ellipse parameters
///
/// For visualization and skymap generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEllipse {
    /// Center RA (degrees)
    pub ra: f64,

    /// Center Dec (degrees)
    pub dec: f64,

    /// Semi-major axis (degrees)
    pub semi_major: f64,

    /// Semi-minor axis (degrees)
    pub semi_minor: f64,

    /// Position angle (degrees, East of North)
    pub position_angle: f64,

    /// Containment probability (e.g., 0.68 for 1-sigma, 0.90 for 90% credible region)
    pub containment: f64,
}

impl ErrorEllipse {
    /// Create a circular error region (typical for GBM)
    pub fn circular(ra: f64, dec: f64, radius: f64, containment: f64) -> Self {
        Self {
            ra,
            dec,
            semi_major: radius,
            semi_minor: radius,
            position_angle: 0.0,
            containment,
        }
    }

    /// Create an elliptical error region (typical for BAT)
    pub fn elliptical(
        ra: f64,
        dec: f64,
        semi_major: f64,
        semi_minor: f64,
        position_angle: f64,
        containment: f64,
    ) -> Self {
        Self {
            ra,
            dec,
            semi_major,
            semi_minor,
            position_angle,
            containment,
        }
    }

    /// Compute error region area (square degrees)
    pub fn area(&self) -> f64 {
        PI * self.semi_major * self.semi_minor
    }
}

impl GrbLocalization {
    /// Generate error ellipse for this localization
    pub fn to_error_ellipse(&self, containment: f64) -> ErrorEllipse {
        // Scale radius for desired containment
        // For 2D Gaussian: 1-sigma circle contains 39%, 2-sigma contains 86%, etc.
        let scale = match containment {
            p if p < 0.40 => 1.0,
            p if p < 0.70 => 1.5,
            p if p <= 0.90 => 2.0,
            p if p < 0.95 => 2.5,
            _ => 3.0,
        };

        let radius = self.error_radius * scale;

        ErrorEllipse::circular(self.obs_ra, self.obs_dec, radius, containment)
    }

    /// Compute angular separation from true position (degrees)
    pub fn position_error(&self) -> f64 {
        angular_separation(
            self.true_ra,
            self.true_dec,
            self.obs_ra,
            self.obs_dec,
        )
        .to_degrees()
    }
}

/// Compute angular separation between two positions (radians)
fn angular_separation(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let ra1_rad = ra1.to_radians();
    let dec1_rad = dec1.to_radians();
    let ra2_rad = ra2.to_radians();
    let dec2_rad = dec2.to_radians();

    let delta_ra = ra2_rad - ra1_rad;
    let delta_dec = dec2_rad - dec1_rad;

    let a = (delta_dec / 2.0).sin().powi(2)
        + dec1_rad.cos() * dec2_rad.cos() * (delta_ra / 2.0).sin().powi(2);

    2.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_fermi_localization() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let true_ra = 180.0;
        let true_dec = 30.0;

        let localization = add_localization_error(
            true_ra,
            true_dec,
            GrbInstrument::FermiGBM,
            &mut rng,
        );

        println!("Fermi GBM localization:");
        println!("  True: ({:.2}°, {:.2}°)", true_ra, true_dec);
        println!("  Obs:  ({:.2}°, {:.2}°)", localization.obs_ra, localization.obs_dec);
        println!("  Error radius: {:.2}°", localization.error_radius);
        println!("  Position error: {:.2}°", localization.position_error());

        // Error radius should be in expected range
        assert!(localization.error_radius >= 10.0);
        assert!(localization.error_radius <= 20.0);

        // Position error should typically be within ~1-sigma
        // (but can be larger due to random chance)
        assert!(localization.position_error() < 50.0);
    }

    #[test]
    fn test_swift_localization() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);

        let true_ra = 45.0;
        let true_dec = -10.0;

        let localization = add_localization_error(
            true_ra,
            true_dec,
            GrbInstrument::SwiftBAT,
            &mut rng,
        );

        println!("Swift BAT localization:");
        println!("  True: ({:.4}°, {:.4}°)", true_ra, true_dec);
        println!("  Obs:  ({:.4}°, {:.4}°)", localization.obs_ra, localization.obs_dec);
        println!("  Error radius: {:.4}° ({:.2} arcmin)",
                 localization.error_radius,
                 localization.error_radius * 60.0);
        println!("  Position error: {:.4}° ({:.2} arcmin)",
                 localization.position_error(),
                 localization.position_error() * 60.0);

        // Error radius should be in expected range (1-4 arcmin)
        assert!(localization.error_radius >= 0.017);
        assert!(localization.error_radius <= 0.067);

        // Position error should be small
        assert!(localization.position_error() < 1.0);
    }

    #[test]
    fn test_error_ellipse() {
        let loc = GrbLocalization {
            true_ra: 180.0,
            true_dec: 30.0,
            obs_ra: 181.0,
            obs_dec: 31.0,
            error_radius: 10.0,
            instrument: GrbInstrument::FermiGBM,
        };

        // 1-sigma circle (39% containment)
        let ellipse_1sig = loc.to_error_ellipse(0.39);
        assert!((ellipse_1sig.semi_major - 10.0).abs() < 1.0);

        // 90% credible region
        let ellipse_90 = loc.to_error_ellipse(0.90);
        assert!(ellipse_90.semi_major > ellipse_1sig.semi_major);
        assert!((ellipse_90.semi_major - 20.0).abs() < 5.0);

        // Check area calculation
        let area = ellipse_90.area();
        assert!(area > 0.0);
        println!("90% error region area: {:.1} sq deg", area);
    }

    #[test]
    fn test_localization_statistics() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(456);

        let true_ra = 100.0;
        let true_dec = 20.0;

        // Generate 100 Fermi localizations
        let n_trials = 100;
        let localizations: Vec<_> = (0..n_trials)
            .map(|_| {
                add_localization_error(
                    true_ra,
                    true_dec,
                    GrbInstrument::FermiGBM,
                    &mut rng,
                )
            })
            .collect();

        // Compute statistics
        let mean_error = localizations.iter()
            .map(|l| l.position_error())
            .sum::<f64>() / n_trials as f64;

        let mean_radius = localizations.iter()
            .map(|l| l.error_radius)
            .sum::<f64>() / n_trials as f64;

        println!("Fermi localization statistics ({} trials):", n_trials);
        println!("  Mean error radius: {:.2}°", mean_radius);
        println!("  Mean position error: {:.2}°", mean_error);

        // Mean error radius should be around 15° (midpoint of 10-20° range)
        assert!(mean_radius > 12.0);
        assert!(mean_radius < 18.0);

        // Mean position error should be similar to mean error radius
        // (for 2D Gaussian, expectation is ~1.25 * sigma)
        assert!(mean_error > 10.0);
        assert!(mean_error < 25.0);
    }
}
