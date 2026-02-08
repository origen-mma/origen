//! Characterize background optical transient rates for multi-messenger analysis
//!
//! This tool generates background optical transients (shock cooling, SNe Ia)
//! and demonstrates rejection based on spatial and temporal coincidence.
//!
//! Usage: cargo run --bin characterize-background-optical

use anyhow::Result;
use mm_simulation::background_optical::{
    calculate_optical_coincidences, generate_background_optical, BackgroundOpticalConfig,
    OpticalTransientType,
};
use rand::thread_rng;
use tracing::{info, Level};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== Background Optical Transient Characterization for O4 ===\n");

    // O4 observing run parameters
    let o4_start_gps = 1369094418.0;
    let o4_duration_days = 365.0;
    let o4_end_gps = o4_start_gps + o4_duration_days * 86400.0;

    info!("O4 observing period:");
    info!("  Start GPS: {:.0}", o4_start_gps);
    info!("  Duration: {:.0} days", o4_duration_days);
    info!("  End GPS: {:.0}\n", o4_end_gps);

    // Expected GW event rates
    let expected_bns_events = 10;
    info!("Expected BNS events in O4: ~{}\n", expected_bns_events);

    // Multi-messenger search parameters
    let time_window_days = 14.0; // Search for 14 days after GW trigger
    let typical_bns_skymap_area = 100.0; // ~100 sq deg (typical BNS)

    info!("Multi-messenger search parameters:");
    info!(
        "  Time window: {:.0} days after GW trigger",
        time_window_days
    );
    info!(
        "  Typical BNS skymap: {:.0} sq deg\n",
        typical_bns_skymap_area
    );

    // Generate background optical transients for ZTF
    let mut rng = thread_rng();

    println!("\n=== ZTF Background Transients ===\n");
    let ztf_config = BackgroundOpticalConfig::ztf();
    info!("ZTF parameters:");
    info!("  Rate: {:.0} transients/night", ztf_config.rate_per_day);
    info!(
        "  Survey coverage: {:.1}% of sky",
        ztf_config.survey_coverage * 100.0
    );
    info!("  Limiting magnitude: {:.1}", ztf_config.limiting_magnitude);
    info!(
        "  Shock cooling fraction: {:.1}%\n",
        ztf_config.shock_cooling_fraction * 100.0
    );

    let ztf_transients =
        generate_background_optical(&ztf_config, o4_start_gps, o4_end_gps, &mut rng);

    let shock_cooling_count = ztf_transients
        .iter()
        .filter(|t| t.transient_type == OpticalTransientType::ShockCooling)
        .count();
    let sne_ia_count = ztf_transients
        .iter()
        .filter(|t| t.transient_type == OpticalTransientType::TypeIaSN)
        .count();

    info!("Generated {} total transients:", ztf_transients.len());
    info!(
        "  Shock cooling: {} ({:.2}%)",
        shock_cooling_count,
        shock_cooling_count as f64 / ztf_transients.len() as f64 * 100.0
    );
    info!(
        "  SNe Ia: {} ({:.2}%)\n",
        sne_ia_count,
        sne_ia_count as f64 / ztf_transients.len() as f64 * 100.0
    );

    // Create mock GW event times (uniformly distributed in O4)
    println!("\n=== Chance Coincidence Analysis ===\n");
    info!(
        "Creating {} mock BNS events uniformly distributed in O4...",
        expected_bns_events
    );

    let mut gw_times = Vec::new();
    let mut gw_skymap_areas = Vec::new();

    for i in 0..expected_bns_events {
        let frac = (i as f64 + 0.5) / expected_bns_events as f64;
        let gw_time = o4_start_gps + frac * (o4_end_gps - o4_start_gps);
        gw_times.push(gw_time);
        gw_skymap_areas.push(typical_bns_skymap_area);
    }

    // Calculate chance coincidences
    let ztf_stats = calculate_optical_coincidences(
        &gw_times,
        &gw_skymap_areas,
        &ztf_transients,
        time_window_days,
    );

    info!("\n=== ZTF Coincidence Statistics ===");
    info!("Total GW events: {}", ztf_stats.total_gw_events);
    info!(
        "Total background transients: {}",
        ztf_stats.total_background_transients
    );
    info!(
        "Temporal coincidences (within {} days): {}",
        time_window_days, ztf_stats.temporal_coincidences
    );
    info!(
        "Spatio-temporal coincidences: {}",
        ztf_stats.spatial_temporal_coincidences
    );
    info!(
        "  └─ Shock cooling: {}",
        ztf_stats.shock_cooling_coincidences
    );
    info!("  └─ SNe Ia: {}", ztf_stats.sne_ia_coincidences);
    info!(
        "Expected false associations (analytical): {:.3}",
        ztf_stats.expected_false_associations
    );
    info!(
        "Chance rate per GW: {:.3}%",
        ztf_stats.chance_rate_per_gw * 100.0
    );

    println!("\n=== Rejection Efficiency ===\n");

    // Calculate rejection based on time window
    let temporal_rejection = 1.0
        - (ztf_stats.temporal_coincidences as f64 / ztf_stats.total_background_transients as f64);
    info!(
        "Temporal rejection (not within {} days of any GW): {:.2}%",
        time_window_days,
        temporal_rejection * 100.0
    );

    // Calculate rejection based on spatial + temporal
    let spatial_temporal_rejection = 1.0
        - (ztf_stats.spatial_temporal_coincidences as f64
            / ztf_stats.temporal_coincidences.max(1) as f64);
    info!(
        "Spatial rejection (temporal coincidences not in GW skymap): {:.2}%",
        spatial_temporal_rejection * 100.0
    );

    // Overall rejection
    let total_rejection = 1.0
        - (ztf_stats.spatial_temporal_coincidences as f64
            / ztf_stats.total_background_transients as f64);
    info!(
        "\nTotal rejection efficiency: {:.4}%",
        total_rejection * 100.0
    );

    println!("\n=== Summary ===\n");
    info!("For O4 with ~{} BNS detections:", expected_bns_events);
    info!(
        "  • ZTF: {:.1} expected false optical associations",
        ztf_stats.expected_false_associations
    );
    info!(
        "  • Chance rate: {:.3}% per BNS event",
        ztf_stats.chance_rate_per_gw * 100.0
    );
    info!("\nBackground rejection:");
    info!(
        "  • Temporal cut (14 days): {:.2}% rejected",
        temporal_rejection * 100.0
    );
    info!(
        "  • Spatial cut (100 sq deg): {:.2}% of temporal coincidences rejected",
        spatial_temporal_rejection * 100.0
    );
    info!(
        "  • Overall: {:.4}% of ALL transients rejected",
        total_rejection * 100.0
    );

    info!("\nConclusion:");
    info!("  Time + spatial cuts are EXTREMELY effective!");
    info!(
        "  False optical associations are rare (~{:.1}% per BNS)",
        ztf_stats.chance_rate_per_gw * 100.0
    );
    info!(
        "  Shock cooling transients: {} false coincidences",
        ztf_stats.shock_cooling_coincidences
    );
    info!(
        "  SNe Ia transients: {} false coincidences",
        ztf_stats.sne_ia_coincidences
    );

    Ok(())
}
