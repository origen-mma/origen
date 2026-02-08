/// Test profile likelihood with conservative config (parallelized)
///
/// Run with: cargo test --test test_t0_profile_conservative -- --nocapture
use mm_core::{
    fit_lightcurve, fit_lightcurve_profile_t0, svi_models, FitConfig, FitModel,
    FitQualityAssessment, LightCurve, Photometry,
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
    let mut lightcurve = LightCurve::new("TEST".to_string());
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
fn test_conservative_profile() {
    println!("\n=== Profile Likelihood vs Joint Optimization ===");
    println!("Conservative config: 10 coarse + 5 fine = 15 grid points (parallelized)\n");

    let (lc, true_t0) = generate_synthetic_kilonova();
    let mjd_offset = 60000.0;
    let true_t0_mjd = mjd_offset + true_t0;

    // Method 1: Joint optimization (baseline)
    println!("Method 1: Joint Optimization (all parameters together)");
    let start = std::time::Instant::now();
    let joint_result = fit_lightcurve(&lc, FitModel::MetzgerKN).unwrap();
    let joint_time = start.elapsed();
    let joint_t0_err = (joint_result.t0 - true_t0_mjd).abs();

    println!(
        "  t0 = {:.3} MJD (error: {:.2} days = {:.1} hours)",
        joint_result.t0,
        joint_t0_err,
        joint_t0_err * 24.0
    );
    println!(
        "  t0_err = ±{:.2} days (±{:.1} hours)",
        joint_result.t0_err,
        joint_result.t0_err * 24.0
    );
    println!("  ELBO = {:.2}", joint_result.elbo);
    println!("  Time: {:.1}s", joint_time.as_secs_f64());

    let joint_quality = FitQualityAssessment::assess(&joint_result, None);
    println!("  Quality: {:?}", joint_quality.quality);

    // Method 2: Profile likelihood
    println!("\nMethod 2: Profile Likelihood (grid search over t0)");
    let config = FitConfig::conservative();
    println!("  Grid size: {:?}", config.profile_grid_size);
    println!(
        "  Total evaluations: {}\n",
        config.profile_grid_size.0 + config.profile_grid_size.1
    );

    let start = std::time::Instant::now();
    let profile_result = fit_lightcurve_profile_t0(&lc, FitModel::MetzgerKN, &config).unwrap();
    let profile_time = start.elapsed();
    let profile_t0_err = (profile_result.t0 - true_t0_mjd).abs();

    println!(
        "  t0 = {:.3} MJD (error: {:.2} days = {:.1} hours)",
        profile_result.t0,
        profile_t0_err,
        profile_t0_err * 24.0
    );
    println!(
        "  t0_err = ±{:.2} days (±{:.1} hours)",
        profile_result.t0_err,
        profile_result.t0_err * 24.0
    );
    println!("  ELBO = {:.2}", profile_result.elbo);
    println!("  Time: {:.1}s", profile_time.as_secs_f64());

    let profile_quality = FitQualityAssessment::assess(&profile_result, None);
    println!("  Quality: {:?}", profile_quality.quality);

    // Comparison
    println!("\n=== Comparison ===");
    println!(
        "t0 error improvement: {:.1}x (joint: {:.2} days, profile: {:.2} days)",
        joint_t0_err / profile_t0_err.max(0.01),
        joint_t0_err,
        profile_t0_err
    );
    println!(
        "Time overhead: {:.1}x slower ({:.1}s vs {:.1}s)",
        profile_time.as_secs_f64() / joint_time.as_secs_f64(),
        profile_time.as_secs_f64(),
        joint_time.as_secs_f64()
    );
    println!(
        "ELBO: joint={:.2}, profile={:.2}",
        joint_result.elbo, profile_result.elbo
    );

    assert!(
        profile_result.elbo > -1000.0,
        "Profile ELBO should not be catastrophic"
    );
}
