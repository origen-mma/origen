use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::particleswarm::ParticleSwarm;
/// Test variance in t0 recovery across multiple runs
///
/// Run with: cargo test --test test_t0_variance -- --nocapture --ignored
use mm_core::{
    pso_fitter::{pso_bounds, BandFitData},
    svi_fitter::svi_fit,
    svi_models,
};
use rand::Rng;

#[derive(Clone)]
struct PsoCost {
    times: Vec<f64>,
    flux: Vec<f64>,
    flux_err: Vec<f64>,
    model: svi_models::SviModel,
}

impl CostFunction for PsoCost {
    type Param = Vec<f64>;
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, ArgminError> {
        let se_idx = self.model.sigma_extra_idx();
        let sigma_extra = p[se_idx].exp();
        let sigma_extra_sq = sigma_extra * sigma_extra;
        let preds = svi_models::eval_model_batch(self.model, p, &self.times);
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

fn generate_synthetic_kilonova() -> (Vec<f64>, Vec<f64>, Vec<f64>, f64) {
    let true_log10_mej = -2.0;
    let true_log10_vej = -1.0;
    let true_log10_kappa_r = 0.5;
    let true_t0 = 0.0;
    let true_params = vec![
        true_log10_mej,
        true_log10_vej,
        true_log10_kappa_r,
        true_t0,
        -3.0,
    ];

    let obs_times = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];

    let clean_fluxes =
        svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &true_params, &obs_times);

    let scale_factor = 200.0;
    let mut rng = rand::thread_rng();
    let mut noisy_fluxes = Vec::new();
    let mut flux_errors = Vec::new();

    for &flux in &clean_fluxes {
        let scaled = flux * scale_factor;
        let snr = 20.0;
        let err = scaled / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        noisy_fluxes.push((scaled + noise).max(0.1));
        flux_errors.push(err);
    }

    (obs_times, noisy_fluxes, flux_errors, true_t0)
}

fn test_single_run(
    pso_iters: u64,
    svi_iters: usize,
    mc_samples: usize,
    lr: f64,
    times: &[f64],
    flux: &[f64],
    flux_err: &[f64],
    true_t0: f64,
) -> f64 {
    // PSO initialization
    let (lower, upper) = pso_bounds(svi_models::SviModel::MetzgerKN);
    let problem = PsoCost {
        times: times.to_vec(),
        flux: flux.to_vec(),
        flux_err: flux_err.to_vec(),
        model: svi_models::SviModel::MetzgerKN,
    };

    let solver = ParticleSwarm::new((lower, upper), 40);
    let pso_result = Executor::new(problem, solver)
        .configure(|state| state.max_iters(pso_iters))
        .run();

    let pso_params = pso_result
        .ok()
        .and_then(|res| res.state().get_best_param().map(|p| p.position.clone()))
        .unwrap_or_else(|| vec![0.0; 5]);

    // SVI optimization
    let n = times.len();
    let data = BandFitData {
        times: times.to_vec(),
        flux: flux.to_vec(),
        flux_err: flux_err.to_vec(),
        peak_flux_obs: flux.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        is_upper: vec![false; n],  // All detections, no upper limits
        upper_flux: vec![0.0; n],  // Unused for detections
    };

    let svi_result = svi_fit(
        svi_models::SviModel::MetzgerKN,
        &data,
        svi_iters,
        mc_samples,
        lr,
        Some(&pso_params),
        true,           // enable_safeguards
        (0.1, 10.0),    // scale_clamp_range
    );

    let fitted_t0 = svi_result.mu[3];
    (fitted_t0 - true_t0).abs()
}

#[test]
#[ignore]
fn test_baseline_variance() {
    println!("\n=== Baseline Settings Variance (50 PSO, 1000 SVI, 4 MC) ===\n");

    let mut errors = Vec::new();
    for trial in 1..=10 {
        let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();
        let error = test_single_run(50, 1000, 4, 0.01, &times, &flux, &flux_err, true_t0);
        println!(
            "Trial {:2}: t0_error = {:.3} days ({:.1} hrs)",
            trial,
            error,
            error * 24.0
        );
        errors.push(error);
    }

    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let variance = errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / errors.len() as f64;
    let std = variance.sqrt();

    println!(
        "\nMean error: {:.3} ± {:.3} days ({:.1} ± {:.1} hrs)",
        mean,
        std,
        mean * 24.0,
        std * 24.0
    );
}

#[test]
#[ignore]
fn test_improved_variance() {
    println!("\n=== Improved Settings Variance (200 PSO, 5000 SVI, 16 MC) ===\n");

    let mut errors = Vec::new();
    for trial in 1..=10 {
        let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();
        let error = test_single_run(200, 5000, 16, 0.01, &times, &flux, &flux_err, true_t0);
        println!(
            "Trial {:2}: t0_error = {:.3} days ({:.1} hrs)",
            trial,
            error,
            error * 24.0
        );
        errors.push(error);
    }

    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let variance = errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / errors.len() as f64;
    let std = variance.sqrt();

    println!(
        "\nMean error: {:.3} ± {:.3} days ({:.1} ± {:.1} hrs)",
        mean,
        std,
        mean * 24.0,
        std * 24.0
    );
}
