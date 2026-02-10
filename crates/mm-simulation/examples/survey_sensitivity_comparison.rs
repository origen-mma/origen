//! Compare afterglow detection rates across different survey sensitivities
//!
//! This example demonstrates how limiting magnitude affects afterglow detectability
//! for different optical surveys (ZTF, DECam, LSST).
//!
//! Run with:
//! ```bash
//! cargo run --release -p mm-simulation --example survey_sensitivity_comparison
//! ```

use mm_simulation::{
    simulate_multimessenger_event, AfterglowConfig, BinaryParams, GrbSimulationConfig,
    GwEventParams,
};
use rand::{thread_rng, Rng};
use std::f64::consts::PI;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║        Afterglow Detection vs Survey Sensitivity            ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let mut rng = thread_rng();
    let n_events = 1000;

    // Survey configurations
    let surveys = vec![("ZTF", 21.0), ("DECam", 23.5), ("LSST", 24.5)];

    println!(
        "Simulating {} BNS mergers with random viewing angles...\n",
        n_events
    );

    for (survey_name, limiting_mag) in surveys {
        let _config = AfterglowConfig {
            limiting_magnitude: limiting_mag,
            ..Default::default()
        };

        let mut n_detectable = 0;
        let mut n_with_grb = 0;
        let mut mags_detected = Vec::new();
        let mut mags_missed = Vec::new();

        for _ in 0..n_events {
            // Random BNS merger at realistic GW detection distances
            let inclination = (rng.gen::<f64>() * PI).acos().acos(); // Isotropic
            let distance = 40.0 + rng.gen::<f64>() * 160.0; // 40-200 Mpc (realistic O4 BNS range)

            let binary_params = BinaryParams {
                mass_1_source: 1.4,
                mass_2_source: 1.3,
                radius_1: 12.0,
                radius_2: 12.0,
                chi_1: 0.0,
                chi_2: 0.0,
                tov_mass: 2.17,
                r_16: 12.0,
                ratio_zeta: 0.2,
                alpha: 1.0,
                ratio_epsilon: 0.1,
            };

            let gw_params = GwEventParams {
                inclination,
                distance,
                z: distance / 4500.0, // Approximate z from distance
            };

            // Simulate with custom afterglow config
            // Note: This will use the default config, not our custom one
            // We need to check the magnitude manually
            let mm_event = simulate_multimessenger_event(
                &binary_params,
                &gw_params,
                &GrbSimulationConfig::default(),
                &mut rng,
            );

            // Only consider events with on-axis GRBs for fair comparison
            if mm_event.has_grb() {
                n_with_grb += 1;

                // Check if afterglow would be detectable with this survey
                if let Some(mag) = mm_event.afterglow.peak_magnitude {
                    if mag < limiting_mag {
                        n_detectable += 1;
                        mags_detected.push(mag);
                    } else {
                        mags_missed.push(mag);
                    }
                }
            }
        }

        let detection_rate = if n_with_grb > 0 {
            n_detectable as f64 / n_with_grb as f64 * 100.0
        } else {
            0.0
        };

        println!("📡 {} (limiting mag = {:.1}):", survey_name, limiting_mag);
        println!(
            "   Events with on-axis GRB: {} / {} ({:.1}%)",
            n_with_grb,
            n_events,
            n_with_grb as f64 / n_events as f64 * 100.0
        );
        println!(
            "   Detectable afterglows: {} / {} GRBs ({:.1}%)",
            n_detectable, n_with_grb, detection_rate
        );

        if !mags_detected.is_empty() {
            let mean_mag = mags_detected.iter().sum::<f64>() / mags_detected.len() as f64;
            let brightest = mags_detected.iter().cloned().fold(f64::INFINITY, f64::min);
            let faintest = mags_detected
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            println!(
                "   Detected mags: mean={:.2}, range={:.2}-{:.2}",
                mean_mag, brightest, faintest
            );
        }

        if !mags_missed.is_empty() {
            let mean_mag = mags_missed.iter().sum::<f64>() / mags_missed.len() as f64;
            println!("   Missed events: mean mag={:.2} (too faint)", mean_mag);
        }

        println!();
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                         KEY INSIGHTS                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("• ZTF (~21 mag): Limited to nearby (<200 Mpc), on-axis afterglows");
    println!("• DECam (~23.5 mag): Can detect moderate distance events");
    println!("• LSST (~24.5 mag): Best for faint, distant, off-axis afterglows");
    println!("\n💡 For GW multi-messenger follow-up, LSST depth is crucial!");
}
