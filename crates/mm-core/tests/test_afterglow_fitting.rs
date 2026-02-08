/// Test afterglow (GRB optical counterpart) fitting with PowerLaw model
///
/// Run with: cargo test --test test_afterglow_fitting -- --nocapture --ignored
use mm_core::{fit_lightcurve, FitModel, FitQualityAssessment, LightCurve, Photometry};
use rand::Rng;

/// Generate synthetic GRB afterglow light curve (PowerLaw decay)
fn generate_synthetic_afterglow(seed: u64) -> (LightCurve, f64) {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    // True parameters for PowerLaw: log_a, log_alpha, log_beta, t0
    let true_log_a: f64 = 5.0; // Amplitude (flux ~ 150)
    let true_log_alpha: f64 = -1.0; // Rise index: alpha ~ 0.37 (fast rise)
    let true_log_beta: f64 = 0.4; // Decay index: beta ~ 2.5 (steep decay)
    let true_t0: f64 = 0.0; // GRB trigger time

    // GRB afterglows: rapid rise (hours), then power-law decay (days to weeks)
    // Typical observations: First detection ~hours after burst, peak ~1 day, then decline
    let obs_times = vec![0.1, 0.5, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 14.0, 21.0, 30.0];

    // Generate fluxes using PowerLaw model approximation
    // f(t) ~ a * (t - t0)^alpha * exp(-(t - t0)^beta)
    let mut fluxes = Vec::new();
    let mut errors = Vec::new();

    for &t in &obs_times {
        let dt = t - true_t0;
        let a = true_log_a.exp();
        let alpha = true_log_alpha.exp();
        let beta = true_log_beta.exp();

        let flux_clean = a * dt.powf(alpha) * (-dt.powf(beta)).exp();

        // Add noise (SNR ~ 20 at peak, lower at late times)
        let snr = if t < 5.0 { 20.0 } else { 10.0 };
        let err = flux_clean / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let flux_noisy = (flux_clean + noise).max(0.1);

        fluxes.push(flux_noisy);
        errors.push(err);
    }

    let mut lightcurve = LightCurve::new(format!("GRB_AFTERGLOW_{}", seed));
    let mjd_offset = 59000.0;

    for (i, &t) in obs_times.iter().enumerate() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + t,
            fluxes[i],
            errors[i],
            "R".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

#[test]
#[ignore]
fn test_afterglow_powerlaw_fitting() {
    println!("\n=== Testing Afterglow (PowerLaw) Fitting ===\n");
    println!("PowerLaw model: f(t) = a * (t - t0)^α * exp(-(t - t0)^β)");
    println!("Use case: GRB optical afterglows\n");

    let mut results = Vec::new();

    for seed in 1..=10 {
        let (lc, true_t0) = generate_synthetic_afterglow(seed);

        match fit_lightcurve(&lc, FitModel::PowerLaw) {
            Ok(fit) => {
                let mjd_offset = 59000.0;
                let true_t0_mjd = mjd_offset + true_t0;
                let t0_error = (fit.t0 - true_t0_mjd).abs();

                let assessment = FitQualityAssessment::assess(&fit, Some(mjd_offset + 0.1));

                println!(
                    "Seed {:2}: ELBO = {:8.2}, t0_err = {:5.2} days, Quality = {:?}",
                    seed, fit.elbo, t0_error, assessment.quality
                );

                results.push((seed, fit.elbo, t0_error, assessment.quality));
            }
            Err(e) => {
                println!("Seed {:2}: FAILED - {}", seed, e);
            }
        }
    }

    println!("\n=== Summary ===");
    let n_total = results.len();

    if n_total == 0 {
        println!("All fits failed!");
        return;
    }

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

    println!("Total: {}", n_total);
    println!(
        "Good (ELBO > 10): {} ({:.0}%)",
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

    // Calculate median t0 error
    let mut t0_errors: Vec<f64> = results.iter().map(|(_, _, err, _)| *err).collect();
    t0_errors.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = t0_errors[t0_errors.len() / 2];

    println!(
        "\nMedian t0 error: {:.2} days ({:.1} hours)",
        median,
        median * 24.0
    );

    // Check if PowerLaw is affected by same issues as MetzgerKN
    if n_failed > 2 {
        println!("\n⚠️  PowerLaw model also shows instability");
    } else if n_good > 5 {
        println!("\n✅ PowerLaw model is stable (no normalization issues)");
    } else {
        println!("\n⚠️  PowerLaw has moderate success rate");
    }
}

#[test]
#[ignore]
fn test_afterglow_vs_kilonova_comparison() {
    println!("\n=== Afterglow (PowerLaw) vs Kilonova (MetzgerKN) Comparison ===\n");

    // Test one afterglow
    let (afterglow_lc, _) = generate_synthetic_afterglow(1);
    println!("Testing PowerLaw (Afterglow):");
    match fit_lightcurve(&afterglow_lc, FitModel::PowerLaw) {
        Ok(fit) => {
            println!("  ELBO: {:.2}", fit.elbo);
            println!("  Converged: {}", fit.converged);
            let assessment = FitQualityAssessment::assess(&fit, None);
            println!("  Quality: {:?}", assessment.quality);
        }
        Err(e) => println!("  FAILED: {}", e),
    }

    println!("\nKey Differences:");
    println!("  - PowerLaw: No internal normalization (simple parametric model)");
    println!("  - MetzgerKN: Physics-based with internal normalization");
    println!("  - PowerLaw: Should NOT be affected by normalization fix");
    println!("  - MetzgerKN: Fixes specifically target this model");
}
