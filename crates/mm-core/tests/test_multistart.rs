/// Test multi-start optimization approach
///
/// Run with: cargo test --test test_multistart -- --nocapture --ignored
use mm_core::{fit_lightcurve, fit_quality::FitQualityAssessment, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

/// Simple multi-start wrapper
fn multistart_fit(
    lightcurve: &LightCurve,
    model: FitModel,
    n_starts: usize,
) -> Vec<(mm_core::LightCurveFitResult, f64)> {
    let mut results = Vec::new();

    for i in 0..n_starts {
        // The randomness comes from PSO's internal random initialization
        // Each call will use a different random state
        match fit_lightcurve(lightcurve, model) {
            Ok(fit) => {
                let assessment = FitQualityAssessment::assess(&fit, None);
                println!(
                    "  Start {}: ELBO = {:.2}, t0_err = {:.3} days, Quality = {:?}",
                    i + 1,
                    fit.elbo,
                    fit.t0_err,
                    assessment.quality
                );
                results.push((fit, 0.0)); // Placeholder for additional metrics
            }
            Err(e) => {
                println!("  Start {}: FAILED - {}", i + 1, e);
            }
        }
    }

    results
}

fn generate_synthetic_kilonova(seed: u64) -> (LightCurve, f64) {
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
fn test_multistart_on_good_case() {
    println!("\n=== Multi-Start Test: Good Case (seed=6) ===\n");

    let (lightcurve, true_t0) = generate_synthetic_kilonova(6);

    println!("Running 3 independent fits:\n");
    let results = multistart_fit(&lightcurve, FitModel::MetzgerKN, 3);

    // Find best by ELBO
    let best = results
        .iter()
        .max_by(|a, b| a.0.elbo.partial_cmp(&b.0.elbo).unwrap())
        .unwrap();

    let mjd_offset = 60000.0;
    let true_t0_mjd = mjd_offset + true_t0;
    let t0_error = (best.0.t0 - true_t0_mjd).abs();

    println!("\n=== Best Result ===");
    println!("ELBO: {:.2}", best.0.elbo);
    println!(
        "t0 error: {:.3} days ({:.1} hours)",
        t0_error,
        t0_error * 24.0
    );

    let assessment = FitQualityAssessment::assess(&best.0, Some(mjd_offset + 0.5));
    println!("Quality: {:?}", assessment.quality);
    println!("Acceptable: {}", assessment.is_acceptable);
}

#[test]
#[ignore]
fn test_multistart_on_bad_case() {
    println!("\n=== Multi-Start Test: Bad Case (seed=4) ===\n");
    println!("Seed 4 typically produces optimizer failures in single-start\n");

    let (lightcurve, true_t0) = generate_synthetic_kilonova(4);

    println!("Running 5 independent fits:\n");
    let results = multistart_fit(&lightcurve, FitModel::MetzgerKN, 5);

    // Find best by ELBO
    let best = results
        .iter()
        .max_by(|a, b| a.0.elbo.partial_cmp(&b.0.elbo).unwrap())
        .unwrap();

    let mjd_offset = 60000.0;
    let true_t0_mjd = mjd_offset + true_t0;
    let t0_error = (best.0.t0 - true_t0_mjd).abs();

    println!("\n=== Best Result ===");
    println!("ELBO: {:.2}", best.0.elbo);
    println!(
        "t0 error: {:.3} days ({:.1} hours)",
        t0_error,
        t0_error * 24.0
    );

    let assessment = FitQualityAssessment::assess(&best.0, Some(mjd_offset + 0.5));
    println!("Quality: {:?}", assessment.quality);
    println!("Acceptable: {}", assessment.is_acceptable);

    // Show ELBO distribution
    println!("\n=== ELBO Distribution ===");
    let mut elbos: Vec<f64> = results.iter().map(|r| r.0.elbo).collect();
    elbos.sort_by(|a, b| b.partial_cmp(a).unwrap());
    println!("ELBOs: {:?}", elbos);

    let n_good = results.iter().filter(|r| r.0.elbo > 10.0).count();
    println!("\nGood fits (ELBO > 10): {} / {}", n_good, results.len());

    if n_good > 0 {
        println!("✅ Multi-start RESCUED a bad case!");
    } else {
        println!("⚠️  Even multi-start struggled with this case");
    }
}

#[test]
#[ignore]
fn test_multistart_statistics() {
    println!("\n=== Multi-Start Statistics ===\n");
    println!("Testing 10 different seeds with 3 starts each\n");

    let mut single_start_failures = 0;
    let mut multistart_rescues = 0;

    for seed in 1..=10 {
        let (lightcurve, _true_t0) = generate_synthetic_kilonova(seed);

        let results = multistart_fit(&lightcurve, FitModel::MetzgerKN, 3);

        let best_elbo = results
            .iter()
            .map(|r| r.0.elbo)
            .fold(f64::NEG_INFINITY, f64::max);

        let worst_elbo = results
            .iter()
            .map(|r| r.0.elbo)
            .fold(f64::INFINITY, f64::min);

        let single_would_fail = worst_elbo < -10.0;
        let multi_rescued = single_would_fail && best_elbo > 0.0;

        if single_would_fail {
            single_start_failures += 1;
        }
        if multi_rescued {
            multistart_rescues += 1;
        }

        let status = if multi_rescued {
            "RESCUED ✅"
        } else if single_would_fail {
            "FAILED ❌"
        } else {
            "OK"
        };

        println!(
            "Seed {:2}: Best ELBO = {:7.2}, Worst ELBO = {:7.2} [{}]",
            seed, best_elbo, worst_elbo, status
        );
    }

    println!("\n=== Summary ===");
    println!(
        "Cases where single-start would fail: {} / 10",
        single_start_failures
    );
    println!(
        "Cases rescued by multi-start: {} / {}",
        multistart_rescues, single_start_failures
    );

    if single_start_failures > 0 {
        let rescue_rate = (multistart_rescues as f64 / single_start_failures as f64) * 100.0;
        println!("Multi-start rescue rate: {:.1}%", rescue_rate);
    }
}
