/// Diagnose systematic bias in t0 recovery
///
/// Run with: cargo test --test diagnose_t0_bias -- --nocapture --ignored
use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

#[test]
#[ignore]
fn diagnose_t0_bias() {
    println!("\n=== Diagnosing t0 Bias ===\n");

    // True kilonova parameters
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

    // Generate synthetic observations
    let obs_times_detections = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];
    let obs_times_nondetections = vec![-3.0, -2.0, -1.0, -0.5];

    let mut all_obs_times = obs_times_nondetections.clone();
    all_obs_times.extend_from_slice(&obs_times_detections);

    // Generate clean model fluxes
    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &all_obs_times,
    );

    let scale_factor = 200.0;

    println!("True model fluxes at key times:");
    for i in 0..all_obs_times.len() {
        let scaled = clean_fluxes[i] * scale_factor;
        let label = if i < obs_times_nondetections.len() {
            "NON-DET"
        } else {
            "DETECT"
        };
        println!(
            "  t={:5.1} days: flux={:8.2} [{}]",
            all_obs_times[i], scaled, label
        );
    }

    println!("\n5-sigma limiting flux: 15.0");
    println!("\nExpected behavior:");
    println!("  - Non-detections should have flux << 15.0");
    println!("  - First detection (t=0.5) should have flux > 15.0");
    println!("  - Model should infer t0 between -0.5 and 0.5\n");

    // Now test with different upper limit configurations
    println!("=== Test 1: Current configuration (4 non-dets before, 18 dets after) ===\n");
    test_configuration(
        &all_obs_times,
        &clean_fluxes,
        scale_factor,
        obs_times_nondetections.len(),
        true_t0,
        "current",
    );

    // Test with more non-detections
    println!("\n=== Test 2: More non-detections (8 before, 18 after) ===\n");
    let more_nondet_times = vec![-6.0, -5.0, -4.0, -3.0, -2.0, -1.5, -1.0, -0.5];
    let mut all_times_v2 = more_nondet_times.clone();
    all_times_v2.extend_from_slice(&obs_times_detections);
    let fluxes_v2 =
        svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &true_params, &all_times_v2);
    test_configuration(
        &all_times_v2,
        &fluxes_v2,
        scale_factor,
        more_nondet_times.len(),
        true_t0,
        "more_nondet",
    );

    // Test with symmetric distribution
    println!("\n=== Test 3: Symmetric (4 before, 4 after, rest later) ===\n");
    let symmetric_times = vec![
        -3.0, -2.0, -1.0, -0.5, // 4 non-dets before
        0.5, 1.0, 1.5, 2.0, // 4 dets right after
        3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 14.0, // later dets
    ];
    let fluxes_v3 = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &symmetric_times,
    );
    test_configuration(
        &symmetric_times,
        &fluxes_v3,
        scale_factor,
        4,
        true_t0,
        "symmetric",
    );
}

fn test_configuration(
    obs_times: &[f64],
    clean_fluxes: &[f64],
    scale_factor: f64,
    n_nondet: usize,
    true_t0: f64,
    label: &str,
) {
    let mut rng = rand::thread_rng();
    let mut all_fluxes = Vec::new();
    let mut all_flux_errors = Vec::new();

    let limiting_flux = 15.0;

    // Non-detections
    for i in 0..n_nondet {
        let flux_err = 5.0;
        let true_flux = clean_fluxes[i] * scale_factor;
        let noise = rng.gen::<f64>() * flux_err - flux_err * 0.5;
        let measured_flux = (true_flux + noise).max(0.0).min(limiting_flux);
        all_fluxes.push(measured_flux);
        all_flux_errors.push(flux_err);
    }

    // Detections
    for i in n_nondet..obs_times.len() {
        let flux = clean_fluxes[i] * scale_factor;
        let snr = 20.0;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        all_fluxes.push((flux + noise).max(0.1));
        all_flux_errors.push(err);
    }

    // Create light curve
    let mut lightcurve = LightCurve::new(format!("TEST_{}", label));
    let mjd_offset = 60000.0;

    for i in 0..n_nondet {
        lightcurve.add_measurement(Photometry::new_upper_limit(
            mjd_offset + obs_times[i],
            limiting_flux,
            "r".to_string(),
        ));
    }

    for i in n_nondet..obs_times.len() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + obs_times[i],
            all_fluxes[i],
            all_flux_errors[i],
            "r".to_string(),
        ));
    }

    // Fit
    let fit_result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN).unwrap();

    let true_t0_mjd = mjd_offset + true_t0;
    let fitted_t0_mjd = fit_result.t0;
    let t0_error_days = fitted_t0_mjd - true_t0_mjd; // Signed error!

    println!("Configuration: {}", label);
    println!("  Non-detections: {}", n_nondet);
    println!("  Detections: {}", obs_times.len() - n_nondet);
    println!("  True t0:    {:.3} MJD", true_t0_mjd);
    println!("  Fitted t0:  {:.3} MJD", fitted_t0_mjd);
    println!(
        "  **BIAS**:   {:+.3} days ({:+.1} hours)",
        t0_error_days,
        t0_error_days * 24.0
    );
    println!("  t0_err (reported): {:.3} days", fit_result.t0_err);
    println!("  ELBO: {:.2}", fit_result.elbo);
}
