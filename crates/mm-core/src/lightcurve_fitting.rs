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

    // Check if retry needed
    if config.enable_retry && result.elbo < config.catastrophic_threshold {
        info!(
            "Catastrophic failure detected (ELBO = {:.2}), retrying with original settings...",
            result.elbo
        );

        // Retry with original (more aggressive) settings
        let retry_config = FitConfig::original();
        let retry_result = fit_lightcurve_with_config(lightcurve, model, &retry_config)?;

        // Return better of the two
        if retry_result.elbo > result.elbo {
            info!(
                "Retry improved ELBO from {:.2} to {:.2}",
                result.elbo, retry_result.elbo
            );
            Ok(retry_result)
        } else {
            info!(
                "Retry did not improve (ELBO {:.2} vs {:.2}), keeping original",
                retry_result.elbo, result.elbo
            );
            Ok(result)
        }
    } else {
        Ok(result)
    }
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
    // Validate input
    if lightcurve.measurements.len() < 5 {
        return Err(CoreError::InsufficientData(format!(
            "Need at least 5 measurements, got {}",
            lightcurve.measurements.len()
        )));
    }

    // Convert FitModel to SviModel
    let svi_model = match model {
        FitModel::Bazin => SviModel::Bazin,
        FitModel::Villar => SviModel::Villar,
        FitModel::PowerLaw => SviModel::PowerLaw,
        FitModel::MetzgerKN => SviModel::MetzgerKN,
    };

    // Prepare data for fitting with preprocessing

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

        // Keep if within 50 days of nearest neighbor
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
        let time_key = (m.mjd * 100.0).round() as i64; // Group within 0.01 day
        time_groups.entry(time_key).or_default().push((
            m.flux,
            m.flux_err,
            m.is_upper_limit,
        ));
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

        // Check if this time bin contains any upper limits
        let any_upper = group.iter().any(|(_, _, u)| *u);

        // Weighted average of flux
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

    // Convert MJD to days relative to first detection
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

    // Check for invalid values
    if flux_norm.iter().any(|f| !f.is_finite()) {
        return Err(CoreError::InsufficientData(
            "Normalized flux contains NaN or Inf values".to_string(),
        ));
    }

    debug!(
        "Fitting {} with {} model: {} measurements (peak_flux={:.2}, {} upper limits)",
        lightcurve.object_id,
        svi_model.name(),
        times.len(),
        peak_flux,
        is_upper.iter().filter(|&&u| u).count()
    );

    // Create band fit data with proper upper limit handling
    let upper_flux_norm: Vec<f64> = flux_norm
        .iter()
        .zip(is_upper.iter())
        .map(|(&f, &is_up)| {
            if is_up {
                f // For upper limits, flux is the limiting value
            } else {
                f // For detections, this field is unused but set to flux
            }
        })
        .collect();

    let data = BandFitData {
        times: times.clone(),
        flux: flux_norm.clone(),
        flux_err: flux_err_norm.clone(),
        peak_flux_obs: peak_flux,
        is_upper: is_upper.clone(),
        upper_flux: upper_flux_norm,
    };

    // Step 1: PSO initialization for the requested model
    // Use PSO to find good starting parameters, but respect user's model choice
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
        enable_safeguards: bool,
        scale_clamp_range: (f64, f64),
    }

    impl CostFunction for PsoCost {
        type Param = Vec<f64>;
        type Output = f64;

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

                // Simple handling for now - treat all as detections
                // TODO: Implement upper limit likelihood when is_upper[i] == true
                let diff = pred - self.flux[i];
                neg_ll += diff * diff / total_var + total_var.ln();
            }
            Ok(neg_ll / n)
        }
    }

    let (lower, upper) = pso_bounds(svi_model);
    let problem = PsoCost {
        times: data.times.clone(),
        flux: data.flux.clone(),
        flux_err: data.flux_err.clone(),
        model: svi_model,
        is_upper: data.is_upper.clone(),
        upper_flux: data.upper_flux.clone(),
        enable_safeguards: config.enable_safeguards,
        scale_clamp_range: config.scale_clamp_range,
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
                debug!("PSO found no valid particles, using heuristic initialization");
                crate::pso_fitter::init_variational_means(svi_model, &data)
            }
        }
        Err(e) => {
            debug!("PSO failed: {}, using heuristic initialization", e);
            crate::pso_fitter::init_variational_means(svi_model, &data)
        }
    };

    // Step 2: SVI refinement with PSO initialization
    info!(
        "Running SVI refinement (LR={}, iters={}, safeguards={})...",
        config.svi_learning_rate, config.svi_iterations, config.enable_safeguards
    );

    let svi_result = svi_fit(
        svi_model,
        &data,
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

    Ok(LightCurveFitResult {
        t0: t0_mjd,
        t0_err,
        model, // Use the originally requested model
        elbo: svi_result.elbo,
        parameters: svi_result.mu,
        parameter_errors,
        converged,
    })
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
