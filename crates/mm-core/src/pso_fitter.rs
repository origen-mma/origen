//! Particle Swarm Optimization for light curve model initialization
//!
//! Uses PSO to find good starting parameters for SVI optimization.
//! Implements model selection by trying multiple models in cascade.

use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::particleswarm::ParticleSwarm;

use crate::svi_models::{eval_model_batch, SviModel};

/// Data structure for fitting a single band
#[derive(Clone)]
pub struct BandFitData {
    pub times: Vec<f64>,
    pub flux: Vec<f64>,
    pub flux_err: Vec<f64>,
    pub peak_flux_obs: f64,
}

/// PSO cost function (negative log-likelihood)
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

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, ArgminError> {
        let se_idx = self.model.sigma_extra_idx();
        let sigma_extra = p[se_idx].exp();
        let sigma_extra_sq = sigma_extra * sigma_extra;
        let preds = eval_model_batch(self.model, p, &self.times);
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

/// PSO model selection: try models in priority order, stop when good enough
///
/// Order (cheapest/broadest first): Bazin → PowerLaw → Villar → MetzgerKN
///
/// Returns (best_model, best_params, best_cost)
pub fn pso_model_select(data: &BandFitData) -> (SviModel, Vec<f64>, f64) {
    const EARLY_STOP: f64 = -3.0; // Good enough fit

    let models: &[SviModel] = &[
        SviModel::Bazin,
        SviModel::PowerLaw,
        SviModel::Villar,
        SviModel::MetzgerKN,
    ];

    let mut best_model = SviModel::Bazin;
    let mut best_params = vec![];
    let mut best_chi2 = f64::INFINITY;

    for &model in models {
        let (lower, upper) = pso_bounds(model);
        let problem = PsoCost {
            times: data.times.clone(),
            flux: data.flux.clone(),
            flux_err: data.flux_err.clone(),
            model,
        };

        let solver = ParticleSwarm::new((lower, upper), 40);
        let res = Executor::new(problem, solver)
            .configure(|state| state.max_iters(200))  // Increased from 50 for better t0 accuracy
            .run();

        match res {
            Ok(res) => {
                let chi2 = res.state().get_cost();
                if chi2 < best_chi2 {
                    best_chi2 = chi2;
                    best_model = model;
                    best_params = res.state().get_best_param().unwrap().position.clone();
                }
            }
            Err(e) => {
                eprintln!("  PSO error for {}: {}", model.name(), e);
            }
        }

        // Early stop: if current best is good enough, don't try more models
        if best_chi2 < EARLY_STOP {
            break;
        }
    }

    (best_model, best_params, best_chi2)
}

/// PSO search bounds for each model
pub fn pso_bounds(model: SviModel) -> (Vec<f64>, Vec<f64>) {
    match model {
        SviModel::Bazin => {
            // log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            let lower = vec![-3.0, -1.0, -100.0, -2.0, -2.0, -5.0];
            let upper = vec![3.0, 1.0, 100.0, 5.0, 6.0, 0.0];
            (lower, upper)
        }
        SviModel::Villar => {
            // log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            let lower = vec![-3.0, -0.05, -3.0, -100.0, -2.0, -2.0, -5.0];
            let upper = vec![3.0, 0.05, 5.0, 100.0, 5.0, 6.0, 0.0];
            (lower, upper)
        }
        SviModel::PowerLaw => {
            // log_a, log_alpha, log_beta, t0, log_sigma_extra
            let lower = vec![-3.0, -1.0, -1.0, -100.0, -5.0];
            let upper = vec![3.0, 2.0, 2.0, 100.0, 0.0];
            (lower, upper)
        }
        SviModel::MetzgerKN => {
            // log10_mej, log10_vej, log10_kappa_r, t0, log_sigma_extra
            let lower = vec![-4.0, -1.5, 0.0, -100.0, -5.0];
            let upper = vec![-1.0, -0.5, 2.0, 100.0, 0.0];
            (lower, upper)
        }
    }
}

/// Initialize variational means from data characteristics
pub fn init_variational_means(model: SviModel, data: &BandFitData) -> Vec<f64> {
    // Find peak flux and time
    let (t_peak, peak_val) = data
        .times
        .iter()
        .zip(data.flux.iter())
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(t, f)| (*t, *f))
        .unwrap_or((0.0, 1.0));

    match model {
        SviModel::Bazin => {
            // log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            let log_a = peak_val.max(0.01).ln();
            let b = 0.0;
            let t0 = t_peak - 5.0; // Estimate explosion ~5 days before peak
            let log_tau_rise = 2.0_f64.ln(); // ~2 day rise
            let log_tau_fall = 20.0_f64.ln(); // ~20 day fall
            let log_sigma_extra = -3.0;
            vec![log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra]
        }
        SviModel::Villar => {
            // log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            let log_a = peak_val.max(0.01).ln();
            let beta = 0.0;
            let log_gamma = 10.0_f64.ln();
            let t0 = t_peak - 5.0;
            let log_tau_rise = 2.0_f64.ln();
            let log_tau_fall = 30.0_f64.ln();
            let log_sigma_extra = -3.0;
            vec![log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra]
        }
        SviModel::PowerLaw => {
            // log_a, log_alpha, log_beta, t0, log_sigma_extra
            let log_a = peak_val.max(0.01).ln();
            let log_alpha = 1.0_f64.ln(); // alpha ~ 1
            let log_beta = 1.5_f64.ln(); // beta ~ 1.5
            let t0 = t_peak - 3.0;
            let log_sigma_extra = -3.0;
            vec![log_a, log_alpha, log_beta, t0, log_sigma_extra]
        }
        SviModel::MetzgerKN => {
            // log10_mej, log10_vej, log10_kappa_r, t0, log_sigma_extra
            let log10_mej = -2.0; // 0.01 Msun
            let log10_vej = -1.0; // 0.1c
            let log10_kappa_r = 0.5; // kappa ~ 3
            let t0 = t_peak - 2.0; // Merger ~2 days before peak
            let log_sigma_extra = -3.0;
            vec![log10_mej, log10_vej, log10_kappa_r, t0, log_sigma_extra]
        }
    }
}
