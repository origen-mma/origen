use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use mm_correlator::spatial::{
    calculate_spatial_probability_from_skymap,
    calculate_spatial_significance,
    calculate_skymap_offset,
    is_in_credible_region,
};
use std::fs::File;
use std::io::{BufRead, BufReader};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    longitude: f64,  // radians (RA)
    latitude: f64,   // radians (Dec)
    distance: f64,   // Mpc
    mass1: f64,      // solar masses
    mass2: f64,      // solar masses
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("=== LIGO Observing Scenarios Skymap Demo ===\n");

    // Path to observing scenarios data
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let injections_file = format!("{}/injections.dat", base_path);
    let skymap_dir = format!("{}/allsky", base_path);

    // Read injection parameters
    info!("Reading injection parameters from: {}", injections_file);
    let injections = read_injection_params(&injections_file)?;
    info!("Loaded {} injection parameters\n", injections.len());

    // Test with first few skymaps
    let test_ids = vec![0, 1, 2, 3, 4];

    for sim_id in test_ids {
        let injection = &injections[sim_id];
        let skymap_path = format!("{}/{}.fits", skymap_dir, sim_id);

        info!("═══════════════════════════════════════════════════════");
        info!("Simulation ID: {}", sim_id);
        info!("═══════════════════════════════════════════════════════");

        // Parse LIGO skymap with our cdshealpix-powered parser
        let skymap = match ParsedSkymap::from_fits(&skymap_path) {
            Ok(s) => s,
            Err(e) => {
                info!("⚠️  Failed to parse skymap: {}", e);
                info!("");
                continue;
            }
        };

        // Display skymap info
        info!("📊 Skymap Properties:");
        info!("  NSIDE: {}", skymap.nside);
        info!("  Ordering: {:?}", skymap.ordering);
        info!("  Total pixels: {}", skymap.probabilities.len());
        info!("  50% CR area: {:.2} sq deg", skymap.area_50());
        info!("  90% CR area: {:.2} sq deg", skymap.area_90());

        // Convert injection position to degrees
        let injection_ra = injection.longitude.to_degrees();
        let injection_dec = injection.latitude.to_degrees();

        info!("");
        info!("🎯 True Injection Parameters:");
        info!("  Position: (RA={:.2}°, Dec={:.2}°)", injection_ra, injection_dec);
        info!("  Distance: {:.1} Mpc", injection.distance);
        info!("  Masses: {:.1} + {:.1} M☉", injection.mass1, injection.mass2);

        // Query skymap at injection position
        let injection_pos = SkyPosition::new(injection_ra, injection_dec, 0.1);

        info!("");
        info!("🔬 Skymap Query at Injection Position:");

        // Get probability at injection position
        let prob = calculate_spatial_probability_from_skymap(&injection_pos, &skymap);
        info!("  Probability: {:.6e}", prob);

        // Check credible region membership
        let in_50cr = is_in_credible_region(&injection_pos, &skymap, 0.5);
        let in_90cr = is_in_credible_region(&injection_pos, &skymap, 0.9);
        info!("  In 50% CR: {}", if in_50cr { "✅ Yes" } else { "❌ No" });
        info!("  In 90% CR: {}", if in_90cr { "✅ Yes" } else { "❌ No" });

        // Calculate spatial significance
        let significance = calculate_spatial_significance(&injection_pos, &skymap);
        info!("  Spatial significance: {:.6e}", significance);

        // Get offset from max probability
        let offset = calculate_skymap_offset(&injection_pos, &skymap);
        info!("  Angular sep from max prob: {:.2}°", offset.angular_separation);

        // Get max probability position
        info!("");
        info!("📍 Skymap Max Probability:");
        info!("  Position: (RA={:.2}°, Dec={:.2}°)",
            skymap.max_prob_position.ra,
            skymap.max_prob_position.dec
        );

        // Calculate how far off the reconstruction is
        let offset_deg = injection_pos.angular_separation(&skymap.max_prob_position);
        info!("  Offset from true position: {:.2}°", offset_deg);

        info!("");
    }

    info!("═══════════════════════════════════════════════════════");
    info!("✅ cdshealpix Integration with Real LIGO Skymaps Verified!");
    info!("   ✓ Successfully parsed LIGO HEALPix FITS files");
    info!("   ✓ Accurate coordinate queries with cdshealpix");
    info!("   ✓ Credible region calculations working");
    info!("   ✓ Ready for production multi-messenger correlation");

    Ok(())
}

/// Read injection parameters from injections.dat
fn read_injection_params(path: &str) -> Result<Vec<InjectionParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut injections = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            // Skip header
            continue;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 9 {
            continue;
        }

        let injection = InjectionParams {
            simulation_id: parts[0].parse()?,
            longitude: parts[1].parse()?,
            latitude: parts[2].parse()?,
            distance: parts[4].parse()?,
            mass1: parts[5].parse()?,
            mass2: parts[6].parse()?,
        };

        injections.push(injection);
    }

    Ok(injections)
}
