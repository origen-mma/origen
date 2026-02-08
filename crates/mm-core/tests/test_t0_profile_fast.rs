/// Quick validation test for profile likelihood with fast config
///
/// Run with: cargo test --test test_t0_profile_fast -- --nocapture
use mm_core::{
    fit_lightcurve_profile_t0, svi_models, FitConfig, FitModel, FitQualityAssessment, LightCurve,
    Photometry,
};

fn generate_synthetic_kilonova() -> (LightCurve, f64) {
    use rand::Rng;
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let true_t0: f64 = 0.0;
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
    let mut lightcurve = LightCurve::new("VALIDATION".to_string());
    let mjd_offset = 60000.0;

    let n_nondet = obs_times_nondetections.len();
    let limiting_flux = 15.0;

    for i in 0..n_nondet {
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

    (lightcurve, true_t0)
}

#[test]
fn test_fast_profile_validation() {
    println!("\n=== Fast Profile Likelihood Validation ===");
    println!("Config: Fast (5 coarse + 3 fine = 8 grid points, parallelized)\n");

    let (lc, true_t0) = generate_synthetic_kilonova();
    let mjd_offset = 60000.0;
    let true_t0_mjd = mjd_offset + true_t0;

    // Use fast config for quick validation
    let config = FitConfig::fast();
    println!("Grid size: {:?}", config.profile_grid_size);
    println!(
        "Total evaluations: {}\n",
        config.profile_grid_size.0 + config.profile_grid_size.1
    );

    let start = std::time::Instant::now();
    let result = fit_lightcurve_profile_t0(&lc, FitModel::MetzgerKN, &config).unwrap();
    let elapsed = start.elapsed();

    let t0_err = (result.t0 - true_t0_mjd).abs();

    println!("Results:");
    println!(
        "  t0 = {:.3} MJD (error: {:.2} days = {:.1} hours)",
        result.t0,
        t0_err,
        t0_err * 24.0
    );
    println!(
        "  t0_err = ±{:.2} days (±{:.1} hours)",
        result.t0_err,
        result.t0_err * 24.0
    );
    println!("  ELBO = {:.2}", result.elbo);
    println!("  Time elapsed: {:.1}s", elapsed.as_secs_f64());

    let quality = FitQualityAssessment::assess(&result, None);
    println!("  Quality: {:?}", quality.quality);

    // Basic sanity checks
    assert!(
        result.elbo > -1000.0,
        "ELBO should not be catastrophically bad"
    );
    println!("\n✅ Fast validation passed!");
}
