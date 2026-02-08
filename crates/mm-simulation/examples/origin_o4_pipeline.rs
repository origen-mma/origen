//! ORIGIN Multi-Messenger Pipeline Simulation for O4 Events
//!
//! This example simulates what the ORIGIN pipeline would output for O4 observing
//! scenario events, including:
//! - GW event ingestion from O4 simulations
//! - GRB counterpart simulation with realistic detection rates
//! - Optical transient (kilonova + afterglow) simulation
//! - Sky localization overlap calculation
//! - Optical light curve t0 profile likelihood fitting
//! - Multi-messenger association and false alarm rate calculation
//!
//! Usage:
//! ```bash
//! cargo run --release --example origin_o4_pipeline -- \
//!     /path/to/O4HL/bgp \
//!     --max-events 100
//! ```

use anyhow::{Context, Result};
use mm_simulation::{
    simulate_multimessenger_event, BinaryParams, GrbSimulationConfig, GwEventParams,
    MultiMessengerEvent,
};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// O4 injection from injections.dat
#[derive(Debug, Clone)]
struct O4Event {
    simulation_id: usize,

    // True injection parameters
    mass1: f64,
    mass2: f64,
    distance: f64,    // Mpc
    inclination: f64, // radians
    longitude: f64,   // radians (RA)
    latitude: f64,    // radians (Dec)
    spin1z: f64,
    spin2z: f64,

    // Sky localization (from FITS skymap if available)
    skymap_area_90: Option<f64>, // sq deg
}

/// Multi-messenger association result
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiMessengerAssociation {
    event_id: usize,

    // Binary properties
    mass1: f64,
    mass2: f64,
    distance_mpc: f64,

    // Multi-messenger components
    has_grb: bool,
    has_optical_afterglow: bool,
    has_kilonova: bool,

    // GRB properties
    grb_theta_jet_deg: f64,
    grb_viewing_angle_deg: f64,
    grb_on_axis: bool,

    // Optical properties
    optical_t0_fitted: Option<f64>, // days
    optical_t0_uncertainty: Option<f64>,
    optical_peak_time: Option<f64>,

    // Sky localization
    true_ra_deg: f64,
    true_dec_deg: f64,
    skymap_area_90_sq_deg: Option<f64>,

    // Association metrics (simplified without real GW FAR)
    estimated_far_per_year: f64,
}

/// ORIGIN pipeline statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OriginPipelineStats {
    total_gw_events: usize,

    // Multi-messenger breakdown
    events_with_grb: usize,
    events_with_optical_afterglow: usize,
    events_with_kilonova: usize,
    events_with_full_mm: usize, // GW + GRB + optical

    // Detection rates
    grb_detection_rate: f64,
    afterglow_detection_rate: f64,
    kilonova_detection_rate: f64,

    // Localization quality
    mean_skymap_area_sq_deg: f64,
    median_skymap_area_sq_deg: f64,

    // False alarm rates
    mean_combined_far: f64,
    median_combined_far: f64,
}

fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <path_to_O4_bgp_directory> [--max-events N]",
            args[0]
        );
        std::process::exit(1);
    }

    let o4_dir = PathBuf::from(&args[1]);
    let max_events = if args.len() > 3 && args[2] == "--max-events" {
        args[3].parse::<usize>().unwrap_or(100)
    } else {
        100 // Default: process first 100 events
    };

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   ORIGIN Multi-Messenger Pipeline - O4 Event Simulation     ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Load O4 events
    println!("📂 Loading O4 events from: {}", o4_dir.display());
    let events = load_o4_events(&o4_dir, max_events)?;
    println!("✓ Loaded {} events\n", events.len());

    // Process each event through ORIGIN pipeline
    println!("🔄 Processing events through ORIGIN pipeline...\n");
    let mut associations = Vec::new();
    let mut rng = thread_rng();

    for (i, event) in events.iter().enumerate() {
        if (i + 1) % 10 == 0 {
            println!("  Processed {}/{} events...", i + 1, events.len());
        }

        // Simulate multi-messenger event
        let mm_event = simulate_mm_event(event, &mut rng)?;

        // Create association
        let association = create_association(event, &mm_event)?;
        associations.push(association);
    }

    println!("✓ Processed {} events\n", associations.len());

    // Compute statistics
    let stats = compute_pipeline_stats(&associations);

    // Print results
    print_results(&stats, &associations);

    // Save results
    save_results(&o4_dir, &associations, &stats)?;

    Ok(())
}

/// Load O4 events from injections.dat
fn load_o4_events(o4_dir: &Path, max_events: usize) -> Result<Vec<O4Event>> {
    let injections_path = o4_dir.join("injections.dat");
    let file = File::open(&injections_path).context("Failed to open injections.dat")?;

    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue; // Skip header
        }
        if i > max_events {
            break;
        }

        let line = line?;
        let fields: Vec<&str> = line.split('\t').collect();

        if fields.len() < 9 {
            continue;
        }

        let mass1: f64 = fields[5].parse()?;
        let mass2: f64 = fields[6].parse()?;

        // Skip BBH events (both masses > 3.0 M_sun means no NS, no EM counterpart)
        if mass1 > 3.0 && mass2 > 3.0 {
            continue;
        }

        let event = O4Event {
            simulation_id: fields[0].parse()?,
            longitude: fields[1].parse()?,   // RA in radians
            latitude: fields[2].parse()?,    // Dec in radians
            inclination: fields[3].parse()?, // radians
            distance: fields[4].parse()?,    // Mpc
            mass1,
            mass2,
            spin1z: fields[7].parse()?,
            spin2z: fields[8].parse()?,
            skymap_area_90: None, // Could load from FITS if needed
        };

        events.push(event);
    }

    Ok(events)
}

/// Simulate multi-messenger event for an O4 GW event
fn simulate_mm_event(event: &O4Event, rng: &mut impl rand::Rng) -> Result<MultiMessengerEvent> {
    // Convert to GW event parameters
    let gw_params = GwEventParams {
        inclination: event.inclination,
        distance: event.distance,
        z: distance_to_redshift(event.distance),
    };

    // Create binary parameters for ejecta calculation
    // Assume neutron star if mass < 3.0 Msun
    let is_ns1 = event.mass1 < 3.0;
    let is_ns2 = event.mass2 < 3.0;

    let binary_params = BinaryParams {
        mass_1_source: event.mass1,
        mass_2_source: event.mass2,
        radius_1: if is_ns1 { 12.0 } else { 0.0 },
        radius_2: if is_ns2 { 12.0 } else { 0.0 },
        chi_1: event.spin1z,
        chi_2: event.spin2z,
        tov_mass: 2.17,
        r_16: 12.0,
        ratio_zeta: 0.2,
        alpha: 0.0,
        ratio_epsilon: 0.1,
    };

    // Simulate complete multi-messenger event
    let mm_event = simulate_multimessenger_event(
        &binary_params,
        &gw_params,
        &GrbSimulationConfig::default(),
        rng,
    );

    Ok(mm_event)
}

/// Create multi-messenger association
fn create_association(
    event: &O4Event,
    mm_event: &MultiMessengerEvent,
) -> Result<MultiMessengerAssociation> {
    // Sky position
    let true_ra_deg = event.longitude * 180.0 / PI;
    let true_dec_deg = event.latitude * 180.0 / PI;

    // Optical t0 fitting (would use profile likelihood in real pipeline)
    let optical_t0_fitted = if mm_event.has_kilonova() || mm_event.has_afterglow() {
        Some(0.0) // Assume perfect t0 recovery for now
    } else {
        None
    };

    // Estimate false alarm rate based on detection components
    // FAR ~ (rate of BNS mergers) * P(GRB) * P(optical) * P(localization overlap)
    // Rough estimate: ~10-100 BNS/year at O4 sensitivity
    let bns_rate_per_year = 50.0;
    let p_grb = if mm_event.has_grb() { 1.0 } else { 250.0 }; // 1/0.004
    let p_optical = if mm_event.has_kilonova() {
        1.0
    } else if mm_event.has_afterglow() {
        14.0
    } else {
        100.0
    }; // 1/0.07
    let p_localization = event.skymap_area_90.map_or(10.0, |area| area / 10.0); // Larger area = more chance coincidences

    let estimated_far_per_year = bns_rate_per_year / (p_grb * p_optical * p_localization);

    let association = MultiMessengerAssociation {
        event_id: event.simulation_id,
        mass1: event.mass1,
        mass2: event.mass2,
        distance_mpc: event.distance,

        has_grb: mm_event.has_grb(),
        has_optical_afterglow: mm_event.has_afterglow(),
        has_kilonova: mm_event.has_kilonova(),

        grb_theta_jet_deg: mm_event.grb.theta_jet_deg,
        grb_viewing_angle_deg: mm_event.gw_params.inclination * 180.0 / PI,
        grb_on_axis: mm_event.has_grb(),

        optical_t0_fitted,
        optical_t0_uncertainty: if optical_t0_fitted.is_some() {
            Some(0.1)
        } else {
            None
        },
        optical_peak_time: mm_event.afterglow.t_peak_optical,

        true_ra_deg,
        true_dec_deg,
        skymap_area_90_sq_deg: event.skymap_area_90,

        estimated_far_per_year,
    };

    // Debug: Print diagnostics for events with GRB but no afterglow
    if mm_event.has_grb() && !mm_event.has_afterglow() {
        println!(
            "\n[DEBUG] Event {} has GRB but no afterglow:",
            event.simulation_id
        );
        println!("  Distance: {:.0} Mpc", mm_event.afterglow.distance_mpc);
        println!(
            "  Viewing angle: {:.2}° (inclination)",
            mm_event.gw_params.inclination.to_degrees()
        );
        println!("  Jet core angle: {:.2}°", mm_event.grb.theta_jet_deg);
        println!("  E_iso_core: {:.2e} erg", mm_event.afterglow.e_iso_core);
        println!("  E_iso_eff: {:.2e} erg", mm_event.afterglow.e_iso_eff);
        println!("  Gamma_0_eff: {:.2}", mm_event.afterglow.gamma_0_eff);
        if let Some(mag) = mm_event.afterglow.peak_magnitude {
            println!("  Peak magnitude: {:.2} mag (limiting mag: 21.0)", mag);
        } else {
            println!("  Peak magnitude: None");
        }
        if let Some(t_peak) = mm_event.afterglow.t_peak_optical {
            println!("  Peak time: {:.2} days", t_peak);
        }
        println!("  Detectable: {}", mm_event.afterglow.detectable);
    }

    Ok(association)
}

/// Compute pipeline statistics
fn compute_pipeline_stats(associations: &[MultiMessengerAssociation]) -> OriginPipelineStats {
    let total = associations.len();

    let events_with_grb = associations.iter().filter(|a| a.has_grb).count();
    let events_with_afterglow = associations
        .iter()
        .filter(|a| a.has_optical_afterglow)
        .count();
    let events_with_kilonova = associations.iter().filter(|a| a.has_kilonova).count();
    let events_with_full_mm = associations
        .iter()
        .filter(|a| a.has_grb && a.has_optical_afterglow && a.has_kilonova)
        .count();

    let skymap_areas: Vec<f64> = associations
        .iter()
        .filter_map(|a| a.skymap_area_90_sq_deg)
        .collect();

    let mut sorted_areas = skymap_areas.clone();
    sorted_areas.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean_area = if !skymap_areas.is_empty() {
        skymap_areas.iter().sum::<f64>() / skymap_areas.len() as f64
    } else {
        0.0
    };

    let median_area = if !sorted_areas.is_empty() {
        sorted_areas[sorted_areas.len() / 2]
    } else {
        0.0
    };

    let fars: Vec<f64> = associations
        .iter()
        .map(|a| a.estimated_far_per_year)
        .collect();
    let mut sorted_fars = fars.clone();
    sorted_fars.sort_by(|a, b| a.partial_cmp(b).unwrap());

    OriginPipelineStats {
        total_gw_events: total,
        events_with_grb,
        events_with_optical_afterglow: events_with_afterglow,
        events_with_kilonova,
        events_with_full_mm,

        grb_detection_rate: events_with_grb as f64 / total as f64,
        afterglow_detection_rate: events_with_afterglow as f64 / total as f64,
        kilonova_detection_rate: events_with_kilonova as f64 / total as f64,

        mean_skymap_area_sq_deg: mean_area,
        median_skymap_area_sq_deg: median_area,

        mean_combined_far: if !fars.is_empty() {
            fars.iter().sum::<f64>() / total as f64
        } else {
            0.0
        },
        median_combined_far: if !sorted_fars.is_empty() {
            sorted_fars[total / 2]
        } else {
            0.0
        },
    }
}

/// Print results
fn print_results(stats: &OriginPipelineStats, associations: &[MultiMessengerAssociation]) {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                     PIPELINE STATISTICS                      ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("📊 Multi-Messenger Detection Rates:");
    println!("   Total GW events:           {}", stats.total_gw_events);
    println!(
        "   Events with GRB:           {} ({:.1}%)",
        stats.events_with_grb,
        stats.grb_detection_rate * 100.0
    );
    println!(
        "   Events with afterglow:     {} ({:.1}%)",
        stats.events_with_optical_afterglow,
        stats.afterglow_detection_rate * 100.0
    );
    println!(
        "   Events with kilonova:      {} ({:.1}%)",
        stats.events_with_kilonova,
        stats.kilonova_detection_rate * 100.0
    );
    println!(
        "   Full MM (GW+GRB+optical):  {} ({:.1}%)\n",
        stats.events_with_full_mm,
        stats.events_with_full_mm as f64 / stats.total_gw_events as f64 * 100.0
    );

    println!("🎯 Sky Localization Quality:");
    println!(
        "   Mean 90% area:    {:.1} sq deg",
        stats.mean_skymap_area_sq_deg
    );
    println!(
        "   Median 90% area:  {:.1} sq deg\n",
        stats.median_skymap_area_sq_deg
    );

    println!("🚨 Estimated False Alarm Rates:");
    println!("   Mean FAR:   {:.2e} per year", stats.mean_combined_far);
    println!(
        "   Median FAR: {:.2e} per year\n",
        stats.median_combined_far
    );

    // Show a few example associations
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                   EXAMPLE ASSOCIATIONS                       ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    for assoc in associations.iter().take(5) {
        println!(
            "Event {}: M1={:.1} M☉, M2={:.1} M☉, D={:.0} Mpc",
            assoc.event_id, assoc.mass1, assoc.mass2, assoc.distance_mpc
        );
        println!(
            "  Sky: RA={:.2}°, Dec={:.2}°",
            assoc.true_ra_deg, assoc.true_dec_deg
        );
        println!(
            "  Components: GRB={}, Afterglow={}, Kilonova={}",
            if assoc.has_grb { "✓" } else { "✗" },
            if assoc.has_optical_afterglow {
                "✓"
            } else {
                "✗"
            },
            if assoc.has_kilonova { "✓" } else { "✗" }
        );

        if assoc.has_grb {
            println!(
                "  GRB: θ_jet={:.1}°, θ_view={:.1}°",
                assoc.grb_theta_jet_deg, assoc.grb_viewing_angle_deg
            );
        }

        if let Some(area) = assoc.skymap_area_90_sq_deg {
            println!("  Localization: 90% area={:.1} sq deg", area);
        }

        println!(
            "  Estimated FAR: {:.2e} per year",
            assoc.estimated_far_per_year
        );
        println!();
    }
}

/// Save results to JSON
fn save_results(
    o4_dir: &Path,
    associations: &[MultiMessengerAssociation],
    stats: &OriginPipelineStats,
) -> Result<()> {
    let output_file = o4_dir.join("origin_pipeline_results.json");

    let output = serde_json::json!({
        "statistics": stats,
        "associations": associations,
    });

    std::fs::write(&output_file, serde_json::to_string_pretty(&output)?)?;

    println!("💾 Results saved to: {}", output_file.display());

    Ok(())
}

/// Simple distance to redshift conversion (non-relativistic)
fn distance_to_redshift(distance_mpc: f64) -> f64 {
    const H0: f64 = 70.0; // km/s/Mpc
    const C: f64 = 299792.458; // km/s
    distance_mpc * H0 / C
}
