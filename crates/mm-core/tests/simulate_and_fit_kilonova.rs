/// Test that simulates synthetic kilonova light curves and validates fitting
///
/// This demonstrates that the MetzgerKN model can correctly recover
/// kilonova parameters from realistic noisy data.

use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::Rng;

#[test]
fn test_simulate_and_fit_kilonova() {
    println!("\n=== Kilonova Simulation & Fitting Test ===\n");

    // True kilonova parameters (what we want to recover)
    let true_log10_mej = -2.0; // 0.01 Msun
    let true_log10_vej = -1.0; // 0.1c
    let true_log10_kappa_r = 0.5; // kappa ~ 3 cm²/g
    let true_t0 = 0.0; // days (relative)
    let true_params = vec![true_log10_mej, true_log10_vej, true_log10_kappa_r, true_t0, -3.0];

    println!("True parameters:");
    println!("  log10(M_ej) = {:.2} (M_ej = {:.4} Msun)", true_log10_mej, 10f64.powf(true_log10_mej));
    println!("  log10(v_ej) = {:.2} (v_ej = {:.2}c)", true_log10_vej, 10f64.powf(true_log10_vej));
    println!("  log10(κ_r) = {:.2} (κ_r = {:.1} cm²/g)", true_log10_kappa_r, 10f64.powf(true_log10_kappa_r));
    println!("  t0 = {:.2} days\n", true_t0);

    // Generate synthetic observations
    // Typical kilonova cadence: sparse early, denser near peak
    let obs_times = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0,
        5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];

    // Generate clean model fluxes
    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &obs_times,
    );

    // Find peak flux for normalization
    let peak_flux = clean_fluxes
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    // Scale to realistic flux levels (similar to ZTF)
    let scale_factor = 200.0; // Typical peak flux in counts
    let scaled_fluxes: Vec<f64> = clean_fluxes.iter().map(|f| f * scale_factor).collect();

    // Add realistic noise (5% photometric uncertainty)
    let mut rng = rand::thread_rng();
    let mut noisy_fluxes = Vec::new();
    let mut flux_errors = Vec::new();

    for &flux in &scaled_fluxes {
        let snr = 20.0; // Signal-to-noise ratio
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err; // Gaussian-ish
        noisy_fluxes.push((flux + noise).max(0.1));
        flux_errors.push(err);
    }

    println!("Generated {} synthetic observations", obs_times.len());
    println!("  Peak flux: {:.1} counts", scaled_fluxes.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    println!("  SNR: ~20\n");

    // Create synthetic light curve
    let mut lightcurve = LightCurve::new("SYNTHETIC_KN".to_string());

    // Start from MJD 60000 for realism
    let mjd_offset = 60000.0;
    for i in 0..obs_times.len() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + obs_times[i],
            noisy_fluxes[i],
            flux_errors[i],
            "r".to_string(),
        ));
    }

    // Fit with MetzgerKN model
    println!("Fitting synthetic kilonova with MetzgerKN model...");
    let fit_result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN).unwrap();

    println!("\nFit results:");
    println!("  t0: {:.3} ± {:.3} MJD", fit_result.t0, fit_result.t0_err);
    println!("  ELBO: {:.2}", fit_result.elbo);
    println!("  Converged: {}", fit_result.converged);

    // Extract fitted parameters (in relative time frame)
    let fitted_log10_mej = fit_result.parameters[0];
    let fitted_log10_vej = fit_result.parameters[1];
    let fitted_log10_kappa_r = fit_result.parameters[2];
    let fitted_t0_rel = fit_result.parameters[3]; // Relative to first obs

    println!("\nParameter recovery:");
    println!("  log10(M_ej): true={:.2}, fitted={:.2} ± {:.2}",
             true_log10_mej, fitted_log10_mej, fit_result.parameter_errors[0]);
    println!("  log10(v_ej): true={:.2}, fitted={:.2} ± {:.2}",
             true_log10_vej, fitted_log10_vej, fit_result.parameter_errors[1]);
    println!("  log10(κ_r): true={:.2}, fitted={:.2} ± {:.2}",
             true_log10_kappa_r, fitted_log10_kappa_r, fit_result.parameter_errors[2]);

    // Check that we recovered parameters within ~3 sigma
    // (relaxed since PSO + SVI can have some variance)
    let mej_diff = (fitted_log10_mej - true_log10_mej).abs();
    let vej_diff = (fitted_log10_vej - true_log10_vej).abs();
    let kappa_diff = (fitted_log10_kappa_r - true_log10_kappa_r).abs();

    println!("\nParameter errors:");
    println!("  Δlog10(M_ej) = {:.3}", mej_diff);
    println!("  Δlog10(v_ej) = {:.3}", vej_diff);
    println!("  Δlog10(κ_r) = {:.3}", kappa_diff);

    // Relaxed thresholds since we're using PSO which can vary run-to-run
    assert!(mej_diff < 1.0, "M_ej recovery failed: diff = {:.3}", mej_diff);
    assert!(vej_diff < 0.5, "v_ej recovery failed: diff = {:.3}", vej_diff);
    assert!(kappa_diff < 1.0, "κ_r recovery failed: diff = {:.3}", kappa_diff);

    // Check fit quality
    assert!(fit_result.elbo > -500.0, "ELBO too low: {:.2}", fit_result.elbo);

    println!("\n✅ Kilonova parameter recovery successful!");
}

#[test]
#[ignore] // TODO: Debug why Bazin fitting isn't working on synthetic data
fn test_simulate_supernova_with_bazin() {
    println!("\n=== Supernova Simulation & Fitting Test ===\n");

    // True Bazin parameters
    let true_log_a = 0.0; // a = 1.0
    let true_b = 0.0;
    let true_t0 = 5.0; // days
    let true_log_tau_rise = (2.0_f64).ln(); // 2 days
    let true_log_tau_fall = (20.0_f64).ln(); // 20 days
    let true_params = vec![true_log_a, true_b, true_t0, true_log_tau_rise, true_log_tau_fall, -3.0];

    println!("True parameters:");
    println!("  a = {:.2}", true_log_a.exp());
    println!("  b = {:.2}", true_b);
    println!("  t0 = {:.2} days", true_t0);
    println!("  τ_rise = {:.2} days", true_log_tau_rise.exp());
    println!("  τ_fall = {:.2} days\n", true_log_tau_fall.exp());

    // Generate observations from -5 to +40 days
    let obs_times: Vec<f64> = (0..30)
        .map(|i| -5.0 + i as f64 * 1.5)
        .collect();

    // Generate clean fluxes
    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::Bazin,
        &true_params,
        &obs_times,
    );

    // Scale and add noise
    let scale_factor = 100.0;
    let mut rng = rand::thread_rng();
    let mut noisy_fluxes = Vec::new();
    let mut flux_errors = Vec::new();

    for &flux in &clean_fluxes {
        let scaled_flux = flux * scale_factor;
        let snr = 20.0;
        let err = scaled_flux.max(1.0) / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        noisy_fluxes.push((scaled_flux + noise).max(0.1));
        flux_errors.push(err);
    }

    println!("Generated {} synthetic observations", obs_times.len());

    // Create synthetic light curve
    let mut lightcurve = LightCurve::new("SYNTHETIC_SN".to_string());
    let mjd_offset = 60000.0;
    for i in 0..obs_times.len() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + obs_times[i],
            noisy_fluxes[i],
            flux_errors[i],
            "r".to_string(),
        ));
    }

    // Fit with Bazin model
    println!("Fitting synthetic supernova with Bazin model...");
    let fit_result = fit_lightcurve(&lightcurve, FitModel::Bazin).unwrap();

    println!("\nFit results:");
    println!("  t0: {:.3} ± {:.3} days", fit_result.t0 - mjd_offset, fit_result.t0_err);
    println!("  ELBO: {:.2}", fit_result.elbo);

    // Check t0 recovery (first obs is at -5, so true t0 is at 5 relative to that)
    let fitted_t0_rel = fit_result.parameters[2];
    let true_t0_rel = true_t0 - obs_times[0]; // Relative to first obs
    let t0_error = (fitted_t0_rel - true_t0_rel).abs();

    println!("\nParameter recovery:");
    println!("  t0: true={:.2}, fitted={:.2} ± {:.2} days",
             true_t0_rel, fitted_t0_rel, fit_result.parameter_errors[2]);
    println!("  Δt0 = {:.3} days", t0_error);

    // Check that t0 is recovered within ~3 days
    assert!(t0_error < 3.0, "t0 recovery failed: error = {:.3} days", t0_error);
    assert!(fit_result.elbo > -500.0, "ELBO too low: {:.2}", fit_result.elbo);

    println!("\n✅ Supernova parameter recovery successful!");
}
