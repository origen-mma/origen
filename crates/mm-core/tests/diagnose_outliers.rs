use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::particleswarm::ParticleSwarm;
use mm_core::pso_fitter::pso_bounds;
/// Diagnose what causes catastrophic t0 outliers
///
/// Run with: cargo test --test diagnose_outliers -- --nocapture --ignored
use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

#[derive(Clone)]
struct PsoCost {
    times: Vec<f64>,
    flux: Vec<f64>,
    flux_err: Vec<f64>,
    model: svi_models::SviModel,
    is_upper: Vec<bool>,
    upper_flux: Vec<f64>,
}

impl CostFunction for PsoCost {
    type Param = Vec<f64>;
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, ArgminError> {
        let se_idx = self.model.sigma_extra_idx();
        let sigma_extra = p[se_idx].exp();
        let sigma_extra_sq = sigma_extra * sigma_extra;
        let mut preds = svi_models::eval_model_batch(self.model, p, &self.times);

        // Apply renormalization for MetzgerKN
        if self.model == svi_models::SviModel::MetzgerKN {
            let max_pred = preds
                .iter()
                .zip(self.is_upper.iter())
                .filter(|(_, &is_up)| !is_up)
                .map(|(p, _)| *p)
                .fold(f64::NEG_INFINITY, f64::max);

            if max_pred > 1e-10 && max_pred.is_finite() {
                let scale = 1.0 / max_pred;
                for pred in preds.iter_mut() {
                    *pred *= scale;
                }
            }
        }

        let n = self.times.len().max(1) as f64;
        let mut neg_ll = 0.0;
        for i in 0..self.times.len() {
            let pred = preds[i];
            if !pred.is_finite() {
                return Ok(1e99);
            }
            let total_var = self.flux_err[i] * self.flux_err[i] + sigma_extra_sq + 1e-10;

            if self.is_upper[i] {
                let z = (self.upper_flux[i] - pred) / total_var.sqrt();
                neg_ll -= log_normal_cdf(z);
            } else {
                let diff = pred - self.flux[i];
                neg_ll += diff * diff / total_var + total_var.ln();
            }
        }
        Ok(neg_ll / n)
    }
}

fn log_normal_cdf(x: f64) -> f64 {
    if x > 8.0 {
        return 0.0;
    }
    if x < -30.0 {
        return -0.5 * x * x - 0.5 * (2.0 * std::f64::consts::PI).ln() - (-x).ln();
    }
    let z = -x * std::f64::consts::FRAC_1_SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erfc_z = poly * (-z * z).exp();
    let phi = if z >= 0.0 {
        0.5 * erfc_z
    } else {
        1.0 - 0.5 * erfc_z
    };
    (phi.max(1e-300)).ln()
}

fn generate_synthetic(seed: u64) -> (LightCurve, f64) {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let true_t0 = 0.0;
    let true_params = vec![-2.0, -1.0, 0.5, true_t0, -3.0];

    let obs_times_detections = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];
    let obs_times_nondetections = vec![-3.0, -2.0, -1.0, -0.5];

    let mut all_obs_times = obs_times_nondetections.clone();
    all_obs_times.extend_from_slice(&obs_times_detections);

    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &all_obs_times,
    );

    let scale_factor = 200.0;
    let mut all_fluxes = Vec::new();
    let mut all_flux_errors = Vec::new();

    let n_nondet = obs_times_nondetections.len();
    let limiting_flux = 15.0;

    for i in 0..n_nondet {
        let flux_err = 5.0;
        let true_flux = clean_fluxes[i] * scale_factor;
        let noise = rng.gen::<f64>() * flux_err - flux_err * 0.5;
        let measured_flux = (true_flux + noise).max(0.0).min(limiting_flux);
        all_fluxes.push(measured_flux);
        all_flux_errors.push(flux_err);
    }

    for i in n_nondet..all_obs_times.len() {
        let flux = clean_fluxes[i] * scale_factor;
        let snr = 20.0;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        all_fluxes.push((flux + noise).max(0.1));
        all_flux_errors.push(err);
    }

    let mut lightcurve = LightCurve::new(format!("SEED_{}", seed));
    let mjd_offset = 60000.0;

    for i in 0..n_nondet {
        lightcurve.add_measurement(Photometry::new_upper_limit(
            mjd_offset + all_obs_times[i],
            limiting_flux,
            "r".to_string(),
        ));
    }

    for i in n_nondet..all_obs_times.len() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + all_obs_times[i],
            all_fluxes[i],
            all_flux_errors[i],
            "r".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

#[test]
#[ignore]
fn diagnose_bad_cases() {
    println!("\n=== Diagnosing Outlier Cases ===\n");
    println!("Running 20 trials to find failure modes...\n");

    let mut good_cases = Vec::new();
    let mut bad_cases = Vec::new();

    for seed in 1..=20 {
        let (lightcurve, true_t0) = generate_synthetic(seed);
        let fit_result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN).unwrap();

        let mjd_offset = 60000.0;
        let true_t0_mjd = mjd_offset + true_t0;
        let fitted_t0_mjd = fit_result.t0;
        let t0_error = fitted_t0_mjd - true_t0_mjd;

        let is_bad = t0_error.abs() > 5.0 || fit_result.elbo < -10.0;

        if is_bad {
            bad_cases.push((seed, t0_error, fit_result.elbo, lightcurve));
            println!(
                "❌ Seed {:2}: t0_error={:+6.2} days, ELBO={:7.2} [BAD]",
                seed, t0_error, fit_result.elbo
            );
        } else {
            good_cases.push((seed, t0_error, fit_result.elbo));
            println!(
                "✅ Seed {:2}: t0_error={:+6.2} days, ELBO={:7.2}",
                seed, t0_error, fit_result.elbo
            );
        }
    }

    println!("\n=== Summary ===");
    println!("Good cases: {} / 20", good_cases.len());
    println!("Bad cases:  {} / 20", bad_cases.len());

    if !bad_cases.is_empty() {
        println!("\n=== Analyzing Bad Cases ===\n");

        for (seed, t0_error, elbo, lightcurve) in bad_cases.iter().take(3) {
            println!(
                "--- Seed {} (error={:+.2} days, ELBO={:.2}) ---",
                seed, t0_error, elbo
            );

            // Check PSO initialization quality
            let measurements: Vec<_> = lightcurve
                .measurements
                .iter()
                .filter(|m| !m.is_upper_limit)
                .collect();

            let peak_flux = measurements
                .iter()
                .map(|m| m.flux)
                .fold(f64::NEG_INFINITY, f64::max);

            println!("  Peak observed flux: {:.2}", peak_flux);
            println!("  Number of detections: {}", measurements.len());
            println!(
                "  Number of upper limits: {}",
                lightcurve
                    .measurements
                    .iter()
                    .filter(|m| m.is_upper_limit)
                    .count()
            );

            // Check flux distribution
            let fluxes: Vec<f64> = measurements.iter().map(|m| m.flux).collect();
            let flux_std = {
                let mean = fluxes.iter().sum::<f64>() / fluxes.len() as f64;
                let variance =
                    fluxes.iter().map(|f| (f - mean).powi(2)).sum::<f64>() / fluxes.len() as f64;
                variance.sqrt()
            };
            println!(
                "  Flux std/mean: {:.3}",
                flux_std / (peak_flux / fluxes.len() as f64)
            );

            // Try PSO with this data
            let mut times: Vec<f64> = Vec::new();
            let mut flux_norm: Vec<f64> = Vec::new();
            let mut flux_err_norm: Vec<f64> = Vec::new();
            let mut is_upper: Vec<bool> = Vec::new();
            let mut upper_flux: Vec<f64> = Vec::new();

            // Add all measurements in order
            for m in &lightcurve.measurements {
                times.push(m.mjd - 60000.0);
                flux_norm.push(m.flux / peak_flux);
                flux_err_norm.push(m.flux_err / peak_flux);
                is_upper.push(m.is_upper_limit);
                upper_flux.push(m.flux / peak_flux);
            }

            let (lower, upper) = pso_bounds(svi_models::SviModel::MetzgerKN);
            let problem = PsoCost {
                times: times.clone(),
                flux: flux_norm.clone(),
                flux_err: flux_err_norm.clone(),
                model: svi_models::SviModel::MetzgerKN,
                is_upper: is_upper.clone(),
                upper_flux: upper_flux.clone(),
            };

            let solver = ParticleSwarm::new((lower, upper), 40);
            let pso_result = Executor::new(problem, solver)
                .configure(|state| state.max_iters(200))
                .run();

            if let Ok(result) = pso_result {
                if let Some(best) = result.state().get_best_param() {
                    let pso_t0 = best.position[3];
                    let pso_cost = result.state().get_cost();
                    println!("  PSO t0: {:.2} (cost={:.2})", pso_t0, pso_cost);
                    println!(
                        "  PSO params: [{:.2}, {:.2}, {:.2}, {:.2}, {:.2}]",
                        best.position[0],
                        best.position[1],
                        best.position[2],
                        best.position[3],
                        best.position[4]
                    );
                } else {
                    println!("  ⚠️  PSO returned NO best particle!");
                }
            } else {
                println!("  ⚠️  PSO FAILED!");
            }
            println!();
        }

        println!("\n=== Recommendations ===");
        if bad_cases.len() > 5 {
            println!("High failure rate ({}%). Consider:", bad_cases.len() * 5);
            println!("  1. Add ELBO threshold: Reject fits with ELBO < -10");
            println!("  2. Increase PSO restarts: Try multiple random seeds");
            println!("  3. Better PSO bounds: Tighten t0 bounds further");
        } else {
            println!(
                "Moderate failure rate ({}%). Acceptable but could improve:",
                bad_cases.len() * 5
            );
            println!("  - Add ELBO validation: Warn if ELBO < 0");
            println!("  - Consider multi-start optimization");
        }
    } else {
        println!("\n✅ No catastrophic failures found in 20 trials!");
    }
}
