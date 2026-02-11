//! Light curve fitting for extracting t0 (explosion/merger time)
//!
//! This module provides SVI-based light curve fitting to estimate the physical
//! explosion or merger time (t0) from optical transient light curves. This is
//! more accurate than using first detection time for multi-messenger correlation.
//!
//! # Models
//!
//! - **Bazin**: Empirical supernova model (Bazin+ 2009)
//! - **Villar**: Improved empirical model (Villar+ 2019)
//! - **PowerLaw**: Simple power-law rise + decay
//! - **MetzgerKN**: Physical kilonova model (Metzger+ 2017)
//!
//! # Example
//!
//! ```rust,ignore
//! use mm_core::lightcurve_fitting::{fit_lightcurve, LightCurveFitResult};
//! use mm_core::optical::LightCurve;
//!
//! let lightcurve = LightCurve { /* ... */ };
//! let result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN)?;
//!
//! println!("Estimated merger time: {} MJD", result.t0);
//! println!("Uncertainty: ±{} days", result.t0_err);
//! ```

use crate::error::CoreError;
use crate::lightcurve::LightCurve;
use crate::pso_fitter::BandFitData;
use crate::svi_fitter::svi_fit;
use crate::svi_models::SviModel;
use tracing::{debug, info};

/// Light curve fitting model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitModel {
    /// Bazin supernova model
    Bazin,
    /// Villar supernova model
    Villar,
    /// Power-law model
    PowerLaw,
    /// Metzger kilonova model (physical)
    MetzgerKN,
}

/// Configuration for light curve fitting optimization
#[derive(Debug, Clone)]
pub struct FitConfig {
    /// SVI learning rate (default: 0.005, original: 0.01)
    pub svi_learning_rate: f64,

    /// SVI iterations (default: 5000)
    pub svi_iterations: usize,

    /// SVI Monte Carlo samples for gradient estimation (default: 16)
    pub svi_mc_samples: usize,

    /// PSO iterations (default: 200)
    pub pso_iterations: u64,

    /// Enable numerical safeguards for MetzgerKN normalization (default: true)
    pub enable_safeguards: bool,

    /// Scale factor clamping range for safeguards (default: 0.1 to 10.0)
    pub scale_clamp_range: (f64, f64),

    /// Enable automatic retry on catastrophic failure (default: true)
    pub enable_retry: bool,

    /// ELBO threshold for catastrophic failure detection (default: -1000.0)
    pub catastrophic_threshold: f64,

    /// Grid size for profile likelihood: (coarse_points, fine_points)
    /// Default: (10, 5) for 15 total evaluations
    pub profile_grid_size: (usize, usize),
}

impl Default for FitConfig {
    fn default() -> Self {
        Self::conservative()
    }
}

impl FitConfig {
    /// Conservative configuration (current settings with safeguards)
    pub fn conservative() -> Self {
        Self {
            svi_learning_rate: 0.005,
            svi_iterations: 5000,
            svi_mc_samples: 16,
            pso_iterations: 200,
            enable_safeguards: true,
            scale_clamp_range: (0.1, 10.0),
            enable_retry: true,
            catastrophic_threshold: -1000.0,
            profile_grid_size: (10, 5), // 15 total points (balanced)
        }
    }

    /// Original configuration (pre-stability-fix settings, more aggressive)
    pub fn original() -> Self {
        Self {
            svi_learning_rate: 0.01, // Higher learning rate
            svi_iterations: 5000,
            svi_mc_samples: 16,
            pso_iterations: 200,
            enable_safeguards: false,         // No safeguards
            scale_clamp_range: (0.01, 100.0), // Wider range
            enable_retry: false,              // Don't retry from retry
            catastrophic_threshold: -1000.0,
            profile_grid_size: (10, 5), // 15 total points (same as conservative)
        }
    }

    /// Fast configuration for testing
    pub fn fast() -> Self {
        Self {
            svi_learning_rate: 0.01,
            svi_iterations: 1000,
            svi_mc_samples: 8,
            pso_iterations: 100,
            enable_safeguards: false,
            scale_clamp_range: (0.1, 10.0),
            enable_retry: false,
            catastrophic_threshold: -1000.0,
            profile_grid_size: (5, 3), // 8 total points (quick validation)
        }
    }
}

/// Light curve fit result with t0 estimate
#[derive(Debug, Clone)]
pub struct LightCurveFitResult {
    /// Estimated t0 (explosion/merger time) in MJD
    pub t0: f64,

    /// Uncertainty in t0 (1-sigma) in days
    pub t0_err: f64,

    /// Model used for fitting
    pub model: FitModel,

    /// Evidence Lower Bound (ELBO) - quality of fit
    pub elbo: f64,

    /// All fitted parameters (model-dependent)
    pub parameters: Vec<f64>,

    /// Parameter uncertainties (1-sigma)
    pub parameter_errors: Vec<f64>,

    /// Whether the fit converged successfully
    pub converged: bool,
}

impl LightCurveFitResult {
    /// Get t0 in GPS time (seconds since GPS epoch)
    pub fn t0_gps(&self) -> f64 {
        mjd_to_gps(self.t0)
    }

    /// Get t0 uncertainty in seconds
    pub fn t0_err_seconds(&self) -> f64 {
        self.t0_err * 86400.0
    }

    /// Check if t0 estimate is reliable based on fit quality
    pub fn is_reliable(&self) -> bool {
        self.converged && self.t0_err < 1.0 // Less than 1 day uncertainty
    }
}

/// Find the best t0 estimate for the Bazin model via grid search.
///
/// The Bazin model has strong degeneracies between t0, tau_rise, and tau_fall,
/// especially when only the declining part of the light curve is observed.
/// This grid search evaluates the model at many (t0, tau_rise, tau_fall) combinations
/// with analytically-solved amplitude (a) and baseline (b), providing a cheap but
/// effective initialization for PSO.
///
/// Returns (best_t0, best_params, t0_lower_bound, t0_upper_bound)
fn bazin_t0_grid_search(
    times: &[f64],
    flux: &[f64],
    flux_err: &[f64],
) -> (f64, Vec<f64>, f64, f64) {
    let n = times.len();
    let t_span = times[n - 1] - times[0];

    // Find observed peak
    let (t_peak_idx, _) = flux
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap_or((0, &1.0));
    let t_peak = times[t_peak_idx];
    let peak_flux = flux[t_peak_idx].max(0.01);

    // Grid range for t0: explosion must be before or near the observed peak
    // Lower: up to half the data span before first observation
    // Upper: shortly after observed peak
    let t0_min = -(t_span * 0.5).min(50.0);
    let t0_max = (t_peak + 10.0).min(t_span + 5.0);

    let n_t0 = 30;
    // Typical SN timescales to search over
    let tau_rises = [1.5, 3.0, 7.0, 15.0];
    let tau_falls = [10.0, 25.0, 50.0];

    let mut best_t0 = t_peak - 5.0;
    let mut best_cost = f64::INFINITY;
    let mut best_params = vec![
        peak_flux.ln(),
        0.0,
        best_t0,
        3.0_f64.ln(),
        25.0_f64.ln(),
        -3.0,
    ];

    // Precompute weights (inverse variance)
    let weights: Vec<f64> = flux_err.iter().map(|e| 1.0 / (e * e + 1e-10)).collect();

    for &tau_rise in &tau_rises {
        for &tau_fall in &tau_falls {
            for i_t0 in 0..n_t0 {
                let t0 = t0_min + (t0_max - t0_min) * i_t0 as f64 / (n_t0 - 1) as f64;

                // Compute basis function g(t) = exp(-(t-t0)/tau_fall) * sigmoid((t-t0)/tau_rise)
                let g: Vec<f64> = times
                    .iter()
                    .map(|&t| {
                        let dt = t - t0;
                        let e_fall = (-dt / tau_fall).exp();
                        let sig = 1.0 / (1.0 + (-dt / tau_rise).exp());
                        e_fall * sig
                    })
                    .collect();

                // Solve for (a, b) analytically via weighted least squares
                // Model: f(t) = a * g(t) + b
                let mut wgg = 0.0;
                let mut wg = 0.0;
                let mut wgf = 0.0;
                let mut wf = 0.0;
                let mut w_sum = 0.0;

                for j in 0..n {
                    let w = weights[j];
                    wgg += w * g[j] * g[j];
                    wg += w * g[j];
                    wgf += w * g[j] * flux[j];
                    wf += w * flux[j];
                    w_sum += w;
                }

                let det = wgg * w_sum - wg * wg;
                if det.abs() < 1e-20 {
                    continue;
                }

                let a = (wgf * w_sum - wf * wg) / det;
                let b = (wgg * wf - wg * wgf) / det;

                // Skip non-physical solutions (amplitude must be positive)
                if a <= 0.0 {
                    continue;
                }

                // Compute weighted chi-squared
                let chi2: f64 = (0..n)
                    .map(|j| {
                        let pred = a * g[j] + b;
                        let diff = pred - flux[j];
                        weights[j] * diff * diff
                    })
                    .sum();

                if chi2 < best_cost {
                    best_cost = chi2;
                    best_t0 = t0;
                    best_params = vec![
                        a.max(1e-10).ln(), // log_a
                        b,
                        t0,
                        tau_rise.ln(), // log_tau_rise
                        tau_fall.ln(), // log_tau_fall
                        -3.0,          // log_sigma_extra
                    ];
                }
            }
        }
    }

    // Set PSO bounds around best t0 with generous margin
    let margin = 20.0;
    let t0_lower = (best_t0 - margin).max(t0_min - 10.0);
    let t0_upper = (best_t0 + margin).min(t0_max + 10.0);

    debug!(
        "Bazin grid search: best_t0={:.2}, chi2={:.2}, bounds=[{:.2}, {:.2}]",
        best_t0, best_cost, t0_lower, t0_upper
    );

    (best_t0, best_params, t0_lower, t0_upper)
}

/// Fit a light curve to extract t0 with automatic retry on failure
///
/// This function performs Stochastic Variational Inference (SVI) to fit
/// the light curve and extract the explosion/merger time parameter.
/// It automatically retries with more aggressive settings if catastrophic failure occurs.
///
/// # Arguments
///
/// * `lightcurve` - The optical light curve to fit
/// * `model` - The model to use for fitting
///
/// # Returns
///
/// The fit result including t0 estimate and uncertainties
///
/// # Errors
///
/// Returns error if:
/// - Light curve has insufficient data points (< 5)
/// - Fit fails to converge
/// - Model evaluation fails
pub fn fit_lightcurve(
    lightcurve: &LightCurve,
    model: FitModel,
) -> Result<LightCurveFitResult, CoreError> {
    let config = FitConfig::default();

    // Try with conservative settings first
    let result = fit_lightcurve_with_config(lightcurve, model, &config)?;

    // Check if retry needed (catastrophic ELBO failure)
    let mut best_result = result;

    if config.enable_retry && best_result.elbo < config.catastrophic_threshold {
        info!(
            "Catastrophic failure detected (ELBO = {:.2}), retrying with original settings...",
            best_result.elbo
        );

        let retry_config = FitConfig::original();
        if let Ok(retry_result) = fit_lightcurve_with_config(lightcurve, model, &retry_config) {
            if retry_result.elbo > best_result.elbo {
                info!(
                    "Retry improved ELBO from {:.2} to {:.2}",
                    best_result.elbo, retry_result.elbo
                );
                best_result = retry_result;
            }
        }
    }

    Ok(best_result)
}

/// Cheap 1D profile-likelihood sweep to refine t0 estimate and uncertainty.
///
/// After SVI converges, this sweeps t0 on a grid while holding other params
/// at their SVI posterior means, analytically re-fitting amplitude at each point.
/// The profile peak gives a bias-corrected t0, and the profile width (where
/// LL drops by 0.5 from peak) gives a proper 1-sigma uncertainty.
///
/// Cost: ~50 model evaluations with analytical amplitude solve = negligible vs SVI.
/// Ported from ZTF lightcurve-fitting pipeline.
fn profile_t0_refine(
    result: &mut LightCurveFitResult,
    data: &BandFitData,
    svi_model: SviModel,
    first_mjd: f64,
) {
    let t0_idx = svi_model.t0_idx();
    let se_idx = svi_model.sigma_extra_idx();

    let old_t0 = result.parameters[t0_idx];
    let old_t0_err = result.parameter_errors[t0_idx];

    // Data-driven sweep range: 30 days before first detection to peak time
    let t_first = data.times.iter().cloned().fold(f64::INFINITY, f64::min);
    let (peak_idx, _) = data
        .flux
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap_or((0, &1.0));
    let t_peak = data.times[peak_idx];
    let t0_lo = t_first - 30.0;
    let t0_hi = t_peak + 5.0;
    if t0_lo >= t0_hi {
        return;
    }

    let n_grid: usize = 50;
    let obs_var: Vec<f64> = data.flux_err.iter().map(|e| e * e + 1e-10).collect();
    let sigma_extra = result.parameters[se_idx].exp();
    let sigma_extra_sq = sigma_extra * sigma_extra;
    let n_obs = data.times.len();

    // Models with a baseline offset b (Bazin has b at index 1)
    let has_baseline = svi_model == SviModel::Bazin;
    let log_a_idx = 0;

    let mut params = result.parameters.clone();
    let mut best_t0 = old_t0;
    let mut best_ll = f64::NEG_INFINITY;
    let mut t0_vals = Vec::with_capacity(n_grid);
    let mut ll_vals = Vec::with_capacity(n_grid);

    for gi in 0..n_grid {
        let t0 = t0_lo + (t0_hi - t0_lo) * gi as f64 / (n_grid - 1).max(1) as f64;

        // Reset to SVI means with this t0
        let n = params.len();
        params.copy_from_slice(&result.parameters[..n]);
        params[t0_idx] = t0;

        // Analytically re-fit amplitude (and baseline for Bazin).
        // All models are linear in a = exp(log_a): flux = a * shape(t) [+ b]
        let saved_log_a = params[log_a_idx];
        params[log_a_idx] = 0.0; // a = 1 to get shape function
        let saved_b = if has_baseline {
            let v = params[1];
            params[1] = 0.0;
            v
        } else {
            0.0
        };

        let shapes = crate::svi_models::eval_model_batch(svi_model, &params, &data.times);

        if has_baseline {
            // Weighted linear regression: flux = a * shape + b
            let mut sw = 0.0;
            let mut sy = 0.0;
            let mut sf = 0.0;
            let mut syf = 0.0;
            let mut sff = 0.0;
            for i in 0..n_obs {
                if data.is_upper[i] || !shapes[i].is_finite() {
                    continue;
                }
                let w = 1.0 / (obs_var[i] + sigma_extra_sq);
                sw += w;
                sy += w * data.flux[i];
                sf += w * shapes[i];
                syf += w * data.flux[i] * shapes[i];
                sff += w * shapes[i] * shapes[i];
            }
            let det = sw * sff - sf * sf;
            if det.abs() > 1e-20 {
                let a_opt = (sw * syf - sf * sy) / det;
                let b_opt = (sff * sy - sf * syf) / det;
                if a_opt > 1e-10 {
                    params[log_a_idx] = a_opt.ln();
                    params[1] = b_opt;
                } else {
                    params[log_a_idx] = saved_log_a;
                    params[1] = saved_b;
                }
            } else {
                params[log_a_idx] = saved_log_a;
                params[1] = saved_b;
            }
        } else {
            // Simple weighted least squares: flux = a * shape
            let mut num = 0.0;
            let mut den = 0.0;
            for i in 0..n_obs {
                if data.is_upper[i] || !shapes[i].is_finite() {
                    continue;
                }
                let w = 1.0 / (obs_var[i] + sigma_extra_sq);
                num += w * data.flux[i] * shapes[i];
                den += w * shapes[i] * shapes[i];
            }
            if den > 1e-20 && num / den > 1e-10 {
                params[log_a_idx] = (num / den).ln();
            } else {
                params[log_a_idx] = saved_log_a;
            }
        }

        // Evaluate log-likelihood with re-fitted amplitude
        let mut preds = crate::svi_models::eval_model_batch(svi_model, &params, &data.times);

        // MetzgerKN renormalization
        if svi_model == SviModel::MetzgerKN {
            let max_pred = preds
                .iter()
                .zip(data.is_upper.iter())
                .filter(|(_, &is_up)| !is_up)
                .map(|(p, _)| *p)
                .fold(f64::NEG_INFINITY, f64::max);
            if max_pred > 1e-10 && max_pred.is_finite() {
                let scale = (1.0 / max_pred).clamp(0.1, 10.0);
                for pred in preds.iter_mut() {
                    *pred *= scale;
                }
            }
        }

        let mut ll = 0.0;
        for i in 0..n_obs {
            let pred = preds[i];
            if !pred.is_finite() {
                continue;
            }
            let total_var = obs_var[i] + sigma_extra_sq;
            if data.is_upper[i] {
                let z = (data.upper_flux[i] - pred) / total_var.sqrt();
                // Approximate log Φ(z)
                ll += if z > 8.0 {
                    0.0
                } else if z < -30.0 {
                    -0.5 * z * z
                } else {
                    let z_neg = -z * std::f64::consts::FRAC_1_SQRT_2;
                    let t = 1.0 / (1.0 + 0.3275911 * z_neg.abs());
                    let poly = t
                        * (0.254829592
                            + t * (-0.284496736
                                + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
                    let erfc_z = poly * (-z_neg * z_neg).exp();
                    let phi = if z_neg >= 0.0 {
                        0.5 * erfc_z
                    } else {
                        1.0 - 0.5 * erfc_z
                    };
                    phi.max(1e-300).ln()
                };
            } else {
                let residual = data.flux[i] - pred;
                ll += -0.5
                    * (residual * residual / total_var
                        + (2.0 * std::f64::consts::PI * total_var).ln());
            }
        }

        t0_vals.push(t0);
        ll_vals.push(ll);

        if ll > best_ll {
            best_ll = ll;
            best_t0 = t0;
        }
    }

    // Estimate profile width via parabolic interpolation around the peak.
    // This is more robust than width-at-half-max on coarse grids.
    // σ = 1/sqrt(-d²LL/dt0²) from the fitted parabola curvature.
    let peak_idx = t0_vals
        .iter()
        .zip(ll_vals.iter())
        .enumerate()
        .max_by(|(_, (_, a)), (_, (_, b))| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);

    let grid_step = (t0_hi - t0_lo) / (n_grid - 1).max(1) as f64;

    let profile_sigma = if peak_idx > 0 && peak_idx < n_grid - 1 {
        // Parabolic fit: LL(t) ≈ a*(t-t_peak)^2 + c at the three points
        let ll_m = ll_vals[peak_idx - 1];
        let ll_0 = ll_vals[peak_idx];
        let ll_p = ll_vals[peak_idx + 1];
        let curvature = (ll_m + ll_p - 2.0 * ll_0) / (grid_step * grid_step);
        if curvature < -1e-10 {
            // σ = 1/sqrt(-curvature) for a profile log-likelihood
            1.0 / (-curvature).sqrt()
        } else {
            // Flat or convex: fall back to width-at-half-max
            let threshold = best_ll - 0.5;
            let mut lo = t0_vals[0];
            let mut hi = *t0_vals.last().unwrap();
            for i in 0..n_grid {
                if ll_vals[i] >= threshold {
                    lo = t0_vals[i];
                    break;
                }
            }
            for i in (0..n_grid).rev() {
                if ll_vals[i] >= threshold {
                    hi = t0_vals[i];
                    break;
                }
            }
            ((hi - lo) / 2.0).max(grid_step)
        }
    } else {
        // Peak at edge of grid: use SVI uncertainty as fallback
        old_t0_err
    };

    info!(
        "profile_t0: {:.2} -> {:.2} (sigma: {:.3} -> {:.3})",
        old_t0, best_t0, old_t0_err, profile_sigma
    );

    // Update result
    result.parameters[t0_idx] = best_t0;
    result.parameter_errors[t0_idx] = profile_sigma;
    result.t0 = first_mjd + best_t0;
    result.t0_err = profile_sigma;
}

/// Preprocessed light curve data ready for fitting
struct PreprocessedData {
    /// Band fit data (normalized, cleaned)
    data: BandFitData,
    /// MJD of first measurement (for converting relative t0 back to MJD)
    first_mjd: f64,
    /// SVI model enum
    svi_model: SviModel,
}

/// Preprocess a light curve for fitting: sort, remove outliers, average duplicates, normalize.
///
/// This shared helper ensures consistent preprocessing between `fit_lightcurve_with_config`
/// and `fit_bazin_profile_t0`.
fn preprocess_lightcurve(
    lightcurve: &LightCurve,
    model: FitModel,
) -> Result<PreprocessedData, CoreError> {
    if lightcurve.measurements.len() < 5 {
        return Err(CoreError::InsufficientData(format!(
            "Need at least 5 measurements, got {}",
            lightcurve.measurements.len()
        )));
    }

    let svi_model = match model {
        FitModel::Bazin => SviModel::Bazin,
        FitModel::Villar => SviModel::Villar,
        FitModel::PowerLaw => SviModel::PowerLaw,
        FitModel::MetzgerKN => SviModel::MetzgerKN,
    };

    // Step 1: Sort by time
    let mut measurements = lightcurve.measurements.clone();
    measurements.sort_by(|a, b| a.mjd.partial_cmp(&b.mjd).unwrap());

    // Step 2: Remove isolated outliers (detections >50 days from nearest neighbor)
    let mut cleaned_measurements = Vec::new();
    for i in 0..measurements.len() {
        let mjd = measurements[i].mjd;
        let prev_dist = if i > 0 {
            mjd - measurements[i - 1].mjd
        } else {
            f64::INFINITY
        };
        let next_dist = if i < measurements.len() - 1 {
            measurements[i + 1].mjd - mjd
        } else {
            f64::INFINITY
        };
        let min_dist = prev_dist.min(next_dist);

        if min_dist < 50.0 {
            cleaned_measurements.push(measurements[i].clone());
        } else {
            debug!(
                "Removing isolated detection at MJD {:.3} (distance: {:.1} days)",
                mjd, min_dist
            );
        }
    }

    if cleaned_measurements.len() < 5 {
        return Err(CoreError::InsufficientData(format!(
            "After removing outliers, only {} measurements remain (need 5)",
            cleaned_measurements.len()
        )));
    }

    // Step 3: Average duplicate measurements at same time, preserving upper limit flags
    use std::collections::HashMap;
    let mut time_groups: HashMap<i64, Vec<(f64, f64, bool)>> = HashMap::new();
    for m in &cleaned_measurements {
        let time_key = (m.mjd * 100.0).round() as i64;
        time_groups
            .entry(time_key)
            .or_default()
            .push((m.flux, m.flux_err, m.is_upper_limit));
    }

    let mut times = Vec::new();
    let mut flux = Vec::new();
    let mut flux_err = Vec::new();
    let mut is_upper = Vec::new();

    let mut time_keys: Vec<_> = time_groups.keys().collect();
    time_keys.sort();

    for &key in time_keys {
        let group = &time_groups[&key];
        let mjd = key as f64 / 100.0;
        let any_upper = group.iter().any(|(_, _, u)| *u);

        let mut weight_sum = 0.0;
        let mut flux_sum = 0.0;
        for &(f, e, _) in group {
            let w = 1.0 / (e * e);
            weight_sum += w;
            flux_sum += w * f;
        }
        let avg_flux = flux_sum / weight_sum;
        let avg_err = (1.0 / weight_sum).sqrt();

        times.push(mjd);
        flux.push(avg_flux);
        flux_err.push(avg_err);
        is_upper.push(any_upper);
    }

    debug!(
        "Preprocessed: {} → {} measurements",
        measurements.len(),
        times.len()
    );

    // Convert MJD to days relative to first measurement
    let first_mjd = times[0];
    let times: Vec<f64> = times.iter().map(|t| t - first_mjd).collect();

    // Normalize flux by peak for better numerical stability
    let peak_flux = flux
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(1.0);

    let flux_norm: Vec<f64> = flux.iter().map(|f| f / peak_flux).collect();
    let flux_err_norm: Vec<f64> = flux_err.iter().map(|e| e / peak_flux).collect();

    if flux_norm.iter().any(|f| !f.is_finite()) {
        return Err(CoreError::InsufficientData(
            "Normalized flux contains NaN or Inf values".to_string(),
        ));
    }

    let upper_flux_norm: Vec<f64> = flux_norm.clone();

    let data = BandFitData {
        times,
        flux: flux_norm,
        flux_err: flux_err_norm,
        peak_flux_obs: peak_flux,
        is_upper,
        upper_flux: upper_flux_norm,
    };

    Ok(PreprocessedData {
        data,
        first_mjd,
        svi_model,
    })
}

/// Fit a light curve to extract t0 with custom configuration
///
/// This is the low-level fitting function that accepts a custom FitConfig.
/// For most use cases, use `fit_lightcurve()` which includes automatic retry.
///
/// # Arguments
///
/// * `lightcurve` - The optical light curve to fit
/// * `model` - The model to use for fitting
/// * `config` - Configuration for optimization parameters
///
/// # Returns
///
/// The fit result including t0 estimate and uncertainties
///
/// # Errors
///
/// Returns error if:
/// - Light curve has insufficient data points (< 5)
/// - Fit fails to converge
/// - Model evaluation fails
pub fn fit_lightcurve_with_config(
    lightcurve: &LightCurve,
    model: FitModel,
    config: &FitConfig,
) -> Result<LightCurveFitResult, CoreError> {
    let preprocessed = preprocess_lightcurve(lightcurve, model)?;
    let data = &preprocessed.data;
    let first_mjd = preprocessed.first_mjd;
    let svi_model = preprocessed.svi_model;

    debug!(
        "Fitting {} with {} model: {} measurements (peak_flux={:.2}, {} upper limits)",
        lightcurve.object_id,
        svi_model.name(),
        data.times.len(),
        data.peak_flux_obs,
        data.is_upper.iter().filter(|&&u| u).count()
    );

    // Step 1: PSO initialization for the requested model
    // Use PSO to find good starting parameters, but respect user's model choice

    // For Bazin model, pre-estimate t0 via grid search to constrain PSO bounds.
    // The Bazin model has strong t0/tau_rise/tau_fall degeneracies, especially
    // when only the declining part is observed. Grid search breaks these degeneracies.
    let (pso_t0_override, grid_fallback_params) = if svi_model == SviModel::Bazin {
        let (_best_t0, grid_params, t0_lo, t0_hi) =
            bazin_t0_grid_search(&data.times, &data.flux, &data.flux_err);
        info!(
            "Bazin grid search: t0={:.2}, bounds=[{:.2}, {:.2}]",
            _best_t0, t0_lo, t0_hi
        );
        (Some((t0_lo, t0_hi)), Some(grid_params))
    } else {
        (None, None)
    };

    info!(
        "Running PSO initialization for {} model...",
        svi_model.name()
    );

    use crate::pso_fitter::pso_bounds;
    use argmin::core::{CostFunction, Executor, State};
    use argmin::solver::particleswarm::ParticleSwarm;

    #[derive(Clone)]
    struct PsoCost {
        times: Vec<f64>,
        flux: Vec<f64>,
        flux_err: Vec<f64>,
        model: SviModel,
        is_upper: Vec<bool>,
        upper_flux: Vec<f64>,
        _enable_safeguards: bool,
        _scale_clamp_range: (f64, f64),
    }

    impl CostFunction for PsoCost {
        type Param = Vec<f64>;
        type Output = f64;

        #[allow(clippy::needless_range_loop)]
        fn cost(&self, p: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
            let se_idx = self.model.sigma_extra_idx();
            let sigma_extra = p[se_idx].exp();
            let sigma_extra_sq = sigma_extra * sigma_extra;
            let preds = crate::svi_models::eval_model_batch(self.model, p, &self.times);
            let n = self.times.len().max(1) as f64;
            let mut neg_ll = 0.0;
            for i in 0..self.times.len() {
                let pred = preds[i];
                if !pred.is_finite() {
                    return Ok(1e99);
                }
                let total_var = self.flux_err[i] * self.flux_err[i] + sigma_extra_sq + 1e-10;

                if self.is_upper[i] {
                    // Upper limit: penalize if model predicts flux above the limit
                    // Use smooth approximation: neg_ll += -log Φ((f_upper - f_pred) / σ)
                    let sigma_total = total_var.sqrt();
                    let z = (self.upper_flux[i] - pred) / sigma_total;
                    // Approximate -log Φ(z): for z >> 0 this is ~0 (no penalty),
                    // for z << 0 this is large (model too bright = bad)
                    let neg_log_phi = if z > 8.0 {
                        0.0
                    } else if z < -8.0 {
                        0.5 * z * z // Quadratic penalty for extreme violations
                    } else {
                        // -log Φ(z) ≈ -log(0.5 * erfc(-z/√2))
                        // Use softplus-like approximation for smooth gradients
                        let exp_neg_z = (-z * 1.7).exp(); // 1.7 ≈ √(π/2) for better approximation
                        (1.0 + exp_neg_z).ln() / 1.7
                    };
                    neg_ll += neg_log_phi;
                } else {
                    // Detection: standard Gaussian negative log-likelihood
                    let diff = pred - self.flux[i];
                    neg_ll += diff * diff / total_var + total_var.ln();
                }
            }
            Ok(neg_ll / n)
        }
    }

    // Get PSO bounds, overriding t0 range for Bazin if grid search was performed
    let (mut lower, mut upper) = pso_bounds(svi_model);
    if let Some((t0_lo, t0_hi)) = pso_t0_override {
        let t0_idx = svi_model.t0_idx();
        lower[t0_idx] = t0_lo;
        upper[t0_idx] = t0_hi;
    }

    let problem = PsoCost {
        times: data.times.clone(),
        flux: data.flux.clone(),
        flux_err: data.flux_err.clone(),
        model: svi_model,
        is_upper: data.is_upper.clone(),
        upper_flux: data.upper_flux.clone(),
        _enable_safeguards: config.enable_safeguards,
        _scale_clamp_range: config.scale_clamp_range,
    };

    let solver = ParticleSwarm::new((lower, upper), 40);
    let pso_result = Executor::new(problem, solver)
        .configure(|state| state.max_iters(config.pso_iterations))
        .run();

    let pso_params = match pso_result {
        Ok(res) => {
            if let Some(best) = res.state().get_best_param() {
                best.position.clone()
            } else {
                debug!("PSO found no valid particles, using grid/heuristic initialization");
                grid_fallback_params
                    .clone()
                    .unwrap_or_else(|| crate::pso_fitter::init_variational_means(svi_model, data))
            }
        }
        Err(e) => {
            debug!("PSO failed: {}, using grid/heuristic initialization", e);
            grid_fallback_params
                .clone()
                .unwrap_or_else(|| crate::pso_fitter::init_variational_means(svi_model, data))
        }
    };

    // Step 2: SVI refinement with PSO initialization
    info!(
        "Running SVI refinement (LR={}, iters={}, safeguards={})...",
        config.svi_learning_rate, config.svi_iterations, config.enable_safeguards
    );

    let svi_result = svi_fit(
        svi_model,
        data,
        config.svi_iterations,
        config.svi_mc_samples,
        config.svi_learning_rate,
        Some(&pso_params),
        config.enable_safeguards,
        config.scale_clamp_range,
    );

    // Extract t0 from parameters
    let t0_idx = svi_result.model.t0_idx();
    let t0_relative = svi_result.mu[t0_idx];
    let t0_mjd = first_mjd + t0_relative;
    let t0_err = svi_result.log_sigma[t0_idx].exp();

    info!(
        "Fitted t0 for {}: {:.3} MJD (±{:.3} days), ELBO: {:.2}",
        lightcurve.object_id, t0_mjd, t0_err, svi_result.elbo
    );

    // Check convergence heuristics
    let converged = svi_result.elbo.is_finite()
        && svi_result.elbo > -1e6
        && t0_err < 10.0 // Reasonable uncertainty
        && t0_mjd > 0.0; // Valid MJD

    // Extract parameter uncertainties
    let parameter_errors: Vec<f64> = svi_result.log_sigma.iter().map(|ls| ls.exp()).collect();

    let mut result = LightCurveFitResult {
        t0: t0_mjd,
        t0_err,
        model, // Use the originally requested model
        elbo: svi_result.elbo,
        parameters: svi_result.mu,
        parameter_errors,
        converged,
    };

    // Apply cheap profile-likelihood refinement for t0 on empirical models.
    // Sweeps t0 on a grid with analytical amplitude re-fit (~50 model evals).
    // Corrects SVI t0 bias and provides calibrated profile-width uncertainty.
    // Only for Bazin/Villar/PowerLaw: these are linear in amplitude (flux = a * shape + b),
    // so the analytical WLS solve is valid. Physical models (MetzgerKN) have
    // complex normalization that breaks this assumption.
    if matches!(
        svi_model,
        SviModel::Bazin | SviModel::Villar | SviModel::PowerLaw
    ) {
        profile_t0_refine(&mut result, data, svi_model, first_mjd);
    }

    Ok(result)
}

/// Convert MJD to GPS time (seconds since GPS epoch)
///
/// GPS epoch: 1980-01-06 00:00:00 UTC
/// MJD epoch: 1858-11-17 00:00:00 UTC
///
/// Difference: 44244 days
pub fn mjd_to_gps(mjd: f64) -> f64 {
    const MJD_TO_GPS_OFFSET: f64 = 44244.0 * 86400.0; // Days to seconds
    (mjd * 86400.0) - MJD_TO_GPS_OFFSET
}

/// Convert GPS time to MJD
pub fn gps_to_mjd(gps: f64) -> f64 {
    const MJD_TO_GPS_OFFSET: f64 = 44244.0 * 86400.0;
    (gps + MJD_TO_GPS_OFFSET) / 86400.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mjd_gps_conversion() {
        // Test known conversion
        let mjd = 58849.0; // 2020-01-01 00:00:00
        let gps = mjd_to_gps(mjd);
        let mjd_back = gps_to_mjd(gps);

        assert!((mjd - mjd_back).abs() < 1e-6);
    }

    #[test]
    fn test_fit_result_reliability() {
        let result = LightCurveFitResult {
            t0: 58849.0,
            t0_err: 0.5,
            model: FitModel::MetzgerKN,
            elbo: -100.0,
            parameters: vec![],
            parameter_errors: vec![],
            converged: true,
        };

        assert!(result.is_reliable());

        let unreliable = LightCurveFitResult {
            t0: 58849.0,
            t0_err: 2.0, // Too large
            model: FitModel::MetzgerKN,
            elbo: -100.0,
            parameters: vec![],
            parameter_errors: vec![],
            converged: true,
        };

        assert!(!unreliable.is_reliable());
    }
}
