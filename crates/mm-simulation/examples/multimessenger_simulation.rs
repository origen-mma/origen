//! Complete multi-messenger event simulation example
//!
//! This example demonstrates the integrated simulation pipeline:
//! GW binary parameters → ejecta properties → GRB + kilonova counterparts
//!
//! Run with:
//! ```bash
//! cargo run --example multimessenger_simulation
//! ```

use mm_simulation::{
    simulate_multimessenger_event, BinaryParams, GrbSimulationConfig, GwEventParams,
};
use rand::thread_rng;

fn main() {
    let mut rng = thread_rng();

    println!("{}", "=".repeat(70));
    println!("Multi-Messenger Event Simulation");
    println!("{}", "=".repeat(70));

    // Example 1: GW170817-like BNS merger
    println!("\n📡 Example 1: GW170817-like Binary Neutron Star Merger");
    println!("{}", "-".repeat(70));

    let gw170817_binary = BinaryParams {
        mass_1_source: 1.46,  // M_sun
        mass_2_source: 1.27,  // M_sun
        radius_1: 11.9,       // km
        radius_2: 11.9,       // km
        chi_1: 0.0,           // Dimensionless spin
        chi_2: 0.0,
        tov_mass: 2.17,       // Maximum NS mass (EOS-dependent)
        r_16: 11.9,           // NS radius at 1.6 M_sun
        ratio_zeta: 0.2,      // Wind/disk ejecta ratio
        alpha: 0.0,           // Ejecta mass correction (M_sun)
        ratio_epsilon: 0.1,   // Jet efficiency
    };

    let gw170817_params = GwEventParams {
        inclination: 0.44, // ~25° viewing angle
        distance: 40.0,    // Mpc
        z: 0.01,           // Redshift
    };

    let event1 = simulate_multimessenger_event(
        &gw170817_binary,
        &gw170817_params,
        &GrbSimulationConfig::default(),
        &mut rng,
    );

    print_event_summary(&event1);

    // Example 2: NSBH merger
    println!("\n📡 Example 2: Neutron Star - Black Hole Merger");
    println!("{}", "-".repeat(70));

    let nsbh_binary = BinaryParams {
        mass_1_source: 5.0,  // BH mass (M_sun)
        mass_2_source: 1.4,  // NS mass (M_sun)
        radius_1: 0.0,       // BH has no radius
        radius_2: 12.0,      // NS radius (km)
        chi_1: 0.5,          // Spinning BH
        chi_2: 0.0,
        tov_mass: 2.17,
        r_16: 12.0,
        ratio_zeta: 0.2,
        alpha: 0.0,
        ratio_epsilon: 0.1,
    };

    let nsbh_params = GwEventParams {
        inclination: 0.3, // ~17° viewing angle
        distance: 100.0,  // Mpc
        z: 0.02,
    };

    let event2 = simulate_multimessenger_event(
        &nsbh_binary,
        &nsbh_params,
        &GrbSimulationConfig::default(),
        &mut rng,
    );

    print_event_summary(&event2);

    // Example 3: Batch simulation
    println!("\n📊 Example 3: Batch Simulation (10 BNS events)");
    println!("{}", "-".repeat(70));

    let n_events = 10;
    let mut binary_params = Vec::new();
    let mut gw_params = Vec::new();

    for _ in 0..n_events {
        use rand::Rng;
        binary_params.push(BinaryParams {
            mass_1_source: 1.2 + rng.gen::<f64>() * 0.6,
            mass_2_source: 1.2 + rng.gen::<f64>() * 0.6,
            radius_1: 11.0 + rng.gen::<f64>() * 2.0,
            radius_2: 11.0 + rng.gen::<f64>() * 2.0,
            chi_1: rng.gen::<f64>() * 0.1,
            chi_2: rng.gen::<f64>() * 0.1,
            tov_mass: 2.17,
            r_16: 11.9,
            ratio_zeta: 0.2,
            alpha: 0.0,
            ratio_epsilon: 0.1,
        });

        gw_params.push(GwEventParams {
            inclination: rng.gen::<f64>() * std::f64::consts::PI,
            distance: 50.0 + rng.gen::<f64>() * 150.0,
            z: 0.01 + rng.gen::<f64>() * 0.04,
        });
    }

    let events = mm_simulation::simulate_multimessenger_batch(
        &binary_params,
        &gw_params,
        &GrbSimulationConfig::default(),
        &mut rng,
    );

    // Statistics
    let n_with_grb = events.iter().filter(|e| e.has_grb()).count();
    let mean_ejecta = events.iter().map(|e| e.kilonova_mass()).sum::<f64>() / n_events as f64;

    println!("Total events simulated: {}", n_events);
    println!("Events with visible GRB: {} ({:.1}%)",
        n_with_grb,
        n_with_grb as f64 / n_events as f64 * 100.0
    );
    println!("Mean kilonova ejecta mass: {:.4} M_sun", mean_ejecta);

    println!("\n{}", "=".repeat(70));
    println!("Simulation complete!");
    println!("{}", "=".repeat(70));
}

fn print_event_summary(event: &mm_simulation::MultiMessengerEvent) {
    println!("\n🔬 Binary Properties:");
    println!("  Type: {:?}", event.binary_type);
    println!("  Mass 1: {:.2} M_sun", event.binary_params.mass_1_source);
    println!("  Mass 2: {:.2} M_sun", event.binary_params.mass_2_source);
    println!("  Distance: {:.1} Mpc", event.gw_params.distance);
    println!(
        "  Inclination: {:.1}°",
        event.gw_params.inclination.to_degrees()
    );

    println!("\n💫 Kilonova Properties:");
    println!("  Total ejecta: {:.4} M_sun", event.ejecta.mej_total);
    println!("    - Dynamical: {:.4} M_sun", event.ejecta.mej_dyn);
    println!("    - Wind: {:.4} M_sun", event.ejecta.mej_wind);
    println!("  Ejecta velocity: {:.3}c", event.ejecta.vej_dyn);
    println!("  Disk mass: {:.4} M_sun", event.ejecta.mdisk);

    if let Some(ejet) = event.ejecta.ejet_grb {
        println!("  GRB jet energy: {:.2e} erg", ejet);
    }

    if event.has_grb() {
        let grb = event.grb_properties().unwrap();
        println!("\n✨ Gamma-Ray Burst (VISIBLE):");
        println!("  Jet opening angle: {:.1}°", grb.theta_jet);
        println!("  T90 duration: {:.2} s", grb.t90_obs);
        println!("  Fluence: {:.2e} erg/cm²", grb.fluence);
        println!("  Peak energy: {:.1} keV", grb.e_peak_obs);
    } else {
        println!("\n✨ Gamma-Ray Burst: NOT VISIBLE");
        println!("  (Viewing angle > jet opening angle)");
    }
}
