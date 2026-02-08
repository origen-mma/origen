//! Joint False Alarm Rate (FAR) calculation for multi-messenger events
//!
//! This module implements statistical methods to compute the probability that
//! a multi-messenger association (GW + GRB + optical) is a chance coincidence
//! rather than a true astrophysical association.
//!
//! # Overview
//!
//! The joint FAR considers:
//! - **GW detection significance**: Network SNR, false alarm rate
//! - **GRB detection significance**: Trigger rate, fluence threshold
//! - **Optical transient rate**: Background transient rate per solid angle
//! - **Spatial coincidence**: Overlap probability given sky localization
//! - **Temporal coincidence**: Probability within time window
//!
//! # Formula
//!
//! ```text
//! FAR_joint = N_trials × P(spatial) × P(temporal) × Rate_GW × Rate_EM
//! ```
//!
//! Where:
//! - `N_trials` = Number of search trials (e.g., GW events per year × time window)
//! - `P(spatial)` = Probability of spatial overlap (skymap area / 4π sr)
//! - `P(temporal)` = Probability of temporal coincidence
//! - `Rate_GW` = GW event rate (false alarm rate from pipeline)
//! - `Rate_EM` = EM counterpart rate (GRB, optical transient rates)

use serde::{Deserialize, Serialize};

/// Configuration for joint FAR calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointFarConfig {
    /// GW observing time (years)
    pub gw_observing_time: f64,

    /// Background GRB rate (per year, all sky)
    pub grb_rate_per_year: f64,

    /// Background optical transient rate (per year per sq deg)
    pub optical_rate_per_sqdeg_per_year: f64,

    /// Time window for optical search after GW trigger (days)
    pub optical_time_window_days: f64,

    /// Time window for GRB search around GW trigger (seconds)
    pub grb_time_window_seconds: f64,
}

impl Default for JointFarConfig {
    fn default() -> Self {
        Self {
            gw_observing_time: 1.0,                    // 1 year
            grb_rate_per_year: 300.0,                  // ~300 SGRBs/year all sky
            optical_rate_per_sqdeg_per_year: 100.0,   // ~100 transients/sq deg/year (ZTF-like)
            optical_time_window_days: 14.0,           // 2 week search window
            grb_time_window_seconds: 10.0,            // ±5 seconds around GW trigger
        }
    }
}

/// Multi-messenger association for FAR calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMessengerAssociation {
    /// GW event properties
    pub gw_snr: f64,
    pub gw_far_per_year: f64,
    pub skymap_area_90: f64,  // sq deg

    /// GRB properties (if detected)
    pub has_grb: bool,
    pub grb_fluence: Option<f64>,          // erg/cm^2
    pub grb_time_offset: Option<f64>,      // seconds from GW trigger

    /// Optical properties (if detected)
    pub has_optical: bool,
    pub optical_magnitude: Option<f64>,
    pub optical_time_offset: Option<f64>,  // days from GW trigger
}

/// Joint FAR calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointFarResult {
    /// Total joint false alarm rate (per year)
    pub far_per_year: f64,

    /// Significance in sigma (Gaussian equivalent)
    pub significance_sigma: f64,

    /// Component contributions
    pub components: FarComponents,

    /// Number of search trials
    pub n_trials: f64,
}

/// Breakdown of FAR components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarComponents {
    /// GW false alarm rate contribution
    pub gw_far: f64,

    /// Spatial overlap probability
    pub spatial_prob: f64,

    /// Temporal coincidence probability (GRB)
    pub temporal_prob_grb: Option<f64>,

    /// Temporal coincidence probability (optical)
    pub temporal_prob_optical: Option<f64>,

    /// Background EM event rate
    pub em_background_rate: f64,
}

/// Calculate joint FAR for multi-messenger association
///
/// # Arguments
///
/// * `assoc` - Multi-messenger association properties
/// * `config` - FAR calculation configuration
///
/// # Returns
///
/// Joint FAR result with significance and component breakdown
pub fn calculate_joint_far(
    assoc: &MultiMessengerAssociation,
    config: &JointFarConfig,
) -> JointFarResult {
    // 1. GW false alarm rate (from search pipeline)
    let gw_far = assoc.gw_far_per_year;

    // 2. Spatial overlap probability
    // P(spatial) = Ω_90 / (4π steradians) = Area_90 / 41253 sq deg
    let spatial_prob = assoc.skymap_area_90 / 41253.0;

    // 3. Number of GW triggers to search
    let n_gw_triggers = gw_far * config.gw_observing_time;

    // 4. Calculate EM background rates and temporal probabilities
    let mut em_background_rate = 0.0;
    let mut temporal_prob_grb = None;
    let mut temporal_prob_optical = None;
    let mut n_trials = n_gw_triggers;

    // GRB contribution (if detected)
    if assoc.has_grb {
        // Temporal probability: GRB within time window around GW trigger
        // P(temporal) = time_window / (1 year in seconds)
        let seconds_per_year = 365.25 * 24.0 * 3600.0;
        let temporal_prob = config.grb_time_window_seconds / seconds_per_year;
        temporal_prob_grb = Some(temporal_prob);

        // Background GRB rate (all sky)
        let grb_rate_in_area = config.grb_rate_per_year * spatial_prob;
        em_background_rate += grb_rate_in_area;

        // Search trials: GW triggers × GRBs in sky area × temporal window
        n_trials *= grb_rate_in_area * temporal_prob;
    }

    // Optical contribution (if detected)
    if assoc.has_optical {
        // Temporal probability: optical transient within search window
        let temporal_prob = config.optical_time_window_days / 365.25;
        temporal_prob_optical = Some(temporal_prob);

        // Background optical transient rate in localization area
        let optical_rate_in_area = config.optical_rate_per_sqdeg_per_year * assoc.skymap_area_90;
        em_background_rate += optical_rate_in_area;

        // Search trials: GW triggers × optical transients in area × temporal window
        n_trials *= optical_rate_in_area * temporal_prob;
    }

    // 5. Joint FAR calculation
    // FAR_joint = N_trials × P(GW) × P(spatial) × P(temporal) × Rate(EM)
    //
    // Simplified: We've already multiplied the rates and probabilities into n_trials
    let far_per_year = n_trials;

    // 6. Convert to significance (sigma)
    // For Poisson process: p-value = exp(-λ) where λ = 1/FAR
    // Significance: σ = Φ^(-1)(1 - p) where Φ is standard normal CDF
    let significance_sigma = if far_per_year > 0.0 {
        // Expected number of false associations per observing time
        let lambda = 1.0 / far_per_year;

        // Poisson p-value for observing ≥1 event
        let p_value = (-lambda).exp();

        // Convert to Gaussian sigma
        // Using inverse complementary error function approximation
        gaussian_sigma_from_pvalue(p_value)
    } else {
        f64::INFINITY
    };

    JointFarResult {
        far_per_year,
        significance_sigma,
        components: FarComponents {
            gw_far,
            spatial_prob,
            temporal_prob_grb,
            temporal_prob_optical,
            em_background_rate,
        },
        n_trials,
    }
}

/// Convert p-value to Gaussian sigma using inverse error function
///
/// For p < 0.5, σ ≈ √2 × erfc^(-1)(2p)
fn gaussian_sigma_from_pvalue(p: f64) -> f64 {
    if p >= 0.5 {
        return 0.0;
    }

    if p <= 1e-15 {
        // For very small p-values, use asymptotic approximation
        // σ ≈ sqrt(-2 ln(p))
        return (-2.0 * p.ln()).sqrt();
    }

    // Use inverse complementary error function
    // σ = √2 × erfc^(-1)(2p)
    //
    // For computational stability, use series approximation
    let z = -2.0 * p.ln();
    let sqrt_z = z.sqrt();

    // Asymptotic expansion for large sigma
    if z > 10.0 {
        return sqrt_z - (z.ln() + (2.0 * std::f64::consts::PI).ln()) / (2.0 * sqrt_z);
    }

    // For moderate p-values, use Newton-Raphson iteration
    // to solve: p = (1 - erf(σ/√2)) / 2
    let mut sigma = sqrt_z;  // Initial guess

    for _ in 0..10 {
        let x = sigma / std::f64::consts::SQRT_2;
        let erf_x = erf(x);
        let pdf_x = (-(x * x)).exp() / (std::f64::consts::PI.sqrt());

        let f = (1.0 - erf_x) / 2.0 - p;
        let df = -pdf_x / std::f64::consts::SQRT_2;

        sigma -= f / df;

        if f.abs() < 1e-10 {
            break;
        }
    }

    sigma
}

/// Error function approximation
fn erf(x: f64) -> f64 {
    // Abramowitz and Stegun approximation (error < 1.5e-7)
    let a1 =  0.254829592;
    let a2 = -0.284496736;
    let a3 =  1.421413741;
    let a4 = -1.453152027;
    let a5 =  1.061405429;
    let p  =  0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    sign * y
}

/// Calculate Pastro (astrophysical probability) from FAR
///
/// P_astro = 1 / (1 + FAR × T_obs)
///
/// where T_obs is the observation time
pub fn calculate_pastro(far_per_year: f64, t_obs_years: f64) -> f64 {
    1.0 / (1.0 + far_per_year * t_obs_years)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gw170817_like_far() {
        // GW170817-like event: nearby BNS with kilonova
        let assoc = MultiMessengerAssociation {
            gw_snr: 32.4,
            gw_far_per_year: 1.0 / 1e5,  // Very significant GW detection
            skymap_area_90: 28.0,          // Small localization area (sq deg)
            has_grb: true,
            grb_fluence: Some(1e-7),       // Weak GRB (far off-axis)
            grb_time_offset: Some(1.7),    // 1.7 seconds after GW
            has_optical: true,
            optical_magnitude: Some(17.0), // Bright kilonova
            optical_time_offset: Some(0.5), // Detected 12 hours after
        };

        let config = JointFarConfig::default();
        let result = calculate_joint_far(&assoc, &config);

        println!("\nGW170817-like FAR calculation:");
        println!("  Joint FAR: {:.2e} per year", result.far_per_year);
        println!("  Significance: {:.1} sigma", result.significance_sigma);
        println!("  Spatial prob: {:.4}", result.components.spatial_prob);
        println!("  N trials: {:.2e}", result.n_trials);

        // Should be highly significant
        assert!(result.significance_sigma > 5.0, "GW170817-like event should be >5σ");
        assert!(result.far_per_year < 1.0, "FAR should be much less than 1/year");
    }

    #[test]
    fn test_typical_o4_event_far() {
        // Typical O4 event: distant BNS with on-axis GRB
        let assoc = MultiMessengerAssociation {
            gw_snr: 12.0,
            gw_far_per_year: 1.0,          // ~1 per year false alarm rate
            skymap_area_90: 500.0,         // Large localization (typical O4)
            has_grb: true,
            grb_fluence: Some(1e-6),       // Moderate GRB
            grb_time_offset: Some(0.5),    // 0.5 seconds after GW
            has_optical: true,
            optical_magnitude: Some(22.0), // Faint afterglow
            optical_time_offset: Some(1.0), // 1 day after
        };

        let config = JointFarConfig::default();
        let result = calculate_joint_far(&assoc, &config);

        println!("\nTypical O4 event FAR calculation:");
        println!("  Joint FAR: {:.2e} per year", result.far_per_year);
        println!("  Significance: {:.1} sigma", result.significance_sigma);
        println!("  Spatial prob: {:.4}", result.components.spatial_prob);
        println!("  EM background rate: {:.2e}", result.components.em_background_rate);

        // Should still be significant, but less than GW170817
        assert!(result.significance_sigma > 3.0, "Typical O4 event should be >3σ");
    }

    #[test]
    fn test_gw_only_far() {
        // GW-only event (no EM counterpart)
        let assoc = MultiMessengerAssociation {
            gw_snr: 15.0,
            gw_far_per_year: 0.1,
            skymap_area_90: 300.0,
            has_grb: false,
            grb_fluence: None,
            grb_time_offset: None,
            has_optical: false,
            optical_magnitude: None,
            optical_time_offset: None,
        };

        let config = JointFarConfig::default();
        let result = calculate_joint_far(&assoc, &config);

        println!("\nGW-only event FAR calculation:");
        println!("  Joint FAR: {:.2e} per year", result.far_per_year);
        println!("  Significance: {:.1} sigma", result.significance_sigma);

        // FAR should just be the GW FAR
        assert!((result.far_per_year - assoc.gw_far_per_year).abs() < 1e-6);
    }

    #[test]
    fn test_pastro_calculation() {
        // High significance event
        let pastro_high = calculate_pastro(1e-5, 1.0);
        assert!(pastro_high > 0.99, "High significance should have Pastro > 99%");

        // Marginal event
        let pastro_marginal = calculate_pastro(1.0, 1.0);
        assert!(pastro_marginal > 0.4 && pastro_marginal < 0.6, "Marginal event should have Pastro ~ 50%");

        // Low significance event
        let pastro_low = calculate_pastro(10.0, 1.0);
        assert!(pastro_low < 0.1, "Low significance should have Pastro < 10%");
    }

    #[test]
    fn test_sigma_conversion() {
        // Test known conversions
        let sigma_3 = gaussian_sigma_from_pvalue(0.0013499);  // 3σ
        let sigma_5 = gaussian_sigma_from_pvalue(2.867e-7);  // 5σ

        println!("\nSigma conversions:");
        println!("  p = 0.0013499 → {:.2} sigma (expected 3.0)", sigma_3);
        println!("  p = 2.867e-7 → {:.2} sigma (expected 5.0)", sigma_5);

        // Allow for numerical precision in conversion
        assert!((sigma_3 - 3.0).abs() < 0.15, "Should convert to ~3σ, got {:.2}", sigma_3);
        assert!((sigma_5 - 5.0).abs() < 0.2, "Should convert to ~5σ, got {:.2}", sigma_5);
    }
}
