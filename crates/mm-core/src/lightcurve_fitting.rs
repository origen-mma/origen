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
use crate::svi_fitter::{svi_fit, LightCurveData};
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

    // Prepare data for SVI fitting
    // Convert MJD to days relative to first detection
    let first_mjd = lightcurve
        .measurements
        .iter()
        .map(|m| m.mjd)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();

    let times: Vec<f64> = lightcurve
        .measurements
        .iter()
        .map(|m| m.mjd - first_mjd)
        .collect();

    let flux: Vec<f64> = lightcurve.measurements.iter().map(|m| m.flux).collect();

    let flux_err: Vec<f64> = lightcurve.measurements.iter().map(|m| m.flux_err).collect();

    debug!(
        "Fitting {} with {} model: {} measurements",
        lightcurve.object_id,
        svi_model.name(),
        times.len()
    );

    // Create light curve data
    let data = LightCurveData::from_measurements(times, flux, flux_err);

    // Run SVI fitting
    // Use moderate iterations for real-time performance
    let n_iter = 200;
    let n_mc_samples = 4;
    let learning_rate = 0.01;

    let svi_result = svi_fit(svi_model, &data, n_iter, n_mc_samples, learning_rate, None);

    // Extract t0 from parameters
    let t0_idx = match svi_model {
        SviModel::Bazin => 2,
        SviModel::Villar => 3,
        SviModel::PowerLaw => 3,
        SviModel::MetzgerKN => 3,
    };

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

    let parameter_errors = svi_result.get_uncertainties();

    Ok(LightCurveFitResult {
        t0: t0_mjd,
        t0_err,
        model,
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
