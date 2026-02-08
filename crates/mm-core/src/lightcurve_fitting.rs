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

/// Fit a light curve to extract t0
///
/// This function performs Stochastic Variational Inference (SVI) to fit
/// the light curve and extract the explosion/merger time parameter.
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

    // Step 3: Average duplicate measurements at same time
    use std::collections::HashMap;
    let mut time_groups: HashMap<i64, Vec<(f64, f64)>> = HashMap::new();
    for m in &cleaned_measurements {
        let time_key = (m.mjd * 100.0).round() as i64; // Group within 0.01 day
        time_groups
            .entry(time_key)
            .or_insert_with(Vec::new)
            .push((m.flux, m.flux_err));
    }

    let mut times = Vec::new();
    let mut flux = Vec::new();
    let mut flux_err = Vec::new();

    let mut time_keys: Vec<_> = time_groups.keys().collect();
    time_keys.sort();

    for &key in time_keys {
        let group = &time_groups[&key];
        let mjd = key as f64 / 100.0;

        // Weighted average of flux
        let mut weight_sum = 0.0;
        let mut flux_sum = 0.0;
        for &(f, e) in group {
            let w = 1.0 / (e * e);
            weight_sum += w;
            flux_sum += w * f;
        }
        let avg_flux = flux_sum / weight_sum;
        let avg_err = (1.0 / weight_sum).sqrt();

        times.push(mjd);
        flux.push(avg_flux);
        flux_err.push(avg_err);
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

    debug!(
        "Fitting {} with {} model: {} measurements",
        lightcurve.object_id,
        svi_model.name(),
        times.len()
    );

    // Create band fit data
    let data = BandFitData {
        times: times.clone(),
        flux: flux_norm,
        flux_err: flux_err_norm,
        peak_flux_obs: peak_flux,
    };

    // Step 1: PSO initialization for the requested model
    // Use PSO to find good starting parameters, but respect user's model choice
    info!("Running PSO initialization for {} model...", svi_model.name());

    use crate::pso_fitter::pso_bounds;
    use argmin::core::{CostFunction, Executor, State};
    use argmin::solver::particleswarm::ParticleSwarm;

    #[derive(Clone)]
    struct PsoCost {
        times: Vec<f64>,
        flux: Vec<f64>,
        flux_err: Vec<f64>,
        model: SviModel,
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
                let diff = pred - self.flux[i];
                let total_var = self.flux_err[i] * self.flux_err[i] + sigma_extra_sq + 1e-10;
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
    };

    let solver = ParticleSwarm::new((lower, upper), 40);
    let pso_result = Executor::new(problem, solver)
        .configure(|state| state.max_iters(50))
        .run();

    let pso_params = match pso_result {
        Ok(res) => res.state().get_best_param().unwrap().position.clone(),
        Err(e) => {
            debug!("PSO failed: {}, using heuristic initialization", e);
            crate::pso_fitter::init_variational_means(svi_model, &data)
        }
    };

    // Step 2: SVI refinement with PSO initialization
    info!("Running SVI refinement...");
    let n_iter = 1000; // CRITICAL: Use 1000 iterations for proper convergence
    let n_mc_samples = 4;
    let learning_rate = 0.01;

    let svi_result = svi_fit(
        svi_model,
        &data,
        n_iter,
        n_mc_samples,
        learning_rate,
        Some(&pso_params),
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
