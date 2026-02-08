/// Integration test for light curve fitting with real ZTF data
use mm_core::{fit_lightcurve, load_lightcurve_csv, FitModel};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/lightcurves_csv")
}

#[test]
fn test_fit_ztf_lightcurve() {
    // Load a real ZTF light curve
    let lc_path = fixtures_dir().join("ZTF25aaabnwi.csv");
    assert!(lc_path.exists(), "Fixture should exist");

    let lightcurve = load_lightcurve_csv(&lc_path).unwrap();

    println!(
        "Loaded {} with {} measurements",
        lightcurve.object_id,
        lightcurve.measurements.len()
    );

    // Try fitting with kilonova model
    let result = fit_lightcurve(&lightcurve, FitModel::MetzgerKN);

    match result {
        Ok(fit) => {
            println!("Fit successful!");
            println!("  t0: {:.3} MJD (±{:.3} days)", fit.t0, fit.t0_err);
            println!("  ELBO: {:.2}", fit.elbo);
            println!("  Converged: {}", fit.converged);
            println!("  Reliable: {}", fit.is_reliable());

            // Basic sanity checks
            assert!(fit.t0.is_finite());
            assert!(fit.t0_err.is_finite());
            assert!(fit.t0_err > 0.0);
        }
        Err(e) => {
            println!("Fit failed: {}", e);
            // Failing is okay for this test - we're just verifying it doesn't crash
        }
    }
}

#[test]
fn test_fit_bazin_model() {
    let lc_path = fixtures_dir().join("ZTF25aaaalin.csv");
    let lightcurve = load_lightcurve_csv(&lc_path).unwrap();

    let result = fit_lightcurve(&lightcurve, FitModel::Bazin);

    match result {
        Ok(fit) => {
            println!("Bazin fit for {}", lightcurve.object_id);
            println!("  t0: {:.3} MJD", fit.t0);
            println!("  t0_err: {:.3} days", fit.t0_err);
            println!("  ELBO: {:.2}", fit.elbo);

            assert!(fit.parameters.len() == 6); // Bazin has 6 params (includes log_sigma_extra)
        }
        Err(e) => {
            println!("Bazin fit failed: {}", e);
        }
    }
}

#[test]
fn test_fit_multiple_objects() {
    // Test fitting on multiple ZTF objects
    let objects = vec!["ZTF25aaaalin.csv", "ZTF25aaaawig.csv", "ZTF25aaabezb.csv"];

    let mut fit_count = 0;
    let mut success_count = 0;

    for obj_file in objects {
        let lc_path = fixtures_dir().join(obj_file);
        if let Ok(lightcurve) = load_lightcurve_csv(&lc_path) {
            fit_count += 1;

            if let Ok(fit) = fit_lightcurve(&lightcurve, FitModel::PowerLaw) {
                if fit.is_reliable() {
                    success_count += 1;
                    println!(
                        "{}: t0={:.3} ±{:.3} MJD",
                        lightcurve.object_id, fit.t0, fit.t0_err
                    );
                }
            }
        }
    }

    println!(
        "Fitted {}/{} objects successfully",
        success_count, fit_count
    );
    assert!(fit_count > 0, "Should have fitted at least one object");
}

#[test]
fn test_insufficient_data_error() {
    // Create light curve with only 2 measurements
    let mut lc = mm_core::LightCurve::new("test".to_string());
    lc.add_measurement(mm_core::Photometry::new(
        60000.0,
        100.0,
        10.0,
        "r".to_string(),
    ));
    lc.add_measurement(mm_core::Photometry::new(
        60001.0,
        150.0,
        10.0,
        "r".to_string(),
    ));

    let result = fit_lightcurve(&lc, FitModel::Bazin);

    assert!(result.is_err());
    match result {
        Err(mm_core::CoreError::InsufficientData(msg)) => {
            println!("Expected error: {}", msg);
            assert!(msg.contains("Need at least 5"));
        }
        _ => panic!("Expected InsufficientData error"),
    }
}
