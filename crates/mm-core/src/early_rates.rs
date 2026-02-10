//! Early linear rise/decay rate computation for source selection
//!
//! Provides a fast, simple discriminator between kilonovae (fast risers/faders)
//! and supernovae (slow risers/faders) using direct linear fits to the earliest
//! light curve observations. Works with as few as 2 detections and runs before
//! the heavier GP-based feature extraction pipeline.
//!
//! Physical basis:
//! - Kilonovae: rise > 1 mag/day, decay > 0.3 mag/day
//! - Type Ia supernovae: rise < 0.5 mag/day, decay < 0.3 mag/day

use crate::lightcurve::LightCurve;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Zero-point for flux-to-magnitude conversion (ZTF microJansky)
const ZP_UJY: f64 = 23.9;

/// Configuration for early linear rate source selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarlyRateConfig {
    /// Enable early rate source selection
    pub enable: bool,

    /// Minimum number of detections needed (minimum 2)
    pub min_detections: usize,

    /// Maximum time window from first detection to consider "early" data (days)
    pub early_window_days: f64,

    /// Minimum rise rate (mag/day) for KN-like classification
    pub kn_min_rise_rate: f64,

    /// Maximum rise rate (mag/day) below which we classify as SN-like
    pub sn_max_rise_rate: f64,

    /// Minimum decay rate (mag/day) for KN-like classification
    pub kn_min_decay_rate: f64,

    /// If true, hard-reject slow risers; if false, apply soft FAR weight
    pub hard_cut: bool,

    /// FAR multiplier for SN-like slow risers (> 1.0 = penalize)
    pub sn_penalty: f64,

    /// FAR multiplier for KN-like fast risers (< 1.0 = boost)
    pub kn_boost: f64,
}

impl Default for EarlyRateConfig {
    fn default() -> Self {
        Self {
            enable: true,
            min_detections: 2,
            early_window_days: 3.0,
            kn_min_rise_rate: 1.0,
            sn_max_rise_rate: 0.5,
            kn_min_decay_rate: 0.3,
            hard_cut: false,
            sn_penalty: 5.0,
            kn_boost: 0.3,
        }
    }
}

/// Computed early linear rates from the first observations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarlyRates {
    /// Linear rise rate in mag/day (negative = brightening in mag space)
    pub rise_rate: f64,

    /// Linear decay rate in mag/day (positive = fading in mag space)
    /// NaN if no post-peak data available
    pub decay_rate: f64,

    /// Number of data points used for rise rate
    pub n_rise_points: usize,

    /// Number of data points used for decay rate
    pub n_decay_points: usize,

    /// Time baseline for rise measurement (days)
    pub rise_baseline: f64,

    /// Time baseline for decay measurement (days)
    pub decay_baseline: f64,

    /// Band used for measurement
    pub band: String,
}

/// Result of early source selection scoring
#[derive(Debug, Clone)]
pub enum EarlySelectionResult {
    /// Candidate passes with a FAR multiplier
    Pass { far_multiplier: f64 },
    /// Candidate is rejected (hard cut mode only)
    Reject { reason: String },
}

/// Compute early linear rise/decay rates from the first observations
///
/// Uses direct linear regression on the earliest detections in the
/// best-sampled band. Works with as few as 2 data points.
///
/// Returns `None` if insufficient detections are available.
pub fn compute_early_rates(lc: &LightCurve, config: &EarlyRateConfig) -> Option<EarlyRates> {
    // Filter to detections only (skip upper limits and non-positive flux)
    let detections: Vec<_> = lc
        .measurements
        .iter()
        .filter(|m| !m.is_upper_limit && m.flux > 0.0)
        .collect();

    if detections.len() < config.min_detections {
        debug!(
            "Insufficient detections for early rates: {} < {}",
            detections.len(),
            config.min_detections
        );
        return None;
    }

    // Group by band, pick the one with most points
    let mut by_band: HashMap<&str, Vec<(f64, f64, f64)>> = HashMap::new();
    for m in &detections {
        by_band
            .entry(&m.filter)
            .or_default()
            .push((m.mjd, m.flux, m.flux_err));
    }

    let (best_band, band_data) = by_band.into_iter().max_by_key(|(_, pts)| pts.len())?;

    if band_data.len() < config.min_detections {
        debug!(
            "Best band {} has only {} detections, need {}",
            best_band,
            band_data.len(),
            config.min_detections
        );
        return None;
    }

    // Sort by time
    let mut sorted = band_data;
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let t_first = sorted[0].0;

    // Restrict to early window
    let early: Vec<_> = sorted
        .into_iter()
        .filter(|(t, _, _)| *t - t_first <= config.early_window_days)
        .collect();

    if early.len() < config.min_detections {
        debug!(
            "Only {} detections within {:.1} day early window",
            early.len(),
            config.early_window_days
        );
        return None;
    }

    // Convert to relative time (days) and magnitude
    let times: Vec<f64> = early.iter().map(|(t, _, _)| t - t_first).collect();
    let mags: Vec<f64> = early
        .iter()
        .map(|(_, flux, _)| ZP_UJY - 2.5 * flux.log10())
        .collect();

    // Find peak (minimum magnitude = brightest) within early window
    let peak_idx = mags
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);

    // Rise rate: linear regression on points up to and including peak
    let rise_end = peak_idx + 1;
    let (rise_rate, n_rise, rise_baseline) = if rise_end >= 2 {
        let slope = linear_regression_slope(&times[..rise_end], &mags[..rise_end]);
        let baseline = times[rise_end - 1] - times[0];
        (slope, rise_end, baseline)
    } else if mags.len() >= 2 {
        // Peak is at first point (already brightest); use first 2 points
        let slope = (mags[1] - mags[0]) / (times[1] - times[0]);
        let baseline = times[1] - times[0];
        (slope, 2, baseline)
    } else {
        (f64::NAN, 0, 0.0)
    };

    // Decay rate: linear regression on points at and after peak
    let (decay_rate, n_decay, decay_baseline) = if peak_idx + 1 < mags.len() {
        let decay_times = &times[peak_idx..];
        let decay_mags = &mags[peak_idx..];
        if decay_times.len() >= 2 {
            let slope = linear_regression_slope(decay_times, decay_mags);
            let baseline = decay_times[decay_times.len() - 1] - decay_times[0];
            (slope, decay_times.len(), baseline)
        } else {
            (f64::NAN, 0, 0.0)
        }
    } else {
        (f64::NAN, 0, 0.0)
    };

    Some(EarlyRates {
        rise_rate,
        decay_rate,
        n_rise_points: n_rise,
        n_decay_points: n_decay,
        rise_baseline,
        decay_baseline,
        band: best_band.to_string(),
    })
}

/// Score a candidate based on early linear rates
///
/// Returns either a FAR multiplier (soft mode) or a rejection (hard mode).
pub fn early_source_selection_score(
    rates: &EarlyRates,
    config: &EarlyRateConfig,
) -> EarlySelectionResult {
    let rise_speed = rates.rise_rate.abs();
    let has_rise = rates.n_rise_points >= 2 && rates.rise_baseline > 0.1;

    // Hard cut: reject slow risers with sufficient baseline
    if config.hard_cut
        && has_rise
        && rates.rise_baseline > 0.5
        && rise_speed < config.sn_max_rise_rate
    {
        return EarlySelectionResult::Reject {
            reason: format!(
                "slow riser ({:.3} mag/day over {:.1}d baseline, < {:.1} threshold)",
                rise_speed, rates.rise_baseline, config.sn_max_rise_rate
            ),
        };
    }

    // Soft scoring
    let mut multiplier = 1.0;

    // Rise rate scoring
    if has_rise {
        if rise_speed > config.kn_min_rise_rate {
            multiplier *= config.kn_boost;
        } else if rise_speed < config.sn_max_rise_rate {
            multiplier *= config.sn_penalty;
        }
    }

    // Decay rate scoring (if available)
    let has_decay = rates.n_decay_points >= 2 && rates.decay_rate.is_finite();
    if has_decay {
        let decay_speed = rates.decay_rate.abs();
        if decay_speed < config.kn_min_decay_rate {
            multiplier *= config.sn_penalty.sqrt(); // moderate penalty for slow decay
        } else if decay_speed > config.kn_min_rise_rate {
            multiplier *= config.kn_boost.sqrt(); // moderate boost for fast decay
        }
    }

    EarlySelectionResult::Pass {
        far_multiplier: multiplier,
    }
}

/// Simple linear regression slope (mag/day)
fn linear_regression_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    if n < 2.0 {
        return f64::NAN;
    }

    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xx: f64 = x.iter().map(|xi| xi * xi).sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return f64::NAN;
    }

    (n * sum_xy - sum_x * sum_y) / denom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightcurve::Photometry;

    /// Helper: create a light curve from (mjd, flux) pairs in a given band
    fn make_lc(object_id: &str, band: &str, points: &[(f64, f64)]) -> LightCurve {
        let mut lc = LightCurve::new(object_id.to_string());
        for &(mjd, flux) in points {
            lc.add_measurement(Photometry::new(mjd, flux, flux * 0.05, band.to_string()));
        }
        lc
    }

    #[test]
    fn test_two_point_kn_rise() {
        // KN-like: brightens by ~2 mag in 0.5 days
        // flux goes from 100 to 1000 uJy = ~2.5 mag change
        let lc = make_lc("KN_test", "r", &[(60000.0, 100.0), (60000.5, 1000.0)]);
        let config = EarlyRateConfig::default();

        let rates = compute_early_rates(&lc, &config).expect("should compute rates");
        let rise_speed = rates.rise_rate.abs();

        assert!(
            rise_speed > 1.0,
            "KN rise speed should be > 1 mag/day, got {:.3}",
            rise_speed
        );
        assert_eq!(rates.n_rise_points, 2);
        assert_eq!(rates.band, "r");

        // Should get a boost (multiplier < 1.0)
        match early_source_selection_score(&rates, &config) {
            EarlySelectionResult::Pass { far_multiplier } => {
                assert!(
                    far_multiplier < 1.0,
                    "KN should be boosted, got {:.3}",
                    far_multiplier
                );
            }
            EarlySelectionResult::Reject { reason } => {
                panic!("KN should not be rejected: {}", reason);
            }
        }
    }

    #[test]
    fn test_sn_ia_slow_rise() {
        // SN Ia: rises ~2 mag over 15 days
        // flux goes from 100 to ~630 over 15 days = ~2 mag over 15d = 0.13 mag/day
        let lc = make_lc("SN_test", "r", &[(60000.0, 100.0), (60003.0, 150.0)]);
        let config = EarlyRateConfig::default();

        let rates = compute_early_rates(&lc, &config).expect("should compute rates");
        let rise_speed = rates.rise_rate.abs();

        assert!(
            rise_speed < 0.5,
            "SN rise speed should be < 0.5 mag/day, got {:.3}",
            rise_speed
        );

        // Should get a penalty (multiplier > 1.0) in soft mode
        match early_source_selection_score(&rates, &config) {
            EarlySelectionResult::Pass { far_multiplier } => {
                assert!(
                    far_multiplier > 1.0,
                    "SN should be penalized, got {:.3}",
                    far_multiplier
                );
            }
            EarlySelectionResult::Reject { .. } => {
                panic!("soft mode should not reject");
            }
        }
    }

    #[test]
    fn test_single_point_insufficient() {
        let lc = make_lc("single", "r", &[(60000.0, 500.0)]);
        let config = EarlyRateConfig::default();

        assert!(
            compute_early_rates(&lc, &config).is_none(),
            "single point should return None"
        );
    }

    #[test]
    fn test_hard_cut_rejects_slow_riser() {
        let lc = make_lc(
            "SN_slow",
            "r",
            &[(60000.0, 100.0), (60001.0, 115.0), (60002.0, 130.0)],
        );
        let config = EarlyRateConfig {
            hard_cut: true,
            ..EarlyRateConfig::default()
        };

        let rates = compute_early_rates(&lc, &config).expect("should compute rates");
        match early_source_selection_score(&rates, &config) {
            EarlySelectionResult::Reject { .. } => {} // expected
            EarlySelectionResult::Pass { far_multiplier } => {
                panic!(
                    "hard cut should reject slow riser, got pass with {:.3}",
                    far_multiplier
                );
            }
        }
    }

    #[test]
    fn test_early_window_restriction() {
        // 5 points over 10 days, but early_window = 3 days
        let lc = make_lc(
            "window_test",
            "r",
            &[
                (60000.0, 100.0),
                (60001.0, 500.0),
                (60002.0, 800.0),
                (60005.0, 200.0),
                (60010.0, 50.0),
            ],
        );
        let config = EarlyRateConfig {
            early_window_days: 3.0,
            ..EarlyRateConfig::default()
        };

        let rates = compute_early_rates(&lc, &config).expect("should compute rates");
        // Only points at days 0, 1, 2 should be used (3 points within 3-day window)
        assert!(
            rates.n_rise_points <= 3,
            "should use at most 3 points within window, got {}",
            rates.n_rise_points
        );
    }

    #[test]
    fn test_multiband_selects_best() {
        let mut lc = LightCurve::new("multiband".to_string());
        // g-band: 2 points
        lc.add_measurement(Photometry::new(60000.0, 100.0, 5.0, "g".to_string()));
        lc.add_measurement(Photometry::new(60000.5, 500.0, 25.0, "g".to_string()));
        // r-band: 4 points
        lc.add_measurement(Photometry::new(60000.0, 80.0, 4.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60000.3, 200.0, 10.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60000.6, 600.0, 30.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60001.0, 400.0, 20.0, "r".to_string()));

        let config = EarlyRateConfig::default();
        let rates = compute_early_rates(&lc, &config).expect("should compute rates");

        assert_eq!(rates.band, "r", "should select r-band (more points)");
    }

    #[test]
    fn test_kn_with_decay() {
        // KN with both rise and decay visible
        let lc = make_lc(
            "KN_decay",
            "r",
            &[
                (60000.0, 100.0),  // start
                (60000.5, 1000.0), // peak
                (60001.5, 200.0),  // fading
                (60002.5, 50.0),   // fading more
            ],
        );
        let config = EarlyRateConfig::default();

        let rates = compute_early_rates(&lc, &config).expect("should compute rates");

        // Rise should be fast
        assert!(
            rates.rise_rate.abs() > 1.0,
            "KN rise should be fast, got {:.3}",
            rates.rise_rate.abs()
        );

        // Decay should be positive (fading = magnitude increasing)
        assert!(
            rates.decay_rate > 0.0,
            "decay rate should be positive (fading), got {:.3}",
            rates.decay_rate
        );

        // Decay speed should indicate fast fader
        assert!(
            rates.decay_rate.abs() > 0.3,
            "KN decay should be > 0.3 mag/day, got {:.3}",
            rates.decay_rate.abs()
        );

        assert!(rates.n_decay_points >= 2);
    }

    #[test]
    fn test_linear_regression_slope_basic() {
        // Perfect line: y = 2x + 1
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![1.0, 3.0, 5.0, 7.0];
        let slope = linear_regression_slope(&x, &y);
        assert!(
            (slope - 2.0).abs() < 1e-10,
            "slope should be 2.0, got {:.6}",
            slope
        );
    }

    #[test]
    fn test_upper_limits_excluded() {
        let mut lc = LightCurve::new("upper_test".to_string());
        // Add upper limits (should be ignored)
        lc.add_measurement(Photometry::new_upper_limit(59999.0, 50.0, "r".to_string()));
        lc.add_measurement(Photometry::new_upper_limit(59999.5, 50.0, "r".to_string()));
        // Add real detections
        lc.add_measurement(Photometry::new(60000.0, 100.0, 5.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60000.5, 800.0, 40.0, "r".to_string()));

        let config = EarlyRateConfig::default();
        let rates = compute_early_rates(&lc, &config).expect("should compute rates");

        // Should only use the 2 real detections
        assert_eq!(rates.n_rise_points, 2);
        assert!(rates.rise_rate.abs() > 1.0, "should detect fast rise");
    }
}
