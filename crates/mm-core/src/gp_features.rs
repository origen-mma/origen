//! GP-based light curve feature extraction for optical background rejection
//!
//! Uses Gaussian Process regression to smooth light curves and extract
//! physical features (rise rate, decay rate, FWHM, derivatives) that
//! distinguish kilonova candidates from background transients.
//!
//! Filtering criteria from counterpart searches:
//! - Fast risers: > 1.0 mag/day (KN-consistent)
//! - Slow decayers: < 0.3 mag/day (SN-like background)

use crate::lightcurve::LightCurve;
use scirs2_core::ndarray::{Array1, Axis};
use serde::{Deserialize, Serialize};
use sklears_core::traits::{Fit, Predict};
use sklears_gaussian_process::{
    kernels::{ConstantKernel, ProductKernel, SumKernel, WhiteKernel, RBF},
    GaussianProcessRegressor,
};
use std::collections::HashMap;
use tracing::debug;

/// Configuration for light curve feature-based filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightCurveFilterConfig {
    /// Enable GP-based light curve filtering
    pub enable: bool,

    /// Minimum rise rate (mag/day) to be considered KN-like
    /// Transients rising faster than this get boosted
    pub min_rise_rate: f64,

    /// Minimum decay rate (mag/day) to be considered KN-like
    /// Transients fading slower than this get penalized
    pub min_decay_rate: f64,

    /// Minimum number of detections required for feature extraction
    pub min_detections: usize,

    /// Maximum penalty/boost factor applied to joint FAR
    pub max_penalty_factor: f64,
}

impl Default for LightCurveFilterConfig {
    fn default() -> Self {
        Self {
            enable: true,
            min_rise_rate: 1.0,  // > 1.0 mag/day = fast riser (KN-like)
            min_decay_rate: 0.3, // < 0.3 mag/day = slow fader (SN-like)
            min_detections: 3,
            max_penalty_factor: 10.0,
        }
    }
}

/// Extracted light curve features from GP fitting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightCurveFeatures {
    /// Rise rate in mag/day (negative = brightening in mag space)
    /// Computed from linear regression on early 25% of GP prediction
    pub rise_rate: f64,

    /// Decay rate in mag/day (positive = fading in mag space)
    /// Computed from linear regression on late 25% of GP prediction
    pub decay_rate: f64,

    /// Peak magnitude (brightest point from GP fit)
    pub peak_mag: f64,

    /// Full width at half maximum in days
    pub fwhm: f64,

    /// Current rate of change at last observation (mag/day)
    pub dfdt_now: f64,

    /// Maximum derivative over the light curve (mag/day)
    pub dfdt_max: f64,

    /// Minimum derivative over the light curve (mag/day)
    pub dfdt_min: f64,

    /// Total duration of observations (days)
    pub duration: f64,

    /// Number of detections used
    pub n_detections: usize,

    /// GP fit quality (reduced chi-squared)
    pub gp_quality: f64,

    /// Band used for fitting
    pub band: String,
}

/// Number of GP prediction grid points
const N_PRED: usize = 50;

/// Zero-point for flux-to-magnitude conversion (ZTF μJy)
const ZP_UJY: f64 = 23.9;

/// Extract light curve features using GP regression
///
/// Converts flux to magnitudes, fits a GP to the best-sampled band,
/// and extracts rise/decay rates and other physical features.
///
/// Returns `None` if the light curve has fewer than `min_detections`
/// points in any single band.
pub fn extract_features(lc: &LightCurve) -> Option<LightCurveFeatures> {
    extract_features_with_config(lc, 3)
}

/// Extract features with a configurable minimum detection count
pub fn extract_features_with_config(
    lc: &LightCurve,
    min_detections: usize,
) -> Option<LightCurveFeatures> {
    // Filter to detections only (skip upper limits)
    let detections: Vec<_> = lc
        .measurements
        .iter()
        .filter(|m| !m.is_upper_limit && m.flux > 0.0)
        .collect();

    if detections.len() < min_detections {
        debug!(
            "Insufficient detections for GP features: {} < {}",
            detections.len(),
            min_detections
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

    if band_data.len() < min_detections {
        debug!(
            "Best band {} has only {} detections, need {}",
            best_band,
            band_data.len(),
            min_detections
        );
        return None;
    }

    // Sort by time
    let mut sorted_data = band_data;
    sorted_data.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Convert to relative time (days from first detection) and magnitudes
    let t_min = sorted_data[0].0;
    let t_max = sorted_data.last().unwrap().0;
    let duration = t_max - t_min;

    if duration < 1e-6 {
        debug!("Light curve duration too short: {:.6} days", duration);
        return None;
    }

    let times: Vec<f64> = sorted_data.iter().map(|(t, _, _)| t - t_min).collect();
    let mags: Vec<f64> = sorted_data
        .iter()
        .map(|(_, flux, _)| ZP_UJY - 2.5 * flux.log10())
        .collect();
    let mag_errors: Vec<f64> = sorted_data
        .iter()
        .map(|(_, flux, flux_err)| 1.0857 * flux_err / flux)
        .collect();

    // Fit GP and get predictions
    let (pred_times, pred_mags, pred_std, gp_quality) =
        fit_gp_and_predict(&times, &mags, &mag_errors, duration)?;

    // Extract features from GP predictions
    let features = compute_features_from_gp(
        &pred_times,
        &pred_mags,
        &pred_std,
        &times,
        &mags,
        &mag_errors,
        duration,
        detections.len(),
        gp_quality,
        best_band.to_string(),
    );

    Some(features)
}

/// Fit a GP to light curve data and predict on a uniform grid
///
/// Port of the grid search pattern from fit_nonparametric_lightcurves_sklears.rs
#[allow(clippy::type_complexity)]
fn fit_gp_and_predict(
    times: &[f64],
    mags: &[f64],
    errors: &[f64],
    duration: f64,
) -> Option<(Vec<f64>, Vec<f64>, Vec<f64>, f64)> {
    let times_arr = Array1::from_vec(times.to_vec());
    let mags_arr = Array1::from_vec(mags.to_vec());
    let xt = times_arr.view().insert_axis(Axis(1)).to_owned();

    // Prediction grid
    let pred_times: Vec<f64> = (0..N_PRED)
        .map(|i| i as f64 * duration / (N_PRED - 1) as f64)
        .collect();
    let pred_times_arr = Array1::from_vec(pred_times.clone());
    let pred_2d = pred_times_arr.view().insert_axis(Axis(1)).to_owned();

    // Average measurement error variance
    let avg_error_var = if !errors.is_empty() {
        errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64
    } else {
        1e-4
    };

    // Compute median dt to set minimum lengthscale
    let mut dt_vec: Vec<f64> = Vec::new();
    for i in 1..times.len() {
        dt_vec.push(times[i] - times[i - 1]);
    }
    dt_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_lengthscale = if !dt_vec.is_empty() {
        let median_dt = dt_vec[dt_vec.len() / 2];
        // Cap at duration/2 so at least some candidates pass for short light curves
        (median_dt * 2.0).max(0.1).min(duration / 2.0)
    } else {
        0.1
    };

    // Grid search over hyperparameters
    // Include smaller factors (1.0, 2.0) for short-duration KN light curves
    let amp_candidates = [0.05, 0.1, 0.2, 0.4];
    let ls_factors = [1.0, 2.0, 4.0, 8.0, 16.0];
    let alpha_candidates = [avg_error_var.max(1e-6), avg_error_var.max(1e-4)];

    let mut best_score = f64::INFINITY;
    let mut best_pred: Option<Vec<f64>> = None;
    let mut best_std: Option<Vec<f64>> = None;
    let mut best_chi2 = f64::NAN;

    for &amp in &amp_candidates {
        for &factor in &ls_factors {
            let lengthscale = (duration / factor).max(0.1);
            if lengthscale < min_lengthscale {
                continue;
            }

            for &alpha in &alpha_candidates {
                let cst: Box<dyn sklears_gaussian_process::Kernel> =
                    Box::new(ConstantKernel::new(amp));
                let rbf: Box<dyn sklears_gaussian_process::Kernel> =
                    Box::new(RBF::new(lengthscale));
                let prod = Box::new(ProductKernel::new(vec![cst, rbf]));
                let white = Box::new(WhiteKernel::new(1e-10));
                let kernel = SumKernel::new(vec![prod, white]);

                let gp = GaussianProcessRegressor::new()
                    .kernel(Box::new(kernel))
                    .alpha(alpha)
                    .normalize_y(true);

                let trained = match gp.fit(&xt, &mags_arr) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Evaluate fit quality at observed points
                let pred_at_obs = match trained.predict(&xt) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let mut residuals_sq = 0.0f64;
                for i in 0..mags.len() {
                    let r = mags[i] - pred_at_obs[i];
                    residuals_sq += r * r;
                }
                let rms = (residuals_sq / mags.len() as f64).sqrt();

                // Compute mean predictive std at observed points
                let mean_pred_std = if let Ok((std_arr, _)) = trained.predict_with_std(&xt) {
                    let v = std_arr.to_vec();
                    let sum: f64 = v.iter().filter(|s| s.is_finite()).sum();
                    let cnt = v.iter().filter(|s| s.is_finite()).count();
                    if cnt > 0 {
                        sum / cnt as f64
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                // Reject candidates with extreme extrapolated peaks
                if let Ok(pred_grid) = trained.predict(&pred_2d) {
                    let pred_grid_min = pred_grid.iter().cloned().fold(f64::INFINITY, f64::min);
                    let obs_min = mags.iter().cloned().fold(f64::INFINITY, f64::min);
                    if pred_grid_min.is_finite() && (pred_grid_min - obs_min).abs() > 6.0 {
                        continue;
                    }
                }

                let score = rms + 0.6 * mean_pred_std;

                if score.is_finite() && score < best_score {
                    best_score = score;

                    // Get predictions on the grid
                    if let Ok(pred) = trained.predict(&pred_2d) {
                        let chi2 = residuals_sq / mags.len().max(1) as f64;
                        best_chi2 = chi2;
                        best_pred = Some(pred.to_vec());

                        if let Ok((std_arr, _)) = trained.predict_with_std(&pred_2d) {
                            best_std = Some(std_arr.to_vec());
                        } else {
                            best_std = Some(vec![0.0; N_PRED]);
                        }
                    }
                }
            }
        }
    }

    let pred = best_pred?;
    let std = best_std.unwrap_or_else(|| vec![0.0; N_PRED]);

    Some((pred_times, pred, std, best_chi2))
}

/// Compute features from GP prediction grid
///
/// Ports feature extraction from fit_nonparametric_lightcurves_sklears.rs
/// and rate computation from lightcurve_common.rs
#[allow(clippy::too_many_arguments)]
fn compute_features_from_gp(
    pred_times: &[f64],
    pred_mags: &[f64],
    _pred_std: &[f64],
    obs_times: &[f64],
    _obs_mags: &[f64],
    _obs_errors: &[f64],
    duration: f64,
    n_detections: usize,
    gp_quality: f64,
    band: String,
) -> LightCurveFeatures {
    // Find peak (minimum magnitude = brightest)
    let peak_idx = pred_mags
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let peak_mag = pred_mags[peak_idx];

    // Rise rate: linear regression on early 25% of GP prediction
    let rise_rate = compute_rate_from_predictions(pred_times, pred_mags, true);

    // Decay rate: linear regression on late 25% of GP prediction
    let decay_rate = compute_rate_from_predictions(pred_times, pred_mags, false);

    // FWHM
    let fwhm = compute_fwhm(pred_times, pred_mags, peak_idx);

    // Derivative features on prediction grid
    let dt_grid = if pred_times.len() > 1 {
        pred_times[1] - pred_times[0]
    } else {
        1.0
    };

    let mut dfdt_max = f64::NEG_INFINITY;
    let mut dfdt_min = f64::INFINITY;
    for i in 0..pred_mags.len().saturating_sub(1) {
        let d = (pred_mags[i + 1] - pred_mags[i]) / dt_grid;
        dfdt_max = dfdt_max.max(d);
        dfdt_min = dfdt_min.min(d);
    }

    // Current derivative (at last observation time)
    let t_last = *obs_times.last().unwrap_or(&0.0);
    let dfdt_now = interpolated_derivative(pred_times, pred_mags, t_last);

    LightCurveFeatures {
        rise_rate,
        decay_rate,
        peak_mag,
        fwhm,
        dfdt_now,
        dfdt_max,
        dfdt_min,
        duration,
        n_detections,
        gp_quality,
        band,
    }
}

/// Compute rise or decay rate from GP predictions using linear regression
///
/// Ported from lightcurve_common.rs compute_rise_rate / compute_decay_rate
fn compute_rate_from_predictions(times: &[f64], mags: &[f64], early: bool) -> f64 {
    if times.len() < 2 {
        return f64::NAN;
    }

    let n_points = ((times.len() as f64) * 0.25).ceil() as usize;
    let n_points = n_points.max(2).min(times.len());

    let (slice_times, slice_mags) = if early {
        (&times[..n_points], &mags[..n_points])
    } else {
        let start = times.len() - n_points;
        (&times[start..], &mags[start..])
    };

    linear_regression_slope(slice_times, slice_mags)
}

/// Simple linear regression to get slope (mag/day)
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

/// Compute FWHM from GP predictions
///
/// Ported from lightcurve_common.rs compute_fwhm
fn compute_fwhm(times: &[f64], mags: &[f64], peak_idx: usize) -> f64 {
    if peak_idx >= mags.len() {
        return f64::NAN;
    }

    let peak_mag = mags[peak_idx];
    // Half-max in magnitude space: 0.75 mag fainter than peak
    let half_max_mag = peak_mag + 0.75;

    // Find crossing before peak
    let mut t_before = f64::NAN;
    for i in (0..peak_idx).rev() {
        if mags[i] >= half_max_mag {
            t_before = times[i];
            break;
        }
    }

    // Find crossing after peak
    let mut t_after = f64::NAN;
    for i in (peak_idx + 1)..mags.len() {
        if mags[i] >= half_max_mag {
            t_after = times[i];
            break;
        }
    }

    if t_before.is_nan() || t_after.is_nan() {
        return f64::NAN;
    }

    t_after - t_before
}

/// Interpolated derivative at a specific time
fn interpolated_derivative(times: &[f64], values: &[f64], t: f64) -> f64 {
    if values.len() < 2 || times.len() < 2 {
        return f64::NAN;
    }

    let dt = 1.0; // 1-day step for derivative
    let f_before = interp_at(times, values, t - dt);
    let f_after = interp_at(times, values, t + dt);

    if f_before.is_finite() && f_after.is_finite() {
        (f_after - f_before) / (2.0 * dt)
    } else {
        f64::NAN
    }
}

/// Linear interpolation at a specific time
fn interp_at(times: &[f64], values: &[f64], t: f64) -> f64 {
    if values.is_empty() || times.is_empty() {
        return f64::NAN;
    }
    if t <= times[0] {
        return values[0];
    }
    if t >= *times.last().unwrap() {
        return *values.last().unwrap();
    }

    let mut i = 0usize;
    while i + 1 < times.len() && times[i + 1] < t {
        i += 1;
    }

    let t0 = times[i];
    let t1 = times[i + 1];
    let y0 = values[i];
    let y1 = values[i + 1];
    let w = (t - t0) / (t1 - t0);
    y0 * (1.0 - w) + y1 * w
}

/// Compute a background rejection score for a light curve
///
/// Returns a multiplier for joint FAR:
/// - score > 1.0: candidate is more background-like (penalize by increasing FAR)
/// - score < 1.0: candidate is more KN-like (boost by decreasing FAR)
/// - score = 1.0: neutral (no adjustment)
///
/// The score is applied as: `joint_far *= background_rejection_score(...)`
pub fn background_rejection_score(
    features: &LightCurveFeatures,
    config: &LightCurveFilterConfig,
) -> f64 {
    let mut score: f64 = 1.0;

    // Rise rate check: KN rises > 1 mag/day (negative slope in magnitude = brightening)
    // In magnitude space, rising = negative slope, so we use abs()
    let rise_speed = features.rise_rate.abs();
    if rise_speed > config.min_rise_rate {
        // Fast riser: looks KN-like → boost (lower score)
        score *= 0.5;
    } else if rise_speed < 0.1 {
        // Very slow riser: looks like SN background → penalize
        score *= 3.0;
    }

    // Decay rate check: KN fades > 0.3 mag/day (positive slope in magnitude = fading)
    let decay_speed = features.decay_rate.abs();
    if decay_speed < config.min_decay_rate {
        // Slow fader: likely SN background → penalize
        score *= 3.0;
    } else if decay_speed > 1.0 {
        // Fast fader: looks like KN → boost
        score *= 0.5;
    }

    // Clamp to configured bounds
    score.clamp(1.0 / config.max_penalty_factor, config.max_penalty_factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightcurve::Photometry;

    /// Create a synthetic kilonova-like light curve (fast rise, fast decay)
    fn make_kilonova_lc() -> LightCurve {
        let mut lc = LightCurve::new("KN_test".to_string());
        let mjd_start = 60000.0;

        // KN-like: rises ~2 mag in 0.5 days, decays ~3 mag in 3 days
        let times_and_flux = [
            (0.0, 50.0),   // faint detection
            (0.2, 200.0),  // rapidly brightening
            (0.5, 1000.0), // near peak
            (0.7, 800.0),  // starting to fade
            (1.0, 400.0),  // fading
            (1.5, 150.0),  // fading fast
            (2.0, 60.0),   // faint
            (3.0, 20.0),   // very faint
        ];

        for (dt, flux) in &times_and_flux {
            lc.add_measurement(Photometry::new(
                mjd_start + dt,
                *flux,
                flux * 0.05, // 5% flux error
                "r".to_string(),
            ));
        }
        lc
    }

    /// Create a synthetic SN Ia-like light curve (slow rise, slow decay)
    fn make_sn_ia_lc() -> LightCurve {
        let mut lc = LightCurve::new("SNIa_test".to_string());
        let mjd_start = 60000.0;

        // SN Ia: rises ~2 mag over 15 days, decays slowly
        let times_and_flux = [
            (0.0, 100.0),
            (3.0, 200.0),
            (6.0, 350.0),
            (9.0, 500.0),
            (12.0, 650.0),
            (15.0, 700.0), // near peak
            (18.0, 680.0),
            (21.0, 650.0),
            (25.0, 600.0),
            (30.0, 520.0), // slow decay
        ];

        for (dt, flux) in &times_and_flux {
            lc.add_measurement(Photometry::new(
                mjd_start + dt,
                *flux,
                flux * 0.03, // 3% flux error
                "r".to_string(),
            ));
        }
        lc
    }

    #[test]
    fn test_kilonova_features() {
        let lc = make_kilonova_lc();
        let features = extract_features(&lc);

        if let Some(feat) = features {
            println!(
                "KN features: rise_rate={:.3}, decay_rate={:.3}, peak_mag={:.2}, fwhm={:.2}",
                feat.rise_rate, feat.decay_rate, feat.peak_mag, feat.fwhm
            );

            // KN should have fast rise (large magnitude change per day)
            assert!(
                feat.rise_rate.abs() > 0.5,
                "KN rise rate should be fast, got {:.3}",
                feat.rise_rate
            );

            // KN should have fast decay
            assert!(
                feat.decay_rate.abs() > 0.3,
                "KN decay rate should be fast, got {:.3}",
                feat.decay_rate
            );

            // Background rejection score should boost this candidate
            let config = LightCurveFilterConfig::default();
            let score = background_rejection_score(&feat, &config);
            println!("KN background rejection score: {:.3}", score);
            assert!(
                score <= 1.0,
                "KN should be boosted (score <= 1.0), got {:.3}",
                score
            );
        } else {
            panic!("Failed to extract features from KN light curve");
        }
    }

    #[test]
    fn test_sn_ia_features() {
        let lc = make_sn_ia_lc();
        let features = extract_features(&lc);

        if let Some(feat) = features {
            println!(
                "SN Ia features: rise_rate={:.3}, decay_rate={:.3}, peak_mag={:.2}, fwhm={:.2}",
                feat.rise_rate, feat.decay_rate, feat.peak_mag, feat.fwhm
            );

            // SN Ia should have slow rise
            assert!(
                feat.rise_rate.abs() < 0.5,
                "SN Ia rise rate should be slow, got {:.3}",
                feat.rise_rate
            );

            // Background rejection score should penalize this candidate
            let config = LightCurveFilterConfig::default();
            let score = background_rejection_score(&feat, &config);
            println!("SN Ia background rejection score: {:.3}", score);
            assert!(
                score > 1.0,
                "SN Ia should be penalized (score > 1.0), got {:.3}",
                score
            );
        } else {
            panic!("Failed to extract features from SN Ia light curve");
        }
    }

    #[test]
    fn test_insufficient_data() {
        let mut lc = LightCurve::new("sparse_test".to_string());
        // Only 2 detections — should return None
        lc.add_measurement(Photometry::new(60000.0, 100.0, 5.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60001.0, 200.0, 10.0, "r".to_string()));

        let features = extract_features(&lc);
        assert!(
            features.is_none(),
            "Should return None for insufficient data"
        );
    }

    #[test]
    fn test_sparse_but_sufficient_data() {
        let mut lc = LightCurve::new("sparse5_test".to_string());
        // 5 detections — sparse but sufficient for GP fitting
        lc.add_measurement(Photometry::new(60000.0, 100.0, 5.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60000.5, 200.0, 10.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60001.0, 500.0, 25.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60002.0, 350.0, 18.0, "r".to_string()));
        lc.add_measurement(Photometry::new(60003.0, 200.0, 10.0, "r".to_string()));

        let features = extract_features(&lc);
        assert!(
            features.is_some(),
            "Should extract features from 5 detections"
        );
    }

    #[test]
    fn test_linear_regression_slope() {
        // Simple test: y = 2x + 1
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 3.0, 5.0, 7.0, 9.0];
        let slope = linear_regression_slope(&x, &y);
        assert!(
            (slope - 2.0).abs() < 1e-10,
            "Slope should be 2.0, got {}",
            slope
        );
    }

    #[test]
    fn test_background_rejection_score_bounds() {
        let config = LightCurveFilterConfig::default();

        // Even extreme values should be clamped
        let fast_features = LightCurveFeatures {
            rise_rate: -5.0, // Very fast rise
            decay_rate: 3.0, // Very fast decay
            peak_mag: 18.0,
            fwhm: 1.0,
            dfdt_now: -2.0,
            dfdt_max: 1.0,
            dfdt_min: -3.0,
            duration: 5.0,
            n_detections: 10,
            gp_quality: 0.5,
            band: "r".to_string(),
        };

        let score = background_rejection_score(&fast_features, &config);
        assert!(score >= 1.0 / config.max_penalty_factor);
        assert!(score <= config.max_penalty_factor);
    }
}
