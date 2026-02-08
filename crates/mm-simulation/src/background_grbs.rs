//! Background GRB simulation for false association rate estimation
//!
//! This module generates unassociated (background) GRBs based on observed rates
//! to characterize the false association background for multi-messenger searches.
//!
//! ## Physical Model
//!
//! - **Rate**: ~300 short GRBs per year all-sky (Swift + Fermi)
//! - **Spatial**: Uniform distribution on the sky
//! - **Temporal**: Poisson process (random arrival times)
//! - **Fluence**: Log-normal distribution based on Swift BAT threshold
//! - **Localization**: Instrument-dependent error circles
//!
//! ## Satellite Considerations
//!
//! - **Swift BAT**: ~1/6 sky FOV, arcmin localization
//! - **Fermi GBM**: ~2/3 sky FOV, few degree localization
//!
//! ## References
//!
//! - Lien et al. 2016: "The Third Swift Burst Alert Telescope Gamma-Ray Burst Catalog"
//! - von Kienlin et al. 2020: "The Fourth Fermi-GBM Gamma-Ray Burst Catalog"

use rand::Rng;
use rand_distr::{Distribution, Exp, Normal, Uniform};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Configuration for background GRB simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundGrbConfig {
    /// Background GRB rate (per year, all sky)
    /// Default: 300 SGRBs/year (Swift BAT + Fermi GBM combined)
    pub rate_per_year: f64,

    /// Minimum fluence threshold (erg/cm²)
    /// Swift BAT: ~1e-8 to 1e-7 erg/cm²
    /// Fermi GBM: ~1e-7 to 1e-6 erg/cm²
    pub fluence_threshold: f64,

    /// Satellite field of view (fraction of sky)
    /// Swift BAT: ~0.17 (1/6 sky)
    /// Fermi GBM: ~0.67 (2/3 sky)
    pub fov_fraction: f64,

    /// Satellite name for localization properties
    pub satellite: GrbSatellite,

    /// Mean of log10(fluence) distribution (erg/cm²)
    pub fluence_log_mean: f64,

    /// Standard deviation of log10(fluence) distribution
    pub fluence_log_std: f64,

    /// Mean of log10(T90) distribution (seconds)
    pub t90_log_mean: f64,

    /// Standard deviation of log10(T90) distribution
    pub t90_log_std: f64,
}

/// GRB satellite instrument types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GrbSatellite {
    SwiftBAT,
    FermiGBM,
}

impl GrbSatellite {
    /// Get typical localization error radius (degrees, 90% CR)
    pub fn localization_error_90(&self) -> f64 {
        match self {
            GrbSatellite::SwiftBAT => 0.05, // ~3 arcmin (0.05 deg)
            GrbSatellite::FermiGBM => 5.0,  // ~5 degrees
        }
    }

    /// Get field of view fraction
    pub fn fov_fraction(&self) -> f64 {
        match self {
            GrbSatellite::SwiftBAT => 0.17, // 1/6 sky
            GrbSatellite::FermiGBM => 0.67, // 2/3 sky
        }
    }

    /// Get fluence threshold (erg/cm²)
    pub fn fluence_threshold(&self) -> f64 {
        match self {
            GrbSatellite::SwiftBAT => 5e-8, // ~5×10⁻⁸ erg/cm²
            GrbSatellite::FermiGBM => 1e-7, // ~10⁻⁷ erg/cm²
        }
    }
}

impl Default for BackgroundGrbConfig {
    fn default() -> Self {
        Self::swift_bat()
    }
}

impl BackgroundGrbConfig {
    /// Swift BAT configuration
    pub fn swift_bat() -> Self {
        Self {
            rate_per_year: 100.0, // ~100 SGRBs/year (Swift BAT alone)
            fluence_threshold: 5e-8,
            fov_fraction: 0.17,
            satellite: GrbSatellite::SwiftBAT,
            fluence_log_mean: -7.3, // log10(5e-8) ≈ -7.3
            fluence_log_std: 0.5,
            t90_log_mean: 0.0, // log10(1.0) = 0 (1 second)
            t90_log_std: 0.3,
        }
    }

    /// Fermi GBM configuration
    pub fn fermi_gbm() -> Self {
        Self {
            rate_per_year: 200.0, // ~200 SGRBs/year (Fermi GBM)
            fluence_threshold: 1e-7,
            fov_fraction: 0.67,
            satellite: GrbSatellite::FermiGBM,
            fluence_log_mean: -6.7, // log10(2e-7) ≈ -6.7
            fluence_log_std: 0.5,
            t90_log_mean: 0.0,
            t90_log_std: 0.3,
        }
    }

    /// Combined Swift + Fermi (for all-sky coverage)
    pub fn combined() -> Self {
        Self {
            rate_per_year: 300.0, // ~300 SGRBs/year (combined)
            fluence_threshold: 5e-8,
            fov_fraction: 1.0,                 // Assume full sky coverage
            satellite: GrbSatellite::FermiGBM, // Use Fermi localization
            fluence_log_mean: -7.0,
            fluence_log_std: 0.5,
            t90_log_mean: 0.0,
            t90_log_std: 0.3,
        }
    }
}

/// Background GRB event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundGrb {
    /// GPS trigger time (seconds)
    pub gps_time: f64,

    /// Right ascension (degrees, J2000)
    pub ra: f64,

    /// Declination (degrees, J2000)
    pub dec: f64,

    /// Bolometric fluence (erg/cm²)
    pub fluence: f64,

    /// Duration T90 (seconds)
    pub t90: f64,

    /// Localization error radius (degrees, 90% confidence region)
    pub localization_error_90: f64,

    /// Satellite that detected this GRB
    pub satellite: GrbSatellite,

    /// GRB identifier (for tracking)
    pub grb_id: String,
}

/// Generate background GRBs as a Poisson process
///
/// # Arguments
///
/// * `config` - Background GRB configuration
/// * `time_start` - Start GPS time (seconds)
/// * `time_end` - End GPS time (seconds)
/// * `rng` - Random number generator
///
/// # Returns
///
/// Vector of background GRBs uniformly distributed in time and space
///
/// # Example
///
/// ```
/// use mm_simulation::background_grbs::{generate_background_grbs, BackgroundGrbConfig};
/// use rand::thread_rng;
///
/// let config = BackgroundGrbConfig::swift_bat();
/// let mut rng = thread_rng();
///
/// // O4 observing run: 1 year starting at GPS 1356566418 (May 2023)
/// let t_start = 1356566418.0;
/// let t_end = t_start + 365.25 * 86400.0;  // 1 year later
///
/// let grbs = generate_background_grbs(&config, t_start, t_end, &mut rng);
/// println!("Generated {} background GRBs", grbs.len());
/// ```
pub fn generate_background_grbs(
    config: &BackgroundGrbConfig,
    time_start: f64,
    time_end: f64,
    rng: &mut impl Rng,
) -> Vec<BackgroundGrb> {
    let duration_seconds = time_end - time_start;
    let duration_years = duration_seconds / (365.25 * 86400.0);

    // Expected number of GRBs in this period (accounting for FOV)
    let expected_count = config.rate_per_year * duration_years * config.fov_fraction;

    // Sample actual count from Poisson distribution
    // For large λ, Poisson(λ) ≈ Normal(λ, sqrt(λ))
    let count = if expected_count > 30.0 {
        let std_dev = expected_count.sqrt();
        let normal = Normal::new(expected_count, std_dev).unwrap();
        normal.sample(rng).round().max(0.0) as usize
    } else {
        // For small λ, sample from exponential inter-arrival times
        let lambda = config.rate_per_year * config.fov_fraction / (365.25 * 86400.0);
        let exp_dist = Exp::new(lambda).unwrap();

        let mut n = 0;
        let mut t = time_start;
        while t < time_end {
            t += exp_dist.sample(rng);
            if t < time_end {
                n += 1;
            }
        }
        n
    };

    // Generate background GRBs
    let mut grbs = Vec::with_capacity(count);

    let time_dist = Uniform::new(time_start, time_end);
    let ra_dist = Uniform::new(0.0, 360.0); // RA: 0-360 degrees
    let sin_dec_dist = Uniform::new(-1.0, 1.0); // Uniform in sin(dec)

    let fluence_dist = Normal::new(config.fluence_log_mean, config.fluence_log_std).unwrap();
    let t90_dist = Normal::new(config.t90_log_mean, config.t90_log_std).unwrap();

    for i in 0..count {
        // Sample trigger time uniformly
        let gps_time = time_dist.sample(rng);

        // Sample position uniformly on the sky
        let ra = ra_dist.sample(rng);
        let sin_dec: f64 = sin_dec_dist.sample(rng);
        let dec = sin_dec.asin().to_degrees();

        // Sample fluence (log-normal distribution)
        let fluence_log = fluence_dist.sample(rng);
        let fluence = 10_f64.powf(fluence_log).max(config.fluence_threshold);

        // Sample T90 (log-normal distribution)
        let t90_log = t90_dist.sample(rng);
        let t90 = 10_f64.powf(t90_log);

        // Localization error from satellite
        let localization_error_90 = config.satellite.localization_error_90();

        let grb_id = format!("BG{:06}", i);

        grbs.push(BackgroundGrb {
            gps_time,
            ra,
            dec,
            fluence,
            t90,
            localization_error_90,
            satellite: config.satellite,
            grb_id,
        });
    }

    // Sort by trigger time
    grbs.sort_by(|a, b| a.gps_time.partial_cmp(&b.gps_time).unwrap());

    grbs
}

/// Calculate expected number of chance coincidences with GW events
///
/// # Arguments
///
/// * `n_gw_events` - Number of GW events
/// * `grb_rate_per_year` - Background GRB rate (per year)
/// * `time_window_seconds` - Time window for association (seconds)
/// * `skymap_area_deg2` - Typical GW skymap area (sq deg)
///
/// # Returns
///
/// Expected number of chance GW-GRB coincidences
///
/// # Example
///
/// ```
/// use mm_simulation::background_grbs::expected_chance_coincidences;
///
/// let n_gw = 50;  // 50 GW events in O4
/// let grb_rate = 300.0;  // 300 SGRBs/year
/// let time_window = 10.0;  // ±5 seconds
/// let skymap_area = 100.0;  // 100 sq deg (typical BNS)
///
/// let expected_false = expected_chance_coincidences(
///     n_gw,
///     grb_rate,
///     time_window,
///     skymap_area,
/// );
///
/// println!("Expected chance coincidences: {:.3}", expected_false);
/// ```
pub fn expected_chance_coincidences(
    n_gw_events: usize,
    grb_rate_per_year: f64,
    time_window_seconds: f64,
    skymap_area_deg2: f64,
) -> f64 {
    let seconds_per_year = 365.25 * 86400.0;

    // Temporal probability: time window / 1 year
    let p_temporal = time_window_seconds / seconds_per_year;

    // Spatial probability: skymap area / full sky (41253 sq deg)
    let p_spatial = skymap_area_deg2 / 41253.0;

    // Expected false associations
    let expected_false = n_gw_events as f64 * grb_rate_per_year * p_temporal * p_spatial;

    expected_false
}

/// Statistics on chance coincidences between GW events and background GRBs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChanceCoincidenceStats {
    /// Total number of GW events
    pub total_gw_events: usize,

    /// Total number of background GRBs
    pub total_background_grbs: usize,

    /// Number of GRBs within time window of any GW event
    pub temporal_coincidences: usize,

    /// Number of GRBs within time AND spatial window
    pub spatial_temporal_coincidences: usize,

    /// Expected number of chance associations (analytical)
    pub expected_false_associations: f64,

    /// Chance coincidence rate (per GW event)
    pub chance_rate_per_gw: f64,
}

/// Calculate chance coincidences between GW events and background GRBs
///
/// # Arguments
///
/// * `gw_times` - GW trigger times (GPS seconds)
/// * `gw_skymap_areas` - GW skymap areas (sq deg, 90% CR)
/// * `background_grbs` - Background GRB events
/// * `time_window` - Time window for association (seconds, ±window/2)
///
/// # Returns
///
/// Statistics on chance coincidences
pub fn calculate_chance_coincidences(
    gw_times: &[f64],
    gw_skymap_areas: &[f64],
    background_grbs: &[BackgroundGrb],
    time_window: f64,
) -> ChanceCoincidenceStats {
    let n_gw = gw_times.len();
    let n_grb = background_grbs.len();

    let mut temporal_coincidences = 0;
    let mut spatial_temporal_coincidences = 0;

    let half_window = time_window / 2.0;

    // For each GW event, count background GRBs within time/space window
    for (i, &gw_time) in gw_times.iter().enumerate() {
        let skymap_area = gw_skymap_areas[i];

        for grb in background_grbs {
            let dt = (grb.gps_time - gw_time).abs();

            if dt <= half_window {
                temporal_coincidences += 1;

                // Simple spatial overlap check: assume circular error regions
                // For now, just check if GRB localization + GW skymap overlap
                // (In practice, you'd use HEALPix probability calculations)

                // Approximate: if skymap is small and localization is good,
                // spatial overlap is proportional to area ratio
                let p_spatial = skymap_area / 41253.0;

                // For demonstration, assume 10% spatial overlap for typical cases
                // In real analysis, compute actual overlap from HEALPix maps
                if p_spatial > 0.01 {
                    spatial_temporal_coincidences += 1;
                }
            }
        }
    }

    // Calculate expected false associations (analytical)
    let mean_skymap_area = gw_skymap_areas.iter().sum::<f64>() / n_gw as f64;
    let grb_rate = n_grb as f64 / (365.25 * 86400.0); // Assuming 1 year

    let expected_false = expected_chance_coincidences(
        n_gw,
        grb_rate * (365.25 * 86400.0), // Convert back to per year
        time_window,
        mean_skymap_area,
    );

    let chance_rate_per_gw = if n_gw > 0 {
        spatial_temporal_coincidences as f64 / n_gw as f64
    } else {
        0.0
    };

    ChanceCoincidenceStats {
        total_gw_events: n_gw,
        total_background_grbs: n_grb,
        temporal_coincidences,
        spatial_temporal_coincidences,
        expected_false_associations: expected_false,
        chance_rate_per_gw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_generate_background_grbs() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = BackgroundGrbConfig::swift_bat();

        // 1 year observation
        let t_start = 1356566418.0;
        let t_end = t_start + 365.25 * 86400.0;

        let grbs = generate_background_grbs(&config, t_start, t_end, &mut rng);

        println!("Generated {} background GRBs", grbs.len());

        // Should generate roughly rate * fov_fraction GRBs
        // Swift BAT: 100/year * 0.17 = ~17 GRBs
        assert!(grbs.len() > 5 && grbs.len() < 50);

        // Check properties
        for grb in &grbs[..5] {
            println!(
                "  GRB {}: GPS={:.0}, RA={:.2}, Dec={:.2}, Fluence={:.2e}",
                grb.grb_id, grb.gps_time, grb.ra, grb.dec, grb.fluence
            );

            assert!(grb.gps_time >= t_start);
            assert!(grb.gps_time <= t_end);
            assert!(grb.ra >= 0.0 && grb.ra <= 360.0);
            assert!(grb.dec >= -90.0 && grb.dec <= 90.0);
            assert!(grb.fluence >= config.fluence_threshold);
        }
    }

    #[test]
    fn test_expected_chance_coincidences() {
        let expected = expected_chance_coincidences(
            50,    // 50 GW events
            300.0, // 300 GRBs/year
            10.0,  // ±5 second window
            100.0, // 100 sq deg skymap
        );

        println!("Expected chance coincidences: {:.6}", expected);

        // Should be very small: ~50 * 300 * (10/31.5M) * (100/41253) ≈ 0.00036
        assert!(expected < 0.01);
        assert!(expected > 0.0);
    }

    #[test]
    fn test_fermi_gbm_config() {
        let config = BackgroundGrbConfig::fermi_gbm();

        assert_eq!(config.satellite, GrbSatellite::FermiGBM);
        assert!(config.fov_fraction > 0.5); // Fermi sees most of sky
        assert!(config.rate_per_year > 150.0);
    }
}
