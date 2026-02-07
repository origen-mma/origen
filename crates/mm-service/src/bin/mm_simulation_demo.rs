use anyhow::Result;
use mm_simulation::{SimulationConfig, SimulationRunner};
use std::path::PathBuf;
use tracing::info;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("=== Multi-Messenger Simulation Demo ===\n");

    // Configuration
    let config = SimulationConfig {
        injections_file: PathBuf::from(
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/injections.dat",
        ),
        skymap_dir: PathBuf::from(
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky",
        ),
        grb_xml_dir: PathBuf::from(
            "/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices",
        ),
        num_simulations: 100,     // Run 100 simulations
        time_offset_range: 100.0, // ±100 seconds
        rotate_grb_skymaps: true, // Rotate GRB positions to match GW
    };

    info!("Simulation Configuration:");
    info!("  LIGO injections: {:?}", config.injections_file);
    info!("  LIGO skymaps: {:?}", config.skymap_dir);
    info!("  GRB XMLs: {:?}", config.grb_xml_dir);
    info!("  Number of simulations: {}", config.num_simulations);
    info!(
        "  Time offset range: ±{:.0} seconds",
        config.time_offset_range
    );
    info!("  Rotate GRB skymaps: {}\n", config.rotate_grb_skymaps);

    // Create runner
    let mut runner = SimulationRunner::new(config)?;

    // Run simulations
    info!("Running simulations...\n");
    let results = runner.run()?;

    // Print statistics
    SimulationRunner::print_statistics(&results);

    // Print first few results
    info!("\n=== Sample Results (first 10) ===");
    for (i, result) in results.iter().take(10).enumerate() {
        info!("Simulation {}:", i);
        info!(
            "  GW Position: (RA={:.2}°, Dec={:.2}°)",
            result.gw_ra, result.gw_dec
        );
        info!(
            "  GRB Position: (RA={:.2}°, Dec={:.2}°)",
            result.grb_ra, result.grb_dec
        );
        info!("  Spatial separation: {:.2}°", result.spatial_separation);
        info!("  Temporal offset: {:.1}s", result.temporal_offset);
        info!("  In 50% CR: {}", if result.in_50_cr { "✅" } else { "❌" });
        info!("  In 90% CR: {}", if result.in_90_cr { "✅" } else { "❌" });
        info!(
            "  Correlated: {}",
            if result.correlated { "✅" } else { "❌" }
        );
        info!(
            "  Spatial significance: {:.6e}\n",
            result.spatial_significance
        );
    }

    info!("✅ Simulation complete!");

    Ok(())
}
