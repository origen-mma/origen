//! GRB Visibility Analysis
//!
//! This example analyzes gamma-ray burst detection probabilities combining:
//! 1. Jet beaming (viewing angle vs jet opening angle)
//! 2. Earth blocking for LEO satellites
//! 3. Field of view constraints
//! 4. Localization accuracy
//!
//! Run with:
//! ```bash
//! cargo run -p mm-simulation --example grb_visibility_analysis
//! ```

use mm_simulation::{
    add_localization_error, is_grb_detectable, simulate_grb_counterpart, GrbInstrument,
    GrbSimulationConfig, GwEventParams, SatelliteConfig, SkyPosition,
};
use rand::thread_rng;
use std::f64::consts::PI;

fn main() {
    println!("{}", "=".repeat(80));
    println!("GRB Visibility & Detection Analysis");
    println!("{}", "=".repeat(80));

    let mut rng = thread_rng();

    // Analysis 1: Jet Beaming Statistics
    println!("\n📊 Analysis 1: Jet Beaming (Intrinsic Visibility)");
    println!("{}", "-".repeat(80));
    analyze_jet_beaming(&mut rng);

    // Analysis 2: Satellite Detection Rates
    println!("\n🛰️  Analysis 2: Satellite Detection Constraints");
    println!("{}", "-".repeat(80));
    analyze_satellite_detection(&mut rng);

    // Analysis 3: Complete Detection Chain
    println!("\n🔗 Analysis 3: Complete GW → GRB Detection Chain");
    println!("{}", "-".repeat(80));
    analyze_complete_detection_chain(&mut rng);

    // Analysis 4: Localization Accuracy
    println!("\n🎯 Analysis 4: GRB Localization Accuracy");
    println!("{}", "-".repeat(80));
    analyze_localization_accuracy(&mut rng);

    println!("\n{}", "=".repeat(80));
    println!("Analysis complete!");
    println!("{}", "=".repeat(80));
}

/// Analyze jet beaming visibility statistics
fn analyze_jet_beaming(rng: &mut impl rand::Rng) {
    let config = GrbSimulationConfig::default();
    let n_events = 10000;

    println!("Simulating {} BNS mergers with isotropic inclinations...", n_events);
    println!("Mean jet opening angle: {:.1}°", config.jet_angle_mean);

    let mut n_visible = 0;
    let mut jet_angles = Vec::new();

    for _ in 0..n_events {
        // Random inclination (isotropic distribution)
        let inclination = (rng.gen::<f64>() * PI).acos().acos(); // Isotropic on sphere

        let gw_params = GwEventParams {
            inclination,
            distance: 100.0,
            z: 0.02,
        };

        let grb = simulate_grb_counterpart(&gw_params, &config, rng);

        if grb.visible {
            n_visible += 1;
        }
        jet_angles.push(grb.theta_jet_deg);
    }

    let visibility_rate = n_visible as f64 / n_events as f64;
    let mean_jet_angle = jet_angles.iter().sum::<f64>() / jet_angles.len() as f64;

    println!("\nResults:");
    println!("  Total mergers: {}", n_events);
    println!("  Visible GRBs (within jet cone): {}", n_visible);
    println!("  Visibility fraction: {:.2}%", visibility_rate * 100.0);
    println!("  Mean jet angle: {:.1}°", mean_jet_angle);

    // Expected visibility for θ_jet ~ 10°
    let solid_angle_jet = 2.0 * PI * (1.0 - (10f64.to_radians()).cos());
    let expected_fraction = solid_angle_jet / (4.0 * PI);
    println!("\nTheoretical expectation (θ_jet = 10°): {:.2}%", expected_fraction * 100.0);
}

/// Analyze satellite detection constraints
fn analyze_satellite_detection(rng: &mut impl rand::Rng) {
    let n_grbs = 1000;

    println!("Testing {} on-axis GRBs (all within jet cone)...", n_grbs);
    println!("Checking detection by different satellites:\n");

    // Test different satellites
    let satellites = vec![
        ("Fermi GBM", SatelliteConfig::fermi()),
        ("Swift BAT", SatelliteConfig::swift()),
        ("Einstein Probe", SatelliteConfig::einstein_probe()),
    ];

    for (name, config) in satellites {
        let mut n_detected = 0;

        for _ in 0..n_grbs {
            // Random GRB position
            let grb_position = SkyPosition {
                ra: rng.gen::<f64>() * 360.0,
                dec: (rng.gen::<f64>() * 2.0 - 1.0).asin().to_degrees(),
            };

            // Random satellite pointing
            let sat_pointing = SkyPosition {
                ra: rng.gen::<f64>() * 360.0,
                dec: (rng.gen::<f64>() * 2.0 - 1.0).asin().to_degrees(),
            };

            if is_grb_detectable(&grb_position, &sat_pointing, &config, rng) {
                n_detected += 1;
            }
        }

        let detection_rate = n_detected as f64 / n_grbs as f64;

        println!("  {} Detection:", name);
        println!("    Altitude: {:.0} km", config.altitude);
        if let Some(fov) = config.fov_solid_angle {
            println!("    FOV: {:.1} sr ({:.1}% of sky)", fov, fov / (4.0 * PI) * 100.0);
        }
        println!("    Detection rate: {:.1}% ({}/{})",
            detection_rate * 100.0, n_detected, n_grbs);
        println!();
    }
}

/// Analyze complete detection chain: BNS → visible GRB → satellite detection
fn analyze_complete_detection_chain(rng: &mut impl rand::Rng) {
    let n_bns = 10000;
    let grb_config = GrbSimulationConfig::default();
    let fermi_config = SatelliteConfig::fermi();
    let swift_config = SatelliteConfig::swift();

    println!("Simulating complete detection chain for {} BNS mergers...\n", n_bns);

    let mut n_visible_grbs = 0;
    let mut n_fermi_detected = 0;
    let mut n_swift_detected = 0;

    for _ in 0..n_bns {
        // Random inclination
        let inclination = (rng.gen::<f64>() * PI).acos().acos();

        let gw_params = GwEventParams {
            inclination,
            distance: 100.0,
            z: 0.02,
        };

        // 1. Check if GRB is visible (jet beaming)
        let grb = simulate_grb_counterpart(&gw_params, &grb_config, rng);

        if !grb.visible {
            continue;
        }
        n_visible_grbs += 1;

        // 2. Random sky position and satellite pointing
        let grb_position = SkyPosition {
            ra: rng.gen::<f64>() * 360.0,
            dec: (rng.gen::<f64>() * 2.0 - 1.0).asin().to_degrees(),
        };

        let sat_pointing = SkyPosition {
            ra: rng.gen::<f64>() * 360.0,
            dec: (rng.gen::<f64>() * 2.0 - 1.0).asin().to_degrees(),
        };

        // 3. Check satellite detection
        if is_grb_detectable(&grb_position, &sat_pointing, &fermi_config, rng) {
            n_fermi_detected += 1;
        }

        if is_grb_detectable(&grb_position, &sat_pointing, &swift_config, rng) {
            n_swift_detected += 1;
        }
    }

    println!("Results:");
    println!("  BNS mergers simulated: {}", n_bns);
    println!("  Visible GRBs (jet beaming): {} ({:.2}%)",
        n_visible_grbs,
        n_visible_grbs as f64 / n_bns as f64 * 100.0
    );
    println!("  Fermi detections: {} ({:.2}% of all, {:.1}% of visible)",
        n_fermi_detected,
        n_fermi_detected as f64 / n_bns as f64 * 100.0,
        n_fermi_detected as f64 / n_visible_grbs as f64 * 100.0
    );
    println!("  Swift detections: {} ({:.2}% of all, {:.1}% of visible)",
        n_swift_detected,
        n_swift_detected as f64 / n_bns as f64 * 100.0,
        n_swift_detected as f64 / n_visible_grbs as f64 * 100.0
    );

    println!("\nKey Insight:");
    println!("  For every 1000 BNS mergers:");
    println!("    → ~{} produce on-axis GRBs (jet beaming)", n_visible_grbs / 10);
    println!("    → ~{} detected by Fermi", n_fermi_detected / 10);
    println!("    → ~{} detected by Swift", n_swift_detected / 10);
}

/// Analyze localization accuracy for different instruments
fn analyze_localization_accuracy(rng: &mut impl rand::Rng) {
    let n_localizations = 1000;
    let true_ra = 180.0;
    let true_dec = 30.0;

    println!("Testing localization accuracy for {} GRBs at (RA={:.1}°, Dec={:.1}°)\n",
        n_localizations, true_ra, true_dec);

    let instruments = vec![
        ("Fermi GBM", GrbInstrument::FermiGBM),
        ("Swift BAT", GrbInstrument::SwiftBAT),
        ("Einstein Probe WXT", GrbInstrument::EinsteinProbeWXT),
        ("IPN Triangulation", GrbInstrument::IPN),
    ];

    for (name, instrument) in instruments {
        let mut position_errors = Vec::new();
        let mut error_radii = Vec::new();

        for _ in 0..n_localizations {
            let localization = add_localization_error(
                true_ra,
                true_dec,
                instrument,
                rng,
            );

            position_errors.push(localization.position_error());
            error_radii.push(localization.error_radius);
        }

        let mean_position_error = position_errors.iter().sum::<f64>() / n_localizations as f64;
        let mean_error_radius = error_radii.iter().sum::<f64>() / n_localizations as f64;
        let min_error = position_errors.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_error = position_errors.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Calculate 90% error region area
        let ellipse_90 = add_localization_error(true_ra, true_dec, instrument, rng)
            .to_error_ellipse(0.90);
        let area_90 = ellipse_90.area();

        println!("  {}:", name);
        println!("    Mean 1σ error radius: {:.3}°", mean_error_radius);
        println!("    Mean position error: {:.3}°", mean_position_error);
        println!("    Position error range: {:.3}° - {:.3}°", min_error, max_error);
        println!("    90% error region: {:.1} sq deg", area_90);

        // Convert to arcminutes for small angles
        if mean_error_radius < 1.0 {
            println!("    (1σ radius: {:.1} arcmin)", mean_error_radius * 60.0);
        }
        println!();
    }

    println!("Note: Position error is actual angular separation from true position.");
    println!("      Error radius is the reported 1σ uncertainty.");
}
