/// Debug catastrophic ELBO failures
///
/// Run with: cargo test --test debug_catastrophic_elbo -- --nocapture --ignored
use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

fn generate_synthetic_kilonova(seed: u64) -> LightCurve {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let true_params = vec![-2.0, -1.0, 0.5, 0.0, -3.0];

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
    let mut lightcurve = LightCurve::new(format!("SEED_{}", seed));
    let mjd_offset = 60000.0;

    let n_nondet = obs_times_nondetections.len();
    let limiting_flux = 15.0;

    for i in 0..n_nondet {
        let flux_err = 5.0;
        let true_flux = clean_fluxes[i] * scale_factor;
        let noise = rng.gen::<f64>() * flux_err - flux_err * 0.5;
        let measured_flux = (true_flux + noise).max(0.0).min(limiting_flux);

        lightcurve.add_measurement(Photometry::new_upper_limit(
            mjd_offset + all_obs_times[i],
            limiting_flux,
            "r".to_string(),
        ));
    }

    for i in n_nondet..all_obs_times.len() {
        let flux = clean_fluxes[i] * scale_factor;
        let snr = 20.0;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let scaled_flux = (flux + noise).max(0.1);

        lightcurve.add_measurement(Photometry::new(
            mjd_offset + all_obs_times[i],
            scaled_flux,
            err,
            "r".to_string(),
        ));
    }

    lightcurve
}

#[test]
#[ignore]
fn test_with_fixes() {
    println!("\n=== Testing With All Fixes Applied ===\n");
    println!("Fixes:");
    println!("  1. Numerical safeguards (scale factor clamping)");
    println!("  2. Lower learning rate (0.01 → 0.005)");
    println!("  3. NaN/Inf checks\n");

    println!("Running 10 trials to measure improvement:\n");

    let mut results = Vec::new();

    for seed in 1..=10 {
        let lc = generate_synthetic_kilonova(seed);

        match fit_lightcurve(&lc, FitModel::MetzgerKN) {
            Ok(fit) => {
                let t0_error = (fit.t0 - 60000.0).abs();
                let quality = if fit.elbo > 50.0 {
                    "Excellent"
                } else if fit.elbo > 10.0 {
                    "Good"
                } else if fit.elbo > 0.0 {
                    "Fair"
                } else if fit.elbo > -10.0 {
                    "Poor"
                } else {
                    "Failed"
                };

                println!(
                    "Seed {:2}: ELBO = {:10.2}, t0_err = {:5.2} days, Quality = {}",
                    seed, fit.elbo, t0_error, quality
                );

                results.push((seed, fit.elbo, t0_error, quality));
            }
            Err(e) => {
                println!("Seed {:2}: CRASHED - {}", seed, e);
            }
        }
    }

    println!("\n=== Summary ===");
    let n_total = results.len();
    let n_good = results
        .iter()
        .filter(|(_, elbo, _, _)| *elbo > 10.0)
        .count();
    let n_fair = results
        .iter()
        .filter(|(_, elbo, _, _)| *elbo > 0.0 && *elbo <= 10.0)
        .count();
    let n_poor = results
        .iter()
        .filter(|(_, elbo, _, _)| *elbo > -10.0 && *elbo <= 0.0)
        .count();
    let n_failed = results
        .iter()
        .filter(|(_, elbo, _, _)| *elbo <= -10.0)
        .count();

    println!("Total trials: {}", n_total);
    println!(
        "Excellent/Good (ELBO > 10): {} ({:.0}%)",
        n_good,
        (n_good as f64 / n_total as f64) * 100.0
    );
    println!(
        "Fair (ELBO 0-10): {} ({:.0}%)",
        n_fair,
        (n_fair as f64 / n_total as f64) * 100.0
    );
    println!(
        "Poor (ELBO -10 to 0): {} ({:.0}%)",
        n_poor,
        (n_poor as f64 / n_total as f64) * 100.0
    );
    println!(
        "Failed (ELBO < -10): {} ({:.0}%)",
        n_failed,
        (n_failed as f64 / n_total as f64) * 100.0
    );

    // Check for catastrophic failures
    let has_catastrophic = results.iter().any(|(_, elbo, _, _)| *elbo < -1000.0);
    if has_catastrophic {
        println!("\n❌ CATASTROPHIC FAILURES STILL PRESENT!");
        for (seed, elbo, _, _) in results.iter().filter(|(_, e, _, _)| *e < -1000.0) {
            println!("  Seed {}: ELBO = {:.2}", seed, elbo);
        }
    } else {
        println!("\n✅ No catastrophic failures (ELBO > -1000)");
    }

    // Calculate median t0 error for good fits
    let good_errors: Vec<f64> = results
        .iter()
        .filter(|(_, elbo, _, _)| *elbo > 0.0)
        .map(|(_, _, err, _)| *err)
        .collect();

    if !good_errors.is_empty() {
        let mut sorted = good_errors.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        println!(
            "\nMedian t0 error (good fits): {:.2} days ({:.1} hours)",
            median,
            median * 24.0
        );
    }
}
