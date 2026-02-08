//! Multi-start optimization for robust parameter estimation
//!
//! Runs multiple independent PSO+SVI fits with different random seeds
//! and selects the best result based on ELBO (Evidence Lower Bound).
//! This helps avoid getting stuck in poor local minima.

use crate::lightcurve_fitting::{LightCurveFitResult, FitModel};
use crate::lightcurve::LightCurve;
use crate::pso_fitter::{BandFitData, pso_bounds};
use crate::svi_fitter::svi_fit;
use crate::svi_models::SviModel;
use crate::error::CoreError;
use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::particleswarm::ParticleSwarm;
use rand::SeedableRng;
use tracing::{debug, info, warn};

/// Multi-start optimization configuration
#[derive(Debug, Clone)]
pub struct MultiStartConfig {
    /// Number of independent optimization runs
    pub n_starts: usize,

    /// PSO iterations per start
    pub pso_iters: u64,

    /// SVI iterations per start
    pub svi_iters: usize,

    /// SVI Monte Carlo samples
    pub svi_mc_samples: usize,

    /// SVI learning rate
    pub svi_lr: f64,

    /// Minimum acceptable ELBO (skip early if achieved)
    pub early_stop_elbo: Option<f64>,
}

impl Default for MultiStartConfig {
    fn default() -> Self {
        Self {
            n_starts: 3,              // Try 3 times by default
            pso_iters: 200,
            svi_iters: 5000,
            svi_mc_samples: 16,
            svi_lr: 0.01,
            early_stop_elbo: Some(50.0),  // Stop early if we get excellent fit
        }
    }
}

impl MultiStartConfig {
    /// Fast configuration (fewer iterations, fewer starts)
    pub fn fast() -> Self {
        Self {
            n_starts: 2,
            pso_iters: 100,
            svi_iters: 2000,
            svi_mc_samples: 8,
            svi_lr: 0.01,
            early_stop_elbo: Some(30.0),
        }
    }

    /// Conservative configuration (more starts, more iterations)
    pub fn conservative() -> Self {
        Self {
            n_starts: 5,
            pso_iters: 300,
            svi_iters: 8000,
            svi_mc_samples: 32,
            svi_lr: 0.005,  // Lower LR for stability
            early_stop_elbo: None,  // Always try all starts
        }
    }
}

/// Result from a single optimization start
#[derive(Debug, Clone)]
pub struct StartResult {
    pub start_id: usize,
    pub elbo: f64,
    pub converged: bool,
    pub parameters: Vec<f64>,
    pub t0: f64,
    pub t0_err: f64,
}

/// Multi-start optimization result
pub struct MultiStartResult {
    /// Best result (highest ELBO)
    pub best: LightCurveFitResult,

    /// All attempted starts
    pub all_starts: Vec<StartResult>,

    /// Number of acceptable starts (ELBO > 0)
    pub n_acceptable: usize,

    /// Did we find at least one good fit?
    pub has_good_fit: bool,
}

/// Run multi-start optimization for a light curve
pub fn multistart_fit(
    lightcurve: &LightCurve,
    model: FitModel,
    config: MultiStartConfig,
) -> Result<MultiStartResult, CoreError> {
    info!(
        "Starting multi-start optimization: {} starts for {}",
        config.n_starts,
        lightcurve.object_id
    );

    // Convert to internal model type
    let svi_model = match model {
        FitModel::Bazin => SviModel::Bazin,
        FitModel::Villar => SviModel::Villar,
        FitModel::PowerLaw => SviModel::PowerLaw,
        FitModel::MetzgerKN => SviModel::MetzgerKN,
    };

    // Prepare data (same preprocessing as regular fit)
    let data = prepare_fit_data(lightcurve)?;

    let mut all_starts = Vec::new();
    let mut best_elbo = f64::NEG_INFINITY;
    let mut best_result: Option<LightCurveFitResult> = None;

    for start_id in 0..config.n_starts {
        debug!("Multi-start {}/{}", start_id + 1, config.n_starts);

        // Run PSO with unique random seed
        let pso_seed = (start_id as u64) * 12345 + 67890;
        let pso_params = run_pso_with_seed(
            &data,
            svi_model,
            config.pso_iters,
            pso_seed,
        )?;

        // Run SVI from PSO initialization
        let svi_result = svi_fit(
            svi_model,
            &data,
            config.svi_iters,
            config.svi_mc_samples,
            config.svi_lr,
            Some(&pso_params),
        );

        let elbo = svi_result.elbo;
        let start_result = StartResult {
            start_id,
            elbo,
            converged: true,  // TODO: track convergence properly
            parameters: svi_result.mu.clone(),
            t0: data.mjd_offset + svi_result.mu[svi_model.t0_idx()],
            t0_err: svi_result.log_sigma[svi_model.t0_idx()].exp(),
        };

        info!(
            "  Start {}: ELBO = {:.2}, t0 = {:.3}",
            start_id + 1, elbo, start_result.t0
        );

        all_starts.push(start_result);

        // Track best result
        if elbo > best_elbo {
            best_elbo = elbo;
            best_result = Some(convert_to_fit_result(
                svi_result,
                &data,
                lightcurve,
                model,
            ));
        }

        // Early stop if we achieved excellent fit
        if let Some(threshold) = config.early_stop_elbo {
            if elbo > threshold {
                info!(
                    "Early stop: achieved ELBO {:.2} > threshold {:.2}",
                    elbo, threshold
                );
                break;
            }
        }
    }

    let n_acceptable = all_starts.iter().filter(|s| s.elbo > 0.0).count();
    let has_good_fit = best_elbo > 10.0;

    if !has_good_fit {
        warn!(
            "Multi-start optimization found no good fits for {} (best ELBO: {:.2})",
            lightcurve.object_id, best_elbo
        );
    }

    Ok(MultiStartResult {
        best: best_result.ok_or_else(|| {
            CoreError::InsufficientData("Multi-start failed to produce any result".to_string())
        })?,
        all_starts,
        n_acceptable,
        has_good_fit,
    })
}

// Helper functions (simplified versions of what's in lightcurve_fitting.rs)

fn prepare_fit_data(lightcurve: &LightCurve) -> Result<PreparedData, CoreError> {
    // TODO: Extract this from lightcurve_fitting.rs to avoid duplication
    // For now, return a placeholder
    unimplemented!("Need to extract preprocessing logic from lightcurve_fitting.rs")
}

fn run_pso_with_seed(
    data: &PreparedData,
    model: SviModel,
    iters: u64,
    seed: u64,
) -> Result<Vec<f64>, CoreError> {
    // TODO: Implement PSO with explicit seed
    unimplemented!("Need to add seed support to PSO")
}

fn convert_to_fit_result(
    svi_result: crate::svi_fitter::SviFitResult,
    data: &PreparedData,
    lightcurve: &LightCurve,
    model: FitModel,
) -> LightCurveFitResult {
    // TODO: Convert from SviFitResult to LightCurveFitResult
    unimplemented!("Need conversion logic")
}

struct PreparedData {
    mjd_offset: f64,
    // ... other fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multistart_config() {
        let default_config = MultiStartConfig::default();
        assert_eq!(default_config.n_starts, 3);

        let fast_config = MultiStartConfig::fast();
        assert_eq!(fast_config.n_starts, 2);

        let conservative_config = MultiStartConfig::conservative();
        assert_eq!(conservative_config.n_starts, 5);
    }
}
