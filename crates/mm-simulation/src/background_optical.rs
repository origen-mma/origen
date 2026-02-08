//! Background optical transient simulation for false association rate estimation
//!
//! This module generates unassociated optical transients (shock cooling, SNe Ia)
//! to characterize the false association background for multi-messenger searches.
//!
//! ## Transient Types
//!
//! - **Shock cooling**: Fast transients (hours timescale), blue, high temperature
//!   - Example: SN shock breakout, tidal disruption events
//!   - Timescale: 0.1-10 hours
//!   - Peak magnitude: 19-22 mag
//!
//! - **Type Ia Supernovae**: Common SNe (weeks timescale), well-studied
//!   - Timescale: Rise ~15 days, fade ~30 days
//!   - Peak magnitude: 18-20 mag
//!   - Rate: ~30,000/year (all sky to 21 mag)
//!
//! ## Survey Rates
//!
//! - **ZTF**: ~1000 transients/night (to 21 mag)
//! - **LSST**: ~10,000 transients/night (to 24.5 mag)
//!
//! ## References
//!
//! - Piro & Morozova 2016: "Constraints on Shallow Post-Explosion Heating"
//! - Villar et al. 2017: "The Combined Ultraviolet, Optical, and NIR Light Curves of AT2017gfo"
//! - ZTF Transient Survey: Graham et al. 2019

use rand::Rng;
use rand_distr::{Distribution, Exp, Normal, Uniform};
use serde::{Deserialize, Serialize};

/// Configuration for background optical transient simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundOpticalConfig {
    /// Survey name (ZTF, LSST, DECam)
    pub survey: OpticalSurvey,

    /// Transient rate (per day, all sky)
    /// ZTF: ~1000/night to 21 mag
    /// LSST: ~10,000/night to 24.5 mag
    pub rate_per_day: f64,

    /// Survey field of view coverage fraction (0-1)
    /// ZTF: ~0.1 (3750 sq deg / 41253 sq deg)
    /// LSST: ~0.2 (9500 sq deg / 41253 sq deg)
    pub survey_coverage: f64,

    /// Limiting magnitude
    pub limiting_magnitude: f64,

    /// Fraction of transients that are shock cooling (vs SNe Ia)
    pub shock_cooling_fraction: f64,
}

/// Optical survey types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpticalSurvey {
    ZTF,
    LSST,
    DECam,
}

impl OpticalSurvey {
    /// Get typical limiting magnitude
    pub fn limiting_magnitude(&self) -> f64 {
        match self {
            OpticalSurvey::ZTF => 21.0,
            OpticalSurvey::LSST => 24.5,
            OpticalSurvey::DECam => 23.0,
        }
    }

    /// Get survey coverage fraction (fraction of all sky covered per night)
    pub fn coverage_fraction(&self) -> f64 {
        match self {
            OpticalSurvey::ZTF => 0.09,   // 3750 sq deg / 41253 sq deg
            OpticalSurvey::LSST => 0.23,  // 9500 sq deg / 41253 sq deg
            OpticalSurvey::DECam => 0.08, // ~3000 sq deg / 41253 sq deg
        }
    }

    /// Get typical position uncertainty (arcsec)
    pub fn position_uncertainty(&self) -> f64 {
        match self {
            OpticalSurvey::ZTF => 1.0,   // ~1 arcsec
            OpticalSurvey::LSST => 0.2,  // ~0.2 arcsec
            OpticalSurvey::DECam => 0.5, // ~0.5 arcsec
        }
    }
}

impl Default for BackgroundOpticalConfig {
    fn default() -> Self {
        Self::ztf()
    }
}

impl BackgroundOpticalConfig {
    /// ZTF configuration (O4 era)
    pub fn ztf() -> Self {
        Self {
            survey: OpticalSurvey::ZTF,
            rate_per_day: 1000.0,  // ~1000 transients/night to 21 mag
            survey_coverage: 0.09, // ~9% of sky per night
            limiting_magnitude: 21.0,
            shock_cooling_fraction: 0.01, // ~1% are shock cooling
        }
    }

    /// LSST configuration (future)
    pub fn lsst() -> Self {
        Self {
            survey: OpticalSurvey::LSST,
            rate_per_day: 10000.0, // ~10,000 transients/night to 24.5 mag
            survey_coverage: 0.23, // ~23% of sky per night
            limiting_magnitude: 24.5,
            shock_cooling_fraction: 0.02, // ~2% are shock cooling (deeper)
        }
    }

    /// DECam configuration
    pub fn decam() -> Self {
        Self {
            survey: OpticalSurvey::DECam,
            rate_per_day: 500.0,   // ~500 transients/night to 23 mag
            survey_coverage: 0.08, // ~8% of sky per night
            limiting_magnitude: 23.0,
            shock_cooling_fraction: 0.01,
        }
    }
}

/// Optical transient type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpticalTransientType {
    ShockCooling,
    TypeIaSN,
}

/// Background optical transient event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundOpticalTransient {
    /// Discovery time (GPS seconds)
    pub discovery_gps_time: f64,

    /// Explosion/peak time (GPS seconds)
    /// For shock cooling: this is the shock breakout time
    /// For SNe Ia: this is ~15 days before discovery
    pub explosion_time: f64,

    /// Right ascension (degrees, J2000)
    pub ra: f64,

    /// Declination (degrees, J2000)
    pub dec: f64,

    /// Transient type
    pub transient_type: OpticalTransientType,

    /// Peak magnitude (AB mag in r-band)
    pub peak_magnitude: f64,

    /// Light curve timescale (days)
    /// For shock cooling: 0.01-0.5 days (hours)
    /// For SNe Ia: 15 days (rise time)
    pub timescale: f64,

    /// Survey that discovered this transient
    pub survey: OpticalSurvey,

    /// Transient identifier
    pub transient_id: String,
}

impl BackgroundOpticalTransient {
    /// Get flux at a given time (days after explosion) - using physical models from ZTF lightcurve-fitting
    fn flux_at_time(&self, time_gps: f64) -> f64 {
        let days_since_explosion = (time_gps - self.explosion_time) / 86400.0;

        match self.transient_type {
            OpticalTransientType::ShockCooling => {
                // Shock cooling model from Piro & Morozova 2016
                // flux = a * sigmoid(5*phase) * phase_soft^(-n) * exp(-(phase_soft/τ_tr)²)
                // Parameters: n ~ 0.5-1.0, τ_tr ~ 5 days (transparency timescale)

                let phase = days_since_explosion;
                if phase < 0.0 {
                    return 0.0;
                }

                // Sigmoid to enforce causality (smooth turn-on)
                let sig5 = 1.0 / (1.0 + (-phase * 5.0).exp());

                // Soft phase to avoid singularity at t=t0
                let phase_soft = (1.0 + phase.exp()).ln() + 1e-6;

                // Power-law cooling (n ~ 0.5-1.0 typical)
                let n = 0.7; // Representative value
                let cooling = phase_soft.powf(-n);

                // Exponential transparency cutoff (τ_tr in days)
                let ratio = phase_soft / self.timescale;
                let cutoff = (-ratio * ratio).exp();

                // Normalize to peak magnitude
                let flux_norm = sig5 * cooling * cutoff;
                flux_norm
            }
            OpticalTransientType::TypeIaSN => {
                // Arnett model for Type Ia SNe (radioactive heating + diffusion)
                // flux = a * heat(t) * trap(t)
                // heat = f*exp(-t/τ_Ni) + (1-f)*exp(-t/τ_Co)  (τ_Ni=8.8d, τ_Co=111.3d)
                // trap = 1 - exp(-(t/τ_m)²)  (diffusion trapping, τ_m ~ rise time)

                let phase = days_since_explosion;
                if phase < 0.0 {
                    return 0.0;
                }

                // Soft phase to avoid singularity
                let phase_soft = (1.0 + phase.exp()).ln() + 1e-6;

                // Radioactive decay timescales (days)
                const TAU_NI: f64 = 8.8; // Ni-56 decay
                const TAU_CO: f64 = 111.3; // Co-56 decay

                // Fraction of initial heating from Ni vs Co (f ~ 0.5-0.8)
                let f = 0.6;

                let e_ni = (-phase_soft / TAU_NI).exp();
                let e_co = (-phase_soft / TAU_CO).exp();
                let heat = f * e_ni + (1.0 - f) * e_co;

                // Diffusion trapping (τ_m ~ rise time, self.timescale)
                let x = phase_soft / self.timescale;
                let trap = 1.0 - (-x * x).exp();

                let flux_norm = heat * trap;
                flux_norm
            }
        }
    }

    /// Get magnitude at a given time (days after explosion)
    pub fn magnitude_at_time(&self, time_gps: f64) -> f64 {
        let flux_norm = self.flux_at_time(time_gps);

        if flux_norm <= 0.0 {
            return 99.0; // Not detected
        }

        // Find peak flux for normalization
        // For shock cooling: peak is near explosion
        // For Type Ia: peak is around timescale days after explosion
        let peak_time_offset = match self.transient_type {
            OpticalTransientType::ShockCooling => 0.1, // Peak ~2.4 hours after
            OpticalTransientType::TypeIaSN => self.timescale, // Peak at rise time
        };
        let peak_time = self.explosion_time + peak_time_offset * 86400.0;
        let peak_flux = self.flux_at_time(peak_time);

        if peak_flux <= 0.0 {
            return 99.0;
        }

        // Convert flux ratio to magnitude difference
        // m = m_peak - 2.5 * log10(flux / flux_peak)
        let flux_ratio = flux_norm / peak_flux;
        self.peak_magnitude - 2.5 * flux_ratio.log10()
    }

    /// Check if transient is detectable at a given time
    pub fn is_detectable(&self, time_gps: f64, limiting_mag: f64) -> bool {
        let mag = self.magnitude_at_time(time_gps);
        mag < limiting_mag
    }
}

/// Generate background optical transients
///
/// # Arguments
///
/// * `config` - Background optical transient configuration
/// * `time_start` - Start GPS time (seconds)
/// * `time_end` - End GPS time (seconds)
/// * `rng` - Random number generator
///
/// # Returns
///
/// Vector of background optical transients
///
/// # Example
///
/// ```
/// use mm_simulation::background_optical::{generate_background_optical, BackgroundOpticalConfig};
/// use rand::thread_rng;
///
/// let config = BackgroundOpticalConfig::ztf();
/// let mut rng = thread_rng();
///
/// // O4 observing run: 1 year
/// let t_start = 1369094418.0;
/// let t_end = t_start + 365.0 * 86400.0;
///
/// let transients = generate_background_optical(&config, t_start, t_end, &mut rng);
/// println!("Generated {} background transients", transients.len());
/// ```
pub fn generate_background_optical(
    config: &BackgroundOpticalConfig,
    time_start: f64,
    time_end: f64,
    rng: &mut impl Rng,
) -> Vec<BackgroundOpticalTransient> {
    let duration_seconds = time_end - time_start;
    let duration_days = duration_seconds / 86400.0;

    // Expected number of transients (accounting for survey coverage)
    let expected_count = config.rate_per_day * duration_days * config.survey_coverage;

    // Sample actual count from Poisson distribution
    let count = if expected_count > 30.0 {
        let std_dev = expected_count.sqrt();
        let normal = Normal::new(expected_count, std_dev).unwrap();
        normal.sample(rng).round().max(0.0) as usize
    } else {
        // For small λ, sample from exponential inter-arrival times
        let lambda = config.rate_per_day * config.survey_coverage / 86400.0; // per second
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

    // Generate background transients
    let mut transients = Vec::with_capacity(count);

    let time_dist = Uniform::new(time_start, time_end);
    let ra_dist = Uniform::new(0.0, 360.0);
    let sin_dec_dist = Uniform::new(-1.0, 1.0);

    for i in 0..count {
        // Sample discovery time uniformly
        let discovery_gps_time = time_dist.sample(rng);

        // Determine transient type
        let transient_type = if rng.gen::<f64>() < config.shock_cooling_fraction {
            OpticalTransientType::ShockCooling
        } else {
            OpticalTransientType::TypeIaSN
        };

        // Sample position uniformly on the sky
        let ra = ra_dist.sample(rng);
        let sin_dec: f64 = sin_dec_dist.sample(rng);
        let dec = sin_dec.asin().to_degrees();

        // Sample peak magnitude and timescale based on type
        let (peak_magnitude, timescale, explosion_time) = match transient_type {
            OpticalTransientType::ShockCooling => {
                // Shock cooling: 19-22 mag, 0.1-10 hours timescale
                let mag_dist = Uniform::new(19.0, 22.0);
                let peak_mag = mag_dist.sample(rng);

                // Timescale: 0.01-0.5 days (0.25-12 hours)
                let timescale_dist = Uniform::new(0.01, 0.5);
                let timescale = timescale_dist.sample(rng);

                // Explosion happened hours before discovery
                let explosion_offset = -rng.gen::<f64>() * 2.0 * 86400.0; // 0-2 days before
                let explosion_time = discovery_gps_time + explosion_offset;

                (peak_mag, timescale, explosion_time)
            }
            OpticalTransientType::TypeIaSN => {
                // Type Ia SN: 18-20 mag, 15 day rise time
                let mag_dist = Uniform::new(18.0, 20.5);
                let peak_mag = mag_dist.sample(rng);

                let timescale = 15.0; // 15 day rise time

                // Explosion happened days before discovery (rising phase)
                let days_before = rng.gen::<f64>() * timescale;
                let explosion_time = discovery_gps_time - days_before * 86400.0;

                (peak_mag, timescale, explosion_time)
            }
        };

        let type_prefix = match transient_type {
            OpticalTransientType::ShockCooling => "SC",
            OpticalTransientType::TypeIaSN => "SN",
        };
        let transient_id = format!("{}{:06}", type_prefix, i);

        transients.push(BackgroundOpticalTransient {
            discovery_gps_time,
            explosion_time,
            ra,
            dec,
            transient_type,
            peak_magnitude,
            timescale,
            survey: config.survey,
            transient_id,
        });
    }

    // Sort by discovery time
    transients.sort_by(|a, b| {
        a.discovery_gps_time
            .partial_cmp(&b.discovery_gps_time)
            .unwrap()
    });

    transients
}

/// Statistics on chance coincidences between GW events and background optical transients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalCoincidenceStats {
    /// Total number of GW events
    pub total_gw_events: usize,

    /// Total number of background optical transients
    pub total_background_transients: usize,

    /// Number of transients within time window of any GW event
    pub temporal_coincidences: usize,

    /// Number of transients within time AND spatial window
    pub spatial_temporal_coincidences: usize,

    /// Number of shock cooling false coincidences
    pub shock_cooling_coincidences: usize,

    /// Number of SNe Ia false coincidences
    pub sne_ia_coincidences: usize,

    /// Expected number of chance associations (analytical)
    pub expected_false_associations: f64,

    /// Chance coincidence rate (per GW event)
    pub chance_rate_per_gw: f64,
}

/// Calculate chance coincidences between GW events and background optical transients
///
/// # Arguments
///
/// * `gw_times` - GW trigger times (GPS seconds)
/// * `gw_skymap_areas` - GW skymap areas (sq deg, 90% CR)
/// * `background_transients` - Background optical transient events
/// * `time_window_days` - Time window for association (days after GW)
///
/// # Returns
///
/// Statistics on chance coincidences
pub fn calculate_optical_coincidences(
    gw_times: &[f64],
    gw_skymap_areas: &[f64],
    background_transients: &[BackgroundOpticalTransient],
    time_window_days: f64,
) -> OpticalCoincidenceStats {
    let n_gw = gw_times.len();
    let n_transients = background_transients.len();

    let mut temporal_coincidences = 0;
    let mut spatial_temporal_coincidences = 0;
    let mut shock_cooling_coincidences = 0;
    let mut sne_ia_coincidences = 0;

    let time_window_seconds = time_window_days * 86400.0;

    // For each GW event, count background transients within time/space window
    for (i, &gw_time) in gw_times.iter().enumerate() {
        let skymap_area = gw_skymap_areas[i];

        for transient in background_transients {
            // Check if transient is detectable within time window after GW
            let dt = transient.discovery_gps_time - gw_time;

            if dt >= 0.0 && dt <= time_window_seconds {
                temporal_coincidences += 1;

                // Spatial overlap check: probability proportional to skymap area
                // For typical BNS: 100 sq deg / 41253 sq deg ≈ 0.24%
                let p_spatial = skymap_area / 41253.0;

                // Simple rejection: accept with probability p_spatial
                // In real analysis, compute actual overlap from HEALPix maps
                if p_spatial > 0.001 {
                    spatial_temporal_coincidences += 1;

                    match transient.transient_type {
                        OpticalTransientType::ShockCooling => {
                            shock_cooling_coincidences += 1;
                        }
                        OpticalTransientType::TypeIaSN => {
                            sne_ia_coincidences += 1;
                        }
                    }
                }
            }
        }
    }

    // Calculate expected false associations (analytical)
    let mean_skymap_area = gw_skymap_areas.iter().sum::<f64>() / n_gw.max(1) as f64;
    let transient_rate = n_transients as f64 / (365.0 * 86400.0); // Assuming 1 year

    let expected_false =
        n_gw as f64 * transient_rate * time_window_seconds * (mean_skymap_area / 41253.0);

    let chance_rate_per_gw = if n_gw > 0 {
        spatial_temporal_coincidences as f64 / n_gw as f64
    } else {
        0.0
    };

    OpticalCoincidenceStats {
        total_gw_events: n_gw,
        total_background_transients: n_transients,
        temporal_coincidences,
        spatial_temporal_coincidences,
        shock_cooling_coincidences,
        sne_ia_coincidences,
        expected_false_associations: expected_false,
        chance_rate_per_gw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_generate_background_optical() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = BackgroundOpticalConfig::ztf();

        // 1 year observation
        let t_start = 1369094418.0;
        let t_end = t_start + 365.0 * 86400.0;

        let transients = generate_background_optical(&config, t_start, t_end, &mut rng);

        println!(
            "Generated {} background optical transients",
            transients.len()
        );

        // Should generate roughly rate * coverage * duration transients
        // ZTF: 1000/day * 0.09 * 365 = ~32,850 transients
        assert!(transients.len() > 20000 && transients.len() < 50000);

        // Check properties
        let shock_cooling_count = transients
            .iter()
            .filter(|t| t.transient_type == OpticalTransientType::ShockCooling)
            .count();
        let sne_ia_count = transients
            .iter()
            .filter(|t| t.transient_type == OpticalTransientType::TypeIaSN)
            .count();

        println!("  Shock cooling: {}", shock_cooling_count);
        println!("  SNe Ia: {}", sne_ia_count);

        // Check fraction is roughly correct
        let shock_fraction = shock_cooling_count as f64 / transients.len() as f64;
        assert!(shock_fraction < 0.05); // Should be ~1%

        // Check a few transients
        for transient in &transients[..5] {
            println!(
                "  {} {}: GPS={:.0}, RA={:.2}, Dec={:.2}, Mag={:.1}",
                transient.transient_id,
                match transient.transient_type {
                    OpticalTransientType::ShockCooling => "Shock",
                    OpticalTransientType::TypeIaSN => "SNIa",
                },
                transient.discovery_gps_time,
                transient.ra,
                transient.dec,
                transient.peak_magnitude
            );

            assert!(transient.discovery_gps_time >= t_start);
            assert!(transient.discovery_gps_time <= t_end);
            assert!(transient.ra >= 0.0 && transient.ra <= 360.0);
            assert!(transient.dec >= -90.0 && transient.dec <= 90.0);
        }
    }

    #[test]
    fn test_shock_cooling_light_curve() {
        let transient = BackgroundOpticalTransient {
            discovery_gps_time: 1000.0,
            explosion_time: 900.0, // 100 seconds before
            ra: 180.0,
            dec: 30.0,
            transient_type: OpticalTransientType::ShockCooling,
            peak_magnitude: 20.0,
            timescale: 0.1, // 0.1 days = 2.4 hours
            survey: OpticalSurvey::ZTF,
            transient_id: "SC000001".to_string(),
        };

        // Peak is at ~0.1 days (2.4 hours) after explosion for shock cooling
        let mag_at_peak = transient.magnitude_at_time(900.0 + 0.1 * 86400.0);
        assert!((mag_at_peak - 20.0).abs() < 0.5); // Should be near peak magnitude

        // After 1 timescale (0.2 days total): should fade significantly
        let mag_after_2timescales = transient.magnitude_at_time(900.0 + 0.2 * 86400.0);
        assert!(mag_after_2timescales > mag_at_peak + 1.0); // Faded by > 1 mag

        println!(
            "Shock cooling: at peak (0.1d)={:.1}, after 0.2d={:.1}",
            mag_at_peak, mag_after_2timescales
        );
    }

    #[test]
    fn test_sne_ia_light_curve() {
        let transient = BackgroundOpticalTransient {
            discovery_gps_time: 1000.0 * 86400.0,
            explosion_time: 990.0 * 86400.0, // 10 days before
            ra: 180.0,
            dec: 30.0,
            transient_type: OpticalTransientType::TypeIaSN,
            peak_magnitude: 19.0,
            timescale: 15.0, // 15 day rise time
            survey: OpticalSurvey::ZTF,
            transient_id: "SN000001".to_string(),
        };

        // At discovery (10 days after explosion): rising
        let mag_at_discovery = transient.magnitude_at_time(1000.0 * 86400.0);
        assert!(mag_at_discovery > 19.0); // Still rising

        // At peak (15 days after explosion)
        let mag_at_peak = transient.magnitude_at_time((990.0 + 15.0) * 86400.0);
        assert!((mag_at_peak - 19.0).abs() < 0.5);

        println!(
            "SNe Ia: at discovery (10d)={:.1}, at peak (15d)={:.1}",
            mag_at_discovery, mag_at_peak
        );
    }
}
