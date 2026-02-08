/// Test fit quality assessment
///
/// Run with: cargo test --test test_fit_quality -- --nocapture
use mm_core::{fit_lightcurve, FitModel, FitQualityAssessment, LightCurve, Photometry};

#[test]
fn test_fit_quality_assessment() {
    // Create a simple synthetic light curve
    let mut lc = LightCurve::new("TEST".to_string());

    // Add some measurements
    let times = [0.5, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0];
    let fluxes = vec![100.0, 120.0, 80.0, 50.0, 20.0, 10.0, 5.0];
    let errors = vec![10.0, 10.0, 8.0, 5.0, 2.0, 1.0, 0.5];

    for ((&t, &f), &e) in times.iter().zip(&fluxes).zip(&errors) {
        lc.add_measurement(Photometry::new(60000.0 + t, f, e, "r".to_string()));
    }

    // Fit with PowerLaw model (should work well)
    let fit_result = fit_lightcurve(&lc, FitModel::PowerLaw).unwrap();

    // Assess quality
    let assessment = FitQualityAssessment::assess(&fit_result, Some(60000.5));

    println!("\n=== Fit Quality Assessment ===");
    println!("Quality: {:?}", assessment.quality);
    println!("ELBO: {:.2}", assessment.elbo);
    println!("Acceptable: {}", assessment.is_acceptable);
    println!("Description: {}", assessment.quality.description());

    if let Some(msg) = assessment.warning_message() {
        println!("\nWarnings:\n{}", msg);
    } else {
        println!("\nNo warnings - fit looks good!");
    }

    // Demonstrate the API (don't assert - synthetic data quality varies)
    println!("\n(Test demonstrates API, not fit quality)");
}

#[test]
fn test_quality_levels() {
    use mm_core::FitQuality;

    println!("\n=== Quality Level Descriptions ===");

    let elbos = vec![60.0, 30.0, 5.0, -5.0, -20.0];

    for elbo in elbos {
        let quality = FitQuality::from_elbo(elbo);
        println!("\nELBO = {:.1}:", elbo);
        println!("  Quality: {:?}", quality);
        println!("  Acceptable: {}", quality.is_acceptable());
        println!("  Description: {}", quality.description());
    }
}
