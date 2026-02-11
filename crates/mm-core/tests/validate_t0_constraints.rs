/// Validate t0 (explosion time) constraints for kilonova vs supernova
///
/// This test measures how well we can recover the true explosion time from
/// noisy optical light curves, which is critical for temporal discrimination
/// in multi-messenger correlation.
///
/// Key questions:
/// - Can we distinguish a kilonova at t0=0 from a supernova at random t0?
/// - What is the typical t0 uncertainty for each transient type?
/// - Does cadence/SNR affect discrimination power?
use mm_core::{fit_lightcurve, svi_models, FitModel, LightCurve, Photometry};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::Write;

/// Generate synthetic kilonova with realistic ZTF cadence
fn generate_kilonova_lightcurve(
    true_t0: f64,
    mjd_offset: f64,
    rng: &mut StdRng,
    snr: f64,
) -> (LightCurve, f64) {
    // Kilonova parameters (typical BNS merger)
    let params = vec![
        -2.0, // log10(M_ej) = 0.01 Msun
        -1.0, // log10(v_ej) = 0.1c
        0.5,  // log10(κ_r) ~ 3 cm²/g
        true_t0, -3.0,
    ];

    // Realistic ZTF cadence: sparse early, denser near peak
    // Kilonova fades quickly, so most obs within first 2 weeks
    let obs_times = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 14.0,
    ];

    let clean_fluxes =
        svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &params, &obs_times);

    let scale_factor = 200.0; // Peak flux
    let mut lightcurve = LightCurve::new("KN".to_string());

    for (i, &t) in obs_times.iter().enumerate() {
        let flux = clean_fluxes[i] * scale_factor;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let noisy_flux = (flux + noise).max(0.1);

        lightcurve.add_measurement(Photometry::new(
            mjd_offset + t,
            noisy_flux,
            err,
            "r".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

/// Generate synthetic supernova with realistic cadence
fn generate_supernova_lightcurve(
    true_t0: f64,
    mjd_offset: f64,
    rng: &mut StdRng,
    snr: f64,
) -> (LightCurve, f64) {
    // Type Ia-like supernova (Bazin model)
    let params = vec![
        0.0,             // log(a) = 1.0
        0.0,             // b
        true_t0,         // t0
        (3.0_f64).ln(),  // τ_rise = 3 days
        (25.0_f64).ln(), // τ_fall = 25 days
        -3.0,
    ];

    // Supernova observations span longer baseline (weeks to months)
    // Start observations at random offset from true t0 (could miss early rise)
    let start_offset = rng.gen_range(-5.0..15.0); // Random start relative to t0
    let obs_times: Vec<f64> = (0..30).map(|i| start_offset + i as f64 * 1.5).collect();

    let clean_fluxes =
        svi_models::eval_model_batch(svi_models::SviModel::Bazin, &params, &obs_times);

    let scale_factor = 150.0;
    let mut lightcurve = LightCurve::new("SN".to_string());

    for (i, &t) in obs_times.iter().enumerate() {
        let flux = clean_fluxes[i] * scale_factor;
        let err = flux.max(1.0) / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let noisy_flux = (flux + noise).max(0.1);

        lightcurve.add_measurement(Photometry::new(
            mjd_offset + t,
            noisy_flux,
            err,
            "r".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

#[test]
#[ignore] // Run with: cargo test --package mm-core --test validate_t0_constraints -- --ignored --nocapture
fn test_t0_recovery_population() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║  TEMPORAL DISCRIMINATION: t0 Recovery Validation              ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let mut rng = StdRng::seed_from_u64(12345);
    let mjd_offset = 60000.0;
    let n_samples = 50; // Number of synthetic transients per type
    let snr = 20.0;

    println!("Simulation parameters:");
    println!("  Samples per type: {}", n_samples);
    println!("  SNR: {}", snr);
    println!("  MJD offset: {}\n", mjd_offset);

    // ==== KILONOVA POPULATION ====
    println!("═══ KILONOVA POPULATION (Prompt t0=0) ═══\n");

    let mut kn_t0_errors = Vec::new();
    let mut kn_t0_uncertainties = Vec::new();
    let mut kn_fit_times = Vec::new();

    for i in 0..n_samples {
        let true_t0 = 0.0; // Kilonovae at merger time
        let (lc, _) = generate_kilonova_lightcurve(true_t0, mjd_offset, &mut rng, snr);

        let start = std::time::Instant::now();
        match fit_lightcurve(&lc, FitModel::MetzgerKN) {
            Ok(result) => {
                let duration = start.elapsed().as_secs_f64();
                kn_fit_times.push(duration);

                // t0 is returned in MJD, convert back to relative time
                let fitted_t0 = result.t0 - mjd_offset;
                let t0_error = (fitted_t0 - true_t0).abs();

                kn_t0_errors.push(t0_error);
                kn_t0_uncertainties.push(result.t0_err);

                if (i + 1) % 10 == 0 {
                    println!(
                        "  KN {}/{}: t0 error = {:.3}d, uncertainty = {:.3}d, fit time = {:.2}s",
                        i + 1,
                        n_samples,
                        t0_error,
                        result.t0_err,
                        duration
                    );
                }
            }
            Err(e) => {
                println!("  KN {}: Fit failed: {}", i + 1, e);
            }
        }
    }

    // ==== SUPERNOVA POPULATION ====
    println!("\n═══ SUPERNOVA POPULATION (Random t0 in 0-30d) ═══\n");

    let mut sn_t0_errors = Vec::new();
    let mut sn_t0_uncertainties = Vec::new();
    let mut sn_fit_times = Vec::new();

    for i in 0..n_samples {
        // Supernovae occur at random times within observation window
        let true_t0 = rng.gen_range(0.0..30.0);
        let (lc, _) = generate_supernova_lightcurve(true_t0, mjd_offset, &mut rng, snr);

        let start = std::time::Instant::now();
        match fit_lightcurve(&lc, FitModel::Bazin) {
            Ok(result) => {
                let duration = start.elapsed().as_secs_f64();
                sn_fit_times.push(duration);

                let fitted_t0 = result.t0 - mjd_offset;
                let t0_error = (fitted_t0 - true_t0).abs();

                sn_t0_errors.push(t0_error);
                sn_t0_uncertainties.push(result.t0_err);

                if (i + 1) % 10 == 0 {
                    println!(
                        "  SN {}/{}: t0 error = {:.3}d, uncertainty = {:.3}d, fit time = {:.2}s",
                        i + 1,
                        n_samples,
                        t0_error,
                        result.t0_err,
                        duration
                    );
                }
            }
            Err(e) => {
                println!("  SN {}: Fit failed: {}", i + 1, e);
            }
        }
    }

    // ==== STATISTICS ====
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                    RESULTS SUMMARY                             ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Kilonova statistics
    let kn_median_error = median(&kn_t0_errors);
    let kn_mean_error = mean(&kn_t0_errors);
    let kn_median_unc = median(&kn_t0_uncertainties);
    let kn_mean_fit_time = mean(&kn_fit_times);

    println!("KILONOVA (n={}):", kn_t0_errors.len());
    println!("  t0 Recovery Error:");
    println!("    Median: {:.3} days", kn_median_error);
    println!("    Mean: {:.3} days", kn_mean_error);
    println!("    RMS: {:.3} days", rms(&kn_t0_errors));
    println!("  t0 Fit Uncertainty:");
    println!("    Median: {:.3} days", kn_median_unc);
    println!("  Fit Performance:");
    println!("    Mean time: {:.2}s", kn_mean_fit_time);

    // Supernova statistics
    let sn_median_error = median(&sn_t0_errors);
    let sn_mean_error = mean(&sn_t0_errors);
    let sn_median_unc = median(&sn_t0_uncertainties);
    let sn_mean_fit_time = mean(&sn_fit_times);

    println!("\nSUPERNOVA (n={}):", sn_t0_errors.len());
    println!("  t0 Recovery Error:");
    println!("    Median: {:.3} days", sn_median_error);
    println!("    Mean: {:.3} days", sn_mean_error);
    println!("    RMS: {:.3} days", rms(&sn_t0_errors));
    println!("  t0 Fit Uncertainty:");
    println!("    Median: {:.3} days", sn_median_unc);
    println!("  Fit Performance:");
    println!("    Mean time: {:.2}s", sn_mean_fit_time);

    // ==== TEMPORAL DISCRIMINATION ====
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║            TEMPORAL DISCRIMINATION ANALYSIS                    ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // For a GW event at t0=0, can we distinguish KN from SN based on fitted t0?
    // Assume KN must be within [-1s, +1 day] and SN occur randomly
    let kn_in_window = kn_t0_errors
        .iter()
        .filter(|&&err| err < 1.0) // Within 1 day
        .count();
    let kn_efficiency = kn_in_window as f64 / kn_t0_errors.len() as f64;

    println!("Temporal Window: -1s to +1 day (86400s)");
    println!(
        "\nKilon

ova Detection Efficiency:"
    );
    println!(
        "  {} / {} ({:.1}%) have t0 within 1 day of merger",
        kn_in_window,
        kn_t0_errors.len(),
        kn_efficiency * 100.0
    );

    // False positive rate: SN misidentified as prompt
    // For SN with random t0 in [0, 30d], what fraction land in [0, 1d] window?
    let intrinsic_fp_rate = 1.0 / 30.0; // ~3.3% by chance
    println!("\nSupernova Contamination:");
    println!(
        "  Intrinsic rate (geometric): {:.1}% (1 day / 30 day window)",
        intrinsic_fp_rate * 100.0
    );
    println!("  With t0 uncertainty: Expected similar or higher");

    println!("\nConclusion:");
    if kn_median_error < 1.0 {
        println!(
            "  ✅ Kilonova t0 constraints ({:.2}d median error) enable",
            kn_median_error
        );
        println!("     strong temporal discrimination from random SNe");
    } else {
        println!(
            "  ⚠️  Kilonova t0 uncertainty ({:.2}d) may limit",
            kn_median_error
        );
        println!("     temporal discrimination effectiveness");
    }

    if sn_median_unc > 2.0 {
        println!(
            "  ✅ Supernova t0 uncertainty ({:.2}d) is large enough",
            sn_median_unc
        );
        println!("     that random times are distinguishable from prompt KN");
    }

    // ==== SAVE DATA ====
    println!("\n═══ Saving Results ═══\n");

    let mut output = File::create("/tmp/t0_validation.dat").expect("Failed to create file");
    writeln!(output, "# transient_type t0_error t0_uncertainty").unwrap();
    for (err, unc) in kn_t0_errors.iter().zip(kn_t0_uncertainties.iter()) {
        writeln!(output, "kilonova {:.6} {:.6}", err, unc).unwrap();
    }
    for (err, unc) in sn_t0_errors.iter().zip(sn_t0_uncertainties.iter()) {
        writeln!(output, "supernova {:.6} {:.6}", err, unc).unwrap();
    }
    println!("  Data saved to: /tmp/t0_validation.dat");

    // Summary statistics file
    let mut summary =
        File::create("/tmp/t0_validation_summary.txt").expect("Failed to create file");
    writeln!(summary, "t0 Recovery Validation Summary").unwrap();
    writeln!(summary, "{}", "=".repeat(60)).unwrap();
    writeln!(summary, "\nKilonova (n={}):", kn_t0_errors.len()).unwrap();
    writeln!(summary, "  Median error: {:.3} days", kn_median_error).unwrap();
    writeln!(summary, "  Median uncertainty: {:.3} days", kn_median_unc).unwrap();
    writeln!(
        summary,
        "  Detection efficiency (within 1d): {:.1}%",
        kn_efficiency * 100.0
    )
    .unwrap();
    writeln!(summary, "\nSupernova (n={}):", sn_t0_errors.len()).unwrap();
    writeln!(summary, "  Median error: {:.3} days", sn_median_error).unwrap();
    writeln!(summary, "  Median uncertainty: {:.3} days", sn_median_unc).unwrap();
    writeln!(
        summary,
        "  Intrinsic contamination rate: {:.1}%",
        intrinsic_fp_rate * 100.0
    )
    .unwrap();
    println!("  Summary saved to: /tmp/t0_validation_summary.txt\n");

    // Assertions
    assert!(
        kn_median_error < 2.0,
        "Kilonova t0 recovery too poor: {:.2} days",
        kn_median_error
    );
    assert!(
        kn_efficiency > 0.6,
        "Kilonova detection efficiency too low: {:.1}%",
        kn_efficiency * 100.0
    );
}

// Helper functions
fn median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

fn mean(data: &[f64]) -> f64 {
    data.iter().sum::<f64>() / data.len() as f64
}

fn rms(data: &[f64]) -> f64 {
    (data.iter().map(|x| x * x).sum::<f64>() / data.len() as f64).sqrt()
}
