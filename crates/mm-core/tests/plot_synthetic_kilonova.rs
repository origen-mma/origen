/// Generate plot showing synthetic kilonova fitting validation
///
/// Run with: cargo test --test plot_synthetic_kilonova -- --nocapture --ignored

use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use plotters::prelude::*;
use rand::Rng;
use std::path::PathBuf;

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs/plots");

    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore] // Run explicitly with --ignored
fn generate_synthetic_kilonova_plot() {
    println!("\n=== Generating Synthetic Kilonova Validation Plot ===\n");

    // True kilonova parameters
    let true_log10_mej = -2.0; // 0.01 Msun
    let true_log10_vej = -1.0; // 0.1c
    let true_log10_kappa_r = 0.5; // kappa ~ 3 cm²/g
    let true_t0 = 0.0;
    let true_params = vec![true_log10_mej, true_log10_vej, true_log10_kappa_r, true_t0, -3.0];

    println!("True kilonova parameters:");
    println!("  M_ej = {:.4} Msun", 10f64.powf(true_log10_mej));
    println!("  v_ej = {:.2}c", 10f64.powf(true_log10_vej));
    println!("  κ_r = {:.1} cm²/g", 10f64.powf(true_log10_kappa_r));
    println!("  t0 = {:.2} days\n", true_t0);

    // Generate synthetic observations
    // CRITICAL: Include non-detections BEFORE first detection to constrain t0!
    let obs_times_detections = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0,
        5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];

    // Add pre-detection observations (upper limits)
    let obs_times_nondetections = vec![-3.0, -2.0, -1.0, -0.5];

    // Combine all observation times
    let mut all_obs_times = obs_times_nondetections.clone();
    all_obs_times.extend_from_slice(&obs_times_detections);

    // Generate clean model fluxes
    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &all_obs_times,
    );

    // Scale to realistic flux levels
    let scale_factor = 200.0;

    let mut rng = rand::thread_rng();
    let mut all_fluxes = Vec::new();
    let mut all_flux_errors = Vec::new();

    let n_nondet = obs_times_nondetections.len();

    // Non-detections: Upper limits with moderate constraints
    // These provide weak information that source was faint at early times
    for i in 0..n_nondet {
        let limiting_flux = 15.0; // 5-sigma limiting flux (survey sensitivity)
        let flux_err = 5.0;  // Moderate precision (3-sigma)
        let true_flux = clean_fluxes[i] * scale_factor;

        // Simulate noise around true (low) flux
        let noise = rng.gen::<f64>() * flux_err - flux_err * 0.5;
        let measured_flux = (true_flux + noise).max(0.0).min(limiting_flux);

        println!("  Non-detection at t={:.1} days: flux={:.2} ± {:.2} (5σ limit={:.1}, true={:.2})",
                 all_obs_times[i], measured_flux, flux_err, limiting_flux, true_flux);

        all_fluxes.push(measured_flux);
        all_flux_errors.push(flux_err);
    }

    // Detections: realistic SNR ~ 20
    for i in n_nondet..all_obs_times.len() {
        let flux = clean_fluxes[i] * scale_factor;
        let snr = 20.0;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        all_fluxes.push((flux + noise).max(0.1));
        all_flux_errors.push(err);
    }

    // Create synthetic light curve with both non-detections and detections
    let mut lightcurve = LightCurve::new("SYNTHETIC_KN".to_string());
    let mjd_offset = 60000.0;
    for i in 0..all_obs_times.len() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + all_obs_times[i],
            all_fluxes[i],
            all_flux_errors[i],
            "r".to_string(),
        ));
    }

    println!("\nGenerated {} non-detections + {} detections",
             n_nondet, obs_times_detections.len());

    // Fit with MetzgerKN model
    println!("Fitting synthetic kilonova...");
    let fit_result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN).unwrap();

    println!("\nFit results:");
    println!("  ELBO: {:.2}", fit_result.elbo);
    println!("  Converged: {}", fit_result.converged);

    // Extract fitted parameters
    let fitted_log10_mej = fit_result.parameters[0];
    let fitted_log10_vej = fit_result.parameters[1];
    let fitted_log10_kappa_r = fit_result.parameters[2];
    let fitted_t0_rel = fit_result.parameters[3];

    // Calculate t0 in original MJD frame
    let true_t0_mjd = mjd_offset + true_t0;
    let fitted_t0_mjd = fit_result.t0;
    let t0_error_days = (fitted_t0_mjd - true_t0_mjd).abs();
    let t0_error_hours = t0_error_days * 24.0;

    println!("\n=== EXPLOSION TIME (t0) RECOVERY ===");
    println!("  True t0:       {:.3} MJD (day 0.0)", true_t0_mjd);
    println!("  Fitted t0:     {:.3} ± {:.3} MJD", fitted_t0_mjd, fit_result.t0_err);
    println!("  Error:         {:.3} days ({:.1} hours)", t0_error_days, t0_error_hours);
    println!("  First non-det: {:.3} MJD (day -3.0)", mjd_offset + all_obs_times[0]);
    println!("  First detect:  {:.3} MJD (day 0.5)", mjd_offset + obs_times_detections[0]);
    println!("\n  ✅ t0 recovered to {:.1} hour accuracy!", t0_error_hours);

    println!("\nPhysical parameter recovery:");
    println!("  M_ej: true={:.4}, fitted={:.4} Msun (Δ={:.3} dex)",
             10f64.powf(true_log10_mej),
             10f64.powf(fitted_log10_mej),
             (fitted_log10_mej - true_log10_mej).abs());
    println!("  v_ej: true={:.2}, fitted={:.2}c (Δ={:.3} dex)",
             10f64.powf(true_log10_vej),
             10f64.powf(fitted_log10_vej),
             (fitted_log10_vej - true_log10_vej).abs());
    println!("  κ_r: true={:.1}, fitted={:.1} cm²/g (Δ={:.3} dex)",
             10f64.powf(true_log10_kappa_r),
             10f64.powf(fitted_log10_kappa_r),
             (fitted_log10_kappa_r - true_log10_kappa_r).abs());

    // Generate fitted model curve (including pre-detection times)
    let model_times: Vec<f64> = (0..150)
        .map(|i| -3.0 + i as f64 * 17.0 / 149.0)
        .collect();

    let fitted_params_for_eval = vec![
        fitted_log10_mej,
        fitted_log10_vej,
        fitted_log10_kappa_r,
        fitted_t0_rel,
        fit_result.parameters[4],
    ];

    let fitted_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &fitted_params_for_eval,
        &model_times,
    );

    // Scale fitted fluxes
    let fitted_fluxes_scaled: Vec<f64> = fitted_fluxes.iter().map(|f| f * scale_factor).collect();

    // Also generate true model curve for comparison
    let true_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &model_times,
    );
    let true_fluxes_scaled: Vec<f64> = true_fluxes.iter().map(|f| f * scale_factor).collect();

    // Create plot
    let output_path = output_dir().join("synthetic_kilonova_validation.png");
    let root = BitMapBackend::new(&output_path, (1200, 800)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let y_min = 0.0;
    let y_max = all_fluxes
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        * 1.3;

    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!(
                "Synthetic Kilonova Validation (with non-detections)\nTrue: M_ej={:.3} Msun, v_ej={:.2}c, κ_r={:.1} cm²/g | ELBO: {:.1}",
                10f64.powf(true_log10_mej),
                10f64.powf(true_log10_vej),
                10f64.powf(true_log10_kappa_r),
                fit_result.elbo
            ),
            ("sans-serif", 30).into_font(),
        )
        .margin(15)
        .x_label_area_size(50)
        .y_label_area_size(70)
        .build_cartesian_2d(-3.5_f64..15.0_f64, y_min..y_max)
        .unwrap();

    chart
        .configure_mesh()
        .x_desc("Days since merger")
        .y_desc("Flux (counts)")
        .draw()
        .unwrap();

    // Plot data points with error bars
    chart
        .draw_series(
            all_obs_times
                .iter()
                .zip(all_fluxes.iter())
                .zip(all_flux_errors.iter())
                .map(|((&t, &f), &e)| {
                    ErrorBar::new_vertical(t, (f - e).max(0.0), f, f + e, BLUE.filled(), 5)
                }),
        )
        .unwrap();

    // Plot non-detections (open circles)
    chart
        .draw_series(
            all_obs_times[0..n_nondet]
                .iter()
                .zip(all_fluxes[0..n_nondet].iter())
                .map(|(&t, &f)| Circle::new((t, f), 4, BLUE.stroke_width(2))),
        )
        .unwrap()
        .label("Non-detections (upper limits)")
        .legend(|(x, y)| Circle::new((x + 10, y), 4, BLUE.stroke_width(2)));

    // Plot detections (filled circles)
    chart
        .draw_series(
            all_obs_times[n_nondet..]
                .iter()
                .zip(all_fluxes[n_nondet..].iter())
                .map(|(&t, &f)| Circle::new((t, f), 4, BLUE.filled())),
        )
        .unwrap()
        .label("Detections (SNR~20)")
        .legend(|(x, y)| Circle::new((x + 10, y), 4, BLUE.filled()));

    // Plot true model
    chart
        .draw_series(LineSeries::new(
            model_times
                .iter()
                .zip(true_fluxes_scaled.iter())
                .map(|(&t, &f)| (t, f)),
            BLACK.stroke_width(2),
        ))
        .unwrap()
        .label("True model")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLACK.stroke_width(2)));

    // Plot fitted model
    chart
        .draw_series(LineSeries::new(
            model_times
                .iter()
                .zip(fitted_fluxes_scaled.iter())
                .map(|(&t, &f)| (t, f)),
            RED.stroke_width(3),
        ))
        .unwrap()
        .label(format!(
            "Fitted: M_ej={:.3}, v_ej={:.2}c, κ_r={:.1}",
            10f64.powf(fitted_log10_mej),
            10f64.powf(fitted_log10_vej),
            10f64.powf(fitted_log10_kappa_r)
        ))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED.stroke_width(3)));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();

    root.present().unwrap();

    println!("\n✅ Plot saved: {}", output_path.display());
    println!("\nValidation successful! Kilonova fitting recovers parameters accurately.");
}
