/// Verify that the normalization fix eliminates systematic bias
///
/// Run with: cargo test --test verify_bias_fix -- --nocapture --ignored
use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

fn generate_and_fit(trial: usize) -> (f64, f64) {
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
    let mut rng = rand::thread_rng();
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

    let mut lightcurve = LightCurve::new(format!("TRIAL_{}", trial));
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

    let fit_result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN).unwrap();

    let true_t0_mjd = mjd_offset + true_t0;
    let fitted_t0_mjd = fit_result.t0;
    let t0_error_days = fitted_t0_mjd - true_t0_mjd; // Signed error

    (t0_error_days, fit_result.elbo)
}

#[test]
#[ignore]
fn verify_bias_fix_multiple_trials() {
    println!("\n=== Verifying Bias Fix (10 trials) ===\n");

    let mut errors = Vec::new();
    for trial in 1..=10 {
        let (error, elbo) = generate_and_fit(trial);
        println!(
            "Trial {:2}: t0_error = {:+.3} days ({:+.1} hrs), ELBO = {:.2}",
            trial,
            error,
            error * 24.0,
            elbo
        );
        errors.push(error);
    }

    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let variance = errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / errors.len() as f64;
    let std = variance.sqrt();

    println!("\n=== Statistics ===");
    println!(
        "Mean error:   {:+.3} ± {:.3} days ({:+.1} ± {:.1} hrs)",
        mean,
        std,
        mean * 24.0,
        std * 24.0
    );
    println!("Median error: {:+.3} days", {
        let mut sorted = errors.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    });
    println!(
        "Range: [{:+.3}, {:+.3}] days",
        errors.iter().cloned().fold(f64::INFINITY, f64::min),
        errors.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    );

    // Count positive vs negative errors
    let n_positive = errors.iter().filter(|&&e| e > 0.0).count();
    let n_negative = errors.iter().filter(|&&e| e < 0.0).count();
    println!(
        "\nBias direction: {} positive, {} negative",
        n_positive, n_negative
    );

    if mean.abs() < 0.3 && n_positive >= 3 && n_negative >= 3 {
        println!(
            "\n✅ SUCCESS: Bias is small ({:.1} hrs) and scattered around zero!",
            mean * 24.0
        );
    } else if mean.abs() < 0.3 {
        println!("\n⚠️  PARTIAL: Mean bias is small but may have directional preference");
    } else {
        println!(
            "\n❌ FAILURE: Systematic bias of {:+.1} hours persists",
            mean * 24.0
        );
    }
}
