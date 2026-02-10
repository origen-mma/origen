use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::particleswarm::ParticleSwarm;
/// Test different optimization settings to improve t0 recovery
///
/// Run with: cargo test --test optimize_t0_recovery -- --nocapture --ignored
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

    #[allow(clippy::needless_range_loop)]
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

#[allow(clippy::too_many_arguments)]
fn test_settings(
    pso_iters: u64,
    svi_iters: usize,
    mc_samples: usize,
    lr: f64,
    times: &[f64],
    flux: &[f64],
    flux_err: &[f64],
    true_t0: f64,
) -> (f64, f64) {
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
        is_upper: vec![false; n], // All detections, no upper limits
        upper_flux: vec![0.0; n], // Unused for detections
    };

    let svi_result = svi_fit(
        svi_models::SviModel::MetzgerKN,
        &data,
        svi_iters,
        mc_samples,
        lr,
        Some(&pso_params),
        true,        // enable_safeguards
        (0.1, 10.0), // scale_clamp_range
    );

    let fitted_t0 = svi_result.mu[3];
    let t0_error = (fitted_t0 - true_t0).abs();
    let elbo = svi_result.elbo;

    (t0_error, elbo)
}

#[test]
#[ignore]
fn test_pso_iterations() {
    println!("\n=== Testing PSO Iteration Count ===\n");
    let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();

    let pso_settings = vec![50, 100, 200, 500];
    for &iters in &pso_settings {
        let (error, elbo) = test_settings(iters, 1000, 4, 0.01, &times, &flux, &flux_err, true_t0);
        println!(
            "PSO iters={:4}: t0_error={:.3} days ({:.1} hrs), ELBO={:.1}",
            iters,
            error,
            error * 24.0,
            elbo
        );
    }
}

#[test]
#[ignore]
fn test_svi_iterations() {
    println!("\n=== Testing SVI Iteration Count ===\n");
    let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();

    let svi_settings = vec![1000, 2000, 5000, 10000];
    for &iters in &svi_settings {
        let (error, elbo) = test_settings(50, iters, 4, 0.01, &times, &flux, &flux_err, true_t0);
        println!(
            "SVI iters={:5}: t0_error={:.3} days ({:.1} hrs), ELBO={:.1}",
            iters,
            error,
            error * 24.0,
            elbo
        );
    }
}

#[test]
#[ignore]
fn test_mc_samples() {
    println!("\n=== Testing Monte Carlo Sample Count ===\n");
    let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();

    let mc_settings = vec![2, 4, 8, 16, 32];
    for &samples in &mc_settings {
        let (error, elbo) =
            test_settings(50, 1000, samples, 0.01, &times, &flux, &flux_err, true_t0);
        println!(
            "MC samples={:2}: t0_error={:.3} days ({:.1} hrs), ELBO={:.1}",
            samples,
            error,
            error * 24.0,
            elbo
        );
    }
}

#[test]
#[ignore]
fn test_learning_rate() {
    println!("\n=== Testing Learning Rate ===\n");
    let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();

    let lr_settings = vec![0.005, 0.01, 0.02, 0.05, 0.1];
    for &lr in &lr_settings {
        let (error, elbo) = test_settings(50, 1000, 4, lr, &times, &flux, &flux_err, true_t0);
        println!(
            "Learning rate={:.3}: t0_error={:.3} days ({:.1} hrs), ELBO={:.1}",
            lr,
            error,
            error * 24.0,
            elbo
        );
    }
}

#[test]
#[ignore]
fn test_best_combo() {
    println!("\n=== Testing Best Combination ===\n");
    println!("Running 10 trials with different settings...\n");

    let configs = vec![
        ("Baseline (50,1000,4,0.01)", 50, 1000, 4, 0.01),
        ("More PSO (200,1000,4,0.01)", 200, 1000, 4, 0.01),
        ("More SVI (50,5000,4,0.01)", 50, 5000, 4, 0.01),
        ("More MC (50,1000,16,0.01)", 50, 1000, 16, 0.01),
        ("Higher LR (50,1000,4,0.02)", 50, 1000, 4, 0.02),
        ("Combo 1 (100,2000,8,0.015)", 100, 2000, 8, 0.015),
        ("Combo 2 (200,5000,16,0.01)", 200, 5000, 16, 0.01),
    ];

    for (name, pso_iter, svi_iter, mc_samp, lr) in configs {
        let (times, flux, flux_err, true_t0) = generate_synthetic_kilonova();
        let (error, elbo) = test_settings(
            pso_iter, svi_iter, mc_samp, lr, &times, &flux, &flux_err, true_t0,
        );
        println!(
            "{:30}: t0_error={:.3} days ({:.1} hrs), ELBO={:.1}",
            name,
            error,
            error * 24.0,
            elbo
        );
    }
}
