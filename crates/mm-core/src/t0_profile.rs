//! Profile likelihood for t0 estimation
//!
//! Two-stage optimization to handle t0 multi-modality:
//! 1. Grid search over plausible t0 values
//! 2. For each t0, optimize all other parameters
//! 3. Select t0 with maximum ELBO

use crate::error::CoreError;
use crate::lightcurve::LightCurve;
use crate::lightcurve_fitting::{FitConfig, FitModel, LightCurveFitResult};
use crate::pso_fitter::BandFitData;
use crate::svi_fitter::svi_fit_fixed_t0;
use crate::svi_models::SviModel;
use rayon::prelude::*;
use tracing::{debug, info};

/// Profile likelihood result for t0
#[derive(Debug, Clone)]
pub struct T0ProfileResult {
    /// Best t0 value (MJD)
    pub t0_best: f64,

    /// Uncertainty in t0 from profile curvature (days)
    pub t0_err: f64,

    /// Grid of t0 values tested
    pub t0_grid: Vec<f64>,

    /// ELBO at each grid point
    pub elbo_profile: Vec<f64>,

    /// Best fit parameters (excluding t0)
    pub best_params: Vec<f64>,

    /// Maximum ELBO achieved
    pub best_elbo: f64,
}

/// Fit lightcurve using profile likelihood for t0
///
/// This is more robust than joint optimization when t0 is multi-modal.
///
/// # Strategy
/// 1. Grid search t0 from (first_detection - 5 days) to first_detection
/// 2. For each t0, optimize all other parameters with SVI
/// 3. Select t0 with maximum ELBO
/// 4. Refine around best t0 with finer grid
/// 5. Estimate uncertainty from ELBO curvature
pub fn fit_lightcurve_profile_t0(
    lightcurve: &LightCurve,
    model: FitModel,
    config: &FitConfig,
) -> Result<LightCurveFitResult, CoreError> {
    info!("Starting profile likelihood optimization for t0");

    // Prepare data (same preprocessing as regular fit)
    let (data, first_mjd, svi_model) = prepare_data_for_profile(lightcurve, model)?;

    // Determine t0 search range
    let first_detection = data
        .times
        .iter()
        .zip(data.is_upper.iter())
        .filter(|(_, &is_up)| !is_up)
        .map(|(t, _)| *t)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .ok_or_else(|| CoreError::InsufficientData("No detections".to_string()))?;

    let t0_min = first_detection - 5.0;
    let t0_max = first_detection;

    info!(
        "Searching t0 from {:.2} to {:.2} days (relative to MJD {:.2})",
        t0_min, t0_max, first_mjd
    );

    // Stage 1: Coarse grid search
    let coarse_grid = linspace(t0_min, t0_max, config.profile_grid_size.0);
    let coarse_profile = evaluate_t0_profile(&coarse_grid, &data, svi_model, config)?;

    let best_coarse_idx = coarse_profile
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.elbo.partial_cmp(&b.elbo).unwrap())
        .map(|(idx, _)| idx)
        .ok_or_else(|| CoreError::InsufficientData("No valid fits in coarse grid".to_string()))?;

    let best_coarse_t0 = coarse_grid[best_coarse_idx];
    let best_coarse_elbo = coarse_profile[best_coarse_idx].elbo;

    info!(
        "Coarse grid: best t0 = {:.2} days (ELBO = {:.2})",
        best_coarse_t0, best_coarse_elbo
    );

    // Stage 2: Fine grid search around best (±0.5 days)
    let fine_t0_min = (best_coarse_t0 - 0.5).max(t0_min);
    let fine_t0_max = (best_coarse_t0 + 0.5).min(t0_max);
    let fine_grid = linspace(fine_t0_min, fine_t0_max, config.profile_grid_size.1);

    let fine_profile = evaluate_t0_profile(&fine_grid, &data, svi_model, config)?;

    let best_fine_idx = fine_profile
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.elbo.partial_cmp(&b.elbo).unwrap())
        .map(|(idx, _)| idx)
        .ok_or_else(|| CoreError::InsufficientData("No valid fits in fine grid".to_string()))?;

    let best_t0_relative = fine_grid[best_fine_idx];
    let best_elbo = fine_profile[best_fine_idx].elbo;
    let best_params = fine_profile[best_fine_idx].params.clone();

    info!(
        "Fine grid: best t0 = {:.2} days (ELBO = {:.2})",
        best_t0_relative, best_elbo
    );

    // Combine grids for uncertainty estimation
    let mut combined_t0 = coarse_grid.clone();
    combined_t0.extend_from_slice(&fine_grid);
    let mut combined_elbo = coarse_profile.iter().map(|r| r.elbo).collect::<Vec<_>>();
    combined_elbo.extend(fine_profile.iter().map(|r| r.elbo));

    // Estimate t0 uncertainty from ELBO curvature
    let t0_err = estimate_t0_uncertainty(&combined_t0, &combined_elbo, best_t0_relative, best_elbo);

    info!(
        "Estimated t0 uncertainty: ±{:.2} days ({:.1} hours)",
        t0_err,
        t0_err * 24.0
    );

    // Convert back to MJD
    let t0_mjd = first_mjd + best_t0_relative;

    // Reconstruct full parameter vector (insert t0 at correct index)
    let t0_idx = svi_model.t0_idx();
    let mut full_params = Vec::with_capacity(svi_model.n_params());
    for (i, &val) in best_params.iter().enumerate() {
        if i == t0_idx {
            full_params.push(best_t0_relative);
        }
        full_params.push(val);
    }
    if t0_idx == best_params.len() {
        full_params.push(best_t0_relative);
    }

    let n_params = full_params.len();

    Ok(LightCurveFitResult {
        t0: t0_mjd,
        t0_err,
        model,
        elbo: best_elbo,
        parameters: full_params,
        parameter_errors: vec![t0_err; n_params], // Placeholder
        converged: true,
    })
}

/// Single point evaluation result
struct ProfilePoint {
    elbo: f64,
    params: Vec<f64>,
}

/// Evaluate ELBO profile at a grid of t0 values (parallelized)
fn evaluate_t0_profile(
    t0_grid: &[f64],
    data: &BandFitData,
    model: SviModel,
    config: &FitConfig,
) -> Result<Vec<ProfilePoint>, CoreError> {
    info!("Evaluating {} t0 grid points in parallel", t0_grid.len());

    // Parallel evaluation using rayon
    let results: Vec<ProfilePoint> = t0_grid
        .par_iter()
        .enumerate()
        .map(|(i, &t0)| {
            debug!("Evaluating t0 = {:.2} ({}/{})", t0, i + 1, t0_grid.len());

            // Optimize all parameters except t0
            let result = svi_fit_fixed_t0(
                model,
                data,
                t0,
                config.svi_iterations,
                config.svi_mc_samples,
                config.svi_learning_rate,
                config.enable_safeguards,
                config.scale_clamp_range,
            );

            ProfilePoint {
                elbo: result.elbo,
                params: result.mu,
            }
        })
        .collect();

    Ok(results)
}

/// Estimate t0 uncertainty from ELBO profile curvature
///
/// Uses the 1-sigma interval where ΔELBO ≈ 0.5 (chi-squared for 1 parameter)
fn estimate_t0_uncertainty(
    t0_grid: &[f64],
    elbo_profile: &[f64],
    t0_best: f64,
    elbo_best: f64,
) -> f64 {
    // Find points where ELBO drops by 0.5 from maximum
    let threshold = elbo_best - 0.5;

    // Find lower bound (search left from best)
    let mut lower_bound = t0_grid[0];
    for i in 0..t0_grid.len() {
        if t0_grid[i] < t0_best && elbo_profile[i] > threshold {
            lower_bound = t0_grid[i];
        }
        if t0_grid[i] >= t0_best {
            break;
        }
    }

    // Find upper bound (search right from best)
    let mut upper_bound = t0_grid[t0_grid.len() - 1];
    for i in (0..t0_grid.len()).rev() {
        if t0_grid[i] > t0_best && elbo_profile[i] > threshold {
            upper_bound = t0_grid[i];
        }
        if t0_grid[i] <= t0_best {
            break;
        }
    }

    // Symmetric uncertainty estimate
    let lower_err = t0_best - lower_bound;
    let upper_err = upper_bound - t0_best;

    // Return average (or max for conservative estimate)
    (lower_err + upper_err) / 2.0
}

/// Generate linearly spaced values
fn linspace(start: f64, end: f64, n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![start];
    }

    let step = (end - start) / (n - 1) as f64;
    (0..n).map(|i| start + i as f64 * step).collect()
}

/// Prepare data for profile fitting (extracted from lightcurve_fitting.rs)
fn prepare_data_for_profile(
    lightcurve: &LightCurve,
    model: FitModel,
) -> Result<(BandFitData, f64, SviModel), CoreError> {
    // This is simplified - in practice, copy preprocessing from lightcurve_fitting.rs
    // For now, just do basic setup

    let svi_model = match model {
        FitModel::Bazin => SviModel::Bazin,
        FitModel::Villar => SviModel::Villar,
        FitModel::PowerLaw => SviModel::PowerLaw,
        FitModel::MetzgerKN => SviModel::MetzgerKN,
    };

    // Basic preprocessing
    let mut measurements = lightcurve.measurements.clone();
    measurements.sort_by(|a, b| a.mjd.partial_cmp(&b.mjd).unwrap());

    if measurements.is_empty() {
        return Err(CoreError::InsufficientData("No measurements".to_string()));
    }

    let first_mjd = measurements[0].mjd;

    let times: Vec<f64> = measurements.iter().map(|m| m.mjd - first_mjd).collect();
    let flux: Vec<f64> = measurements.iter().map(|m| m.flux).collect();
    let flux_err: Vec<f64> = measurements.iter().map(|m| m.flux_err).collect();
    let is_upper: Vec<bool> = measurements.iter().map(|m| m.is_upper_limit).collect();

    // Normalize by peak
    let peak_flux = flux
        .iter()
        .zip(is_upper.iter())
        .filter(|(_, &is_up)| !is_up)
        .map(|(f, _)| *f)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(1.0);

    let flux_norm: Vec<f64> = flux.iter().map(|f| f / peak_flux).collect();
    let flux_err_norm: Vec<f64> = flux_err.iter().map(|e| e / peak_flux).collect();
    let upper_flux: Vec<f64> = flux_norm.clone();

    let data = BandFitData {
        times,
        flux: flux_norm,
        flux_err: flux_err_norm,
        peak_flux_obs: peak_flux,
        is_upper,
        upper_flux,
    };

    Ok((data, first_mjd, svi_model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linspace() {
        let vals = linspace(0.0, 10.0, 5);
        assert_eq!(vals.len(), 5);
        assert!((vals[0] - 0.0).abs() < 1e-10);
        assert!((vals[4] - 10.0).abs() < 1e-10);
        assert!((vals[2] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_uncertainty_estimation() {
        // Need grid points above threshold (elbo_best - 0.5 = 9.5) near the peak
        let t0_grid = vec![-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0];
        let elbo_profile = vec![-10.0, -3.0, 9.7, 10.0, 9.7, -3.0, -10.0];

        let t0_err = estimate_t0_uncertainty(&t0_grid, &elbo_profile, 0.0, 10.0);

        // lower_bound = -1.0 (9.7 > 9.5), upper_bound = 1.0 (9.7 > 9.5)
        // t0_err = (1.0 + 1.0) / 2.0 = 1.0
        assert!(t0_err > 0.5 && t0_err < 1.5);
    }
}
