//! Characterize background GRB rates for multi-messenger analysis
//!
//! This tool generates background (unassociated) GRBs and computes
//! expected false association rates with gravitational wave events.
//!
//! Usage: cargo run --bin characterize-background-grbs

use anyhow::Result;
use mm_simulation::{
    background_grbs::{
        calculate_chance_coincidences, generate_background_grbs, BackgroundGrbConfig, GrbSatellite,
    },
    expected_chance_coincidences,
};
use rand::thread_rng;
use tracing::{info, Level};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== Background GRB Characterization for O4 ===\n");

    // O4 observing run parameters
    // Start: May 24, 2023 (GPS 1369094418)
    // Expected ~1 year of observing time
    let o4_start_gps = 1369094418.0;
    let o4_duration_days = 365.0;
    let o4_end_gps = o4_start_gps + o4_duration_days * 86400.0;

    info!("O4 observing period:");
    info!("  Start GPS: {:.0}", o4_start_gps);
    info!("  Duration: {:.0} days", o4_duration_days);
    info!("  End GPS: {:.0}\n", o4_end_gps);

    // Expected GW event rates for O4
    let expected_bns_events = 10; // ~10 BNS detections expected
    let expected_all_gw_events = 50; // ~50 total GW events (BNS + BBH)

    info!("Expected GW events in O4:");
    info!("  BNS mergers: ~{}", expected_bns_events);
    info!("  All GW events: ~{}\n", expected_all_gw_events);

    // Multi-messenger search parameters
    let time_window_seconds = 10.0; // ±5 seconds around GW trigger
    let typical_bns_skymap_area = 100.0; // ~100 sq deg (typical BNS)

    info!("Multi-messenger search parameters:");
    info!("  Time window: ±{:.1} seconds", time_window_seconds / 2.0);
    info!(
        "  Typical BNS skymap: {:.0} sq deg\n",
        typical_bns_skymap_area
    );

    // Generate background GRBs for each satellite
    let mut rng = thread_rng();

    println!("\n=== Swift BAT Background ===\n");
    let swift_config = BackgroundGrbConfig::swift_bat();
    info!("Swift BAT parameters:");
    info!("  Rate: {:.0} SGRBs/year", swift_config.rate_per_year);
    info!("  FOV: {:.1}% of sky", swift_config.fov_fraction * 100.0);
    info!(
        "  Localization: {:.2}°",
        swift_config.satellite.localization_error_90()
    );

    let swift_grbs = generate_background_grbs(&swift_config, o4_start_gps, o4_end_gps, &mut rng);
    info!("  Generated {} background GRBs\n", swift_grbs.len());

    // Calculate expected false associations
    let expected_false_swift = expected_chance_coincidences(
        expected_bns_events,
        swift_config.rate_per_year,
        time_window_seconds,
        typical_bns_skymap_area,
    );

    info!("Expected false associations (Swift BAT):");
    info!("  With BNS events: {:.6}", expected_false_swift);
    info!(
        "  Probability per BNS: {:.6}%\n",
        expected_false_swift / expected_bns_events as f64 * 100.0
    );

    println!("\n=== Fermi GBM Background ===\n");
    let fermi_config = BackgroundGrbConfig::fermi_gbm();
    info!("Fermi GBM parameters:");
    info!("  Rate: {:.0} SGRBs/year", fermi_config.rate_per_year);
    info!("  FOV: {:.1}% of sky", fermi_config.fov_fraction * 100.0);
    info!(
        "  Localization: {:.1}°",
        fermi_config.satellite.localization_error_90()
    );

    let fermi_grbs = generate_background_grbs(&fermi_config, o4_start_gps, o4_end_gps, &mut rng);
    info!("  Generated {} background GRBs\n", fermi_grbs.len());

    let expected_false_fermi = expected_chance_coincidences(
        expected_bns_events,
        fermi_config.rate_per_year,
        time_window_seconds,
        typical_bns_skymap_area,
    );

    info!("Expected false associations (Fermi GBM):");
    info!("  With BNS events: {:.6}", expected_false_fermi);
    info!(
        "  Probability per BNS: {:.6}%\n",
        expected_false_fermi / expected_bns_events as f64 * 100.0
    );

    println!("\n=== Combined Swift + Fermi ===\n");
    let combined_config = BackgroundGrbConfig::combined();
    let expected_false_combined = expected_chance_coincidences(
        expected_bns_events,
        combined_config.rate_per_year,
        time_window_seconds,
        typical_bns_skymap_area,
    );

    info!("Expected false associations (Combined):");
    info!("  With BNS events: {:.6}", expected_false_combined);
    info!(
        "  Probability per BNS: {:.6}%",
        expected_false_combined / expected_bns_events as f64 * 100.0
    );

    println!("\n=== Monte Carlo Validation ===\n");
    info!("Simulating chance coincidences with mock GW events...");

    // Create mock GW event times (uniformly distributed in O4)
    let mut gw_times = Vec::new();
    let mut gw_skymap_areas = Vec::new();

    for i in 0..expected_bns_events {
        let frac = (i as f64 + 0.5) / expected_bns_events as f64;
        let gw_time = o4_start_gps + frac * (o4_end_gps - o4_start_gps);
        gw_times.push(gw_time);
        gw_skymap_areas.push(typical_bns_skymap_area);
    }

    // Calculate chance coincidences with Swift BAT
    let swift_stats = calculate_chance_coincidences(
        &gw_times,
        &gw_skymap_areas,
        &swift_grbs,
        time_window_seconds,
    );

    info!("\nSwift BAT Monte Carlo:");
    info!("  Total GW events: {}", swift_stats.total_gw_events);
    info!(
        "  Total background GRBs: {}",
        swift_stats.total_background_grbs
    );
    info!(
        "  Temporal coincidences: {}",
        swift_stats.temporal_coincidences
    );
    info!(
        "  Spatio-temporal coincidences: {}",
        swift_stats.spatial_temporal_coincidences
    );
    info!(
        "  Expected false: {:.6}",
        swift_stats.expected_false_associations
    );
    info!(
        "  Chance rate per GW: {:.6}%",
        swift_stats.chance_rate_per_gw * 100.0
    );

    // Calculate chance coincidences with Fermi GBM
    let fermi_stats = calculate_chance_coincidences(
        &gw_times,
        &gw_skymap_areas,
        &fermi_grbs,
        time_window_seconds,
    );

    info!("\nFermi GBM Monte Carlo:");
    info!("  Total GW events: {}", fermi_stats.total_gw_events);
    info!(
        "  Total background GRBs: {}",
        fermi_stats.total_background_grbs
    );
    info!(
        "  Temporal coincidences: {}",
        fermi_stats.temporal_coincidences
    );
    info!(
        "  Spatio-temporal coincidences: {}",
        fermi_stats.spatial_temporal_coincidences
    );
    info!(
        "  Expected false: {:.6}",
        fermi_stats.expected_false_associations
    );
    info!(
        "  Chance rate per GW: {:.6}%",
        fermi_stats.chance_rate_per_gw * 100.0
    );

    println!("\n=== Summary ===\n");
    info!("For O4 with ~{} BNS detections:", expected_bns_events);
    info!(
        "  • Swift BAT: {:.4} expected false associations",
        expected_false_swift
    );
    info!(
        "  • Fermi GBM: {:.4} expected false associations",
        expected_false_fermi
    );
    info!(
        "  • Combined: {:.4} expected false associations",
        expected_false_combined
    );
    info!("\nConclusion: Chance GW-GRB coincidences are RARE (<0.1% per BNS event)");
    info!("A detected GW+GRB association is likely REAL, not background!");

    Ok(())
}
