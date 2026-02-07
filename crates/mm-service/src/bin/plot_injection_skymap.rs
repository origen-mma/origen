use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use mm_correlator::spatial::{
    calculate_spatial_probability_from_skymap,
    is_in_credible_region,
};
use cdshealpix::nested::center;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    longitude: f64,  // radians (RA or lambda?)
    latitude: f64,   // radians (Dec or theta?)
    distance: f64,   // Mpc
    mass1: f64,      // solar masses
    mass2: f64,      // solar masses
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== LIGO Injection Skymap Visualization ===\n");

    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let injections_file = format!("{}/injections.dat", base_path);
    let skymap_dir = format!("{}/allsky", base_path);

    // Read injections
    info!("Reading injection parameters...");
    let injections = read_injection_params(&injections_file)?;
    info!("Loaded {} injections\n", injections.len());

    // Test with first few injections to see the pattern
    let test_ids = vec![0, 1, 2, 3, 4];

    for sim_id in test_ids {
        let injection = &injections[sim_id];
        let skymap_path = format!("{}/{}.fits", skymap_dir, sim_id);

        info!("═══════════════════════════════════════════════════════");
        info!("Simulation ID: {}", sim_id);
        info!("═══════════════════════════════════════════════════════");

        // Parse skymap
        let skymap = match ParsedSkymap::from_fits(&skymap_path) {
            Ok(s) => s,
            Err(e) => {
                info!("⚠️  Failed to parse skymap: {}", e);
                continue;
            }
        };

        info!("📊 Skymap Info:");
        info!("  NSIDE: {}, Total pixels: {}", skymap.nside, skymap.probabilities.len());
        info!("  50% CR: {:.1} sq deg, 90% CR: {:.1} sq deg",
              skymap.area_50(), skymap.area_90());
        info!("  Max prob at: (RA={:.2}°, Dec={:.2}°)\n",
              skymap.max_prob_position.ra, skymap.max_prob_position.dec);

        // Test BOTH coordinate conventions
        test_coordinate_convention(&injection, &skymap, "Standard (as-is)");
        test_coordinate_convention_colatitude(&injection, &skymap, "Colatitude (90-lat)");

        // Export skymap data for plotting
        export_skymap_for_plotting(&skymap, &injection, sim_id)?;

        info!("");
    }

    info!("✅ Data exported to skymap_plots/");
    info!("   Use Python/matplotlib to visualize");

    Ok(())
}

fn test_coordinate_convention(injection: &InjectionParams, skymap: &ParsedSkymap, label: &str) {
    let inj_ra = injection.longitude.to_degrees();
    let inj_dec = injection.latitude.to_degrees();

    info!("🧪 Testing {} convention:", label);
    info!("  Injection RA,Dec: ({:.2}°, {:.2}°)", inj_ra, inj_dec);

    let inj_pos = SkyPosition::new(inj_ra, inj_dec, 0.1);
    let prob = calculate_spatial_probability_from_skymap(&inj_pos, skymap);
    let in_50cr = is_in_credible_region(&inj_pos, skymap, 0.5);
    let in_90cr = is_in_credible_region(&inj_pos, skymap, 0.9);

    let sep = angular_separation(
        inj_ra, inj_dec,
        skymap.max_prob_position.ra, skymap.max_prob_position.dec
    );

    info!("  Probability at injection: {:.6e}", prob);
    info!("  In 50% CR: {}, In 90% CR: {}", in_50cr, in_90cr);
    info!("  Angular sep from max: {:.2}°", sep);
}

fn test_coordinate_convention_colatitude(injection: &InjectionParams, skymap: &ParsedSkymap, label: &str) {
    let inj_ra = injection.longitude.to_degrees();
    // Convert colatitude to declination: dec = 90 - theta
    let inj_dec = 90.0 - injection.latitude.to_degrees();

    info!("🧪 Testing {} convention:", label);
    info!("  Injection RA,Dec: ({:.2}°, {:.2}°)", inj_ra, inj_dec);

    let inj_pos = SkyPosition::new(inj_ra, inj_dec, 0.1);
    let prob = calculate_spatial_probability_from_skymap(&inj_pos, skymap);
    let in_50cr = is_in_credible_region(&inj_pos, skymap, 0.5);
    let in_90cr = is_in_credible_region(&inj_pos, skymap, 0.9);

    let sep = angular_separation(
        inj_ra, inj_dec,
        skymap.max_prob_position.ra, skymap.max_prob_position.dec
    );

    info!("  Probability at injection: {:.6e}", prob);
    info!("  In 50% CR: {}, In 90% CR: {}", in_50cr, in_90cr);
    info!("  Angular sep from max: {:.2}°", sep);
}

fn export_skymap_for_plotting(skymap: &ParsedSkymap, injection: &InjectionParams, sim_id: usize) -> Result<()> {
    // Create output directory
    std::fs::create_dir_all("skymap_plots")?;

    let depth = (skymap.nside as f64).log2() as u8;

    // Export skymap probabilities with coordinates
    let output_path = format!("skymap_plots/skymap_{}.csv", sim_id);
    let mut file = File::create(&output_path)?;
    writeln!(file, "pixel_idx,ra,dec,probability,in_50cr,in_90cr")?;

    // Get 50% and 90% CR pixel sets for fast lookup
    let cr_50_pixels: std::collections::HashSet<usize> =
        skymap.credible_regions[0].pixel_indices.iter().copied().collect();
    let cr_90_pixels: std::collections::HashSet<usize> =
        skymap.credible_regions[1].pixel_indices.iter().copied().collect();

    // Sample every Nth pixel to keep file size manageable
    let sample_rate = if skymap.probabilities.len() > 50000 { 8 } else { 1 };

    for (idx, &prob) in skymap.probabilities.iter().enumerate() {
        if idx % sample_rate != 0 {
            continue;
        }

        let (lon, lat) = center(depth, idx as u64);
        let ra = lon.to_degrees();
        let dec = lat.to_degrees();

        let in_50 = cr_50_pixels.contains(&idx);
        let in_90 = cr_90_pixels.contains(&idx);

        writeln!(file, "{},{:.6},{:.6},{:.10e},{},{}",
                 idx, ra, dec, prob, in_50 as u8, in_90 as u8)?;
    }

    // Export injection and max prob positions
    let meta_path = format!("skymap_plots/skymap_{}_meta.txt", sim_id);
    let mut meta_file = File::create(&meta_path)?;
    writeln!(meta_file, "# Simulation {}", sim_id)?;
    writeln!(meta_file, "# Injection (standard): RA={:.6}, Dec={:.6}",
             injection.longitude.to_degrees(),
             injection.latitude.to_degrees())?;
    writeln!(meta_file, "# Injection (colatitude): RA={:.6}, Dec={:.6}",
             injection.longitude.to_degrees(),
             90.0 - injection.latitude.to_degrees())?;
    writeln!(meta_file, "# Max probability: RA={:.6}, Dec={:.6}",
             skymap.max_prob_position.ra,
             skymap.max_prob_position.dec)?;
    writeln!(meta_file, "# Masses: {:.1} + {:.1} Msun",
             injection.mass1, injection.mass2)?;
    writeln!(meta_file, "# Distance: {:.1} Mpc", injection.distance)?;

    info!("  📁 Exported: {}", output_path);

    Ok(())
}

fn angular_separation(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    use std::f64::consts::PI;

    let ra1_rad = ra1 * PI / 180.0;
    let dec1_rad = dec1 * PI / 180.0;
    let ra2_rad = ra2 * PI / 180.0;
    let dec2_rad = dec2 * PI / 180.0;

    let cos_sep = dec1_rad.sin() * dec2_rad.sin() +
                  dec1_rad.cos() * dec2_rad.cos() * (ra1_rad - ra2_rad).cos();

    cos_sep.acos() * 180.0 / PI
}

fn read_injection_params(path: &str) -> Result<Vec<InjectionParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut injections = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue; // Skip header
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 9 {
            continue;
        }

        injections.push(InjectionParams {
            simulation_id: parts[0].parse()?,
            longitude: parts[1].parse()?,
            latitude: parts[2].parse()?,
            distance: parts[4].parse()?,
            mass1: parts[5].parse()?,
            mass2: parts[6].parse()?,
        });
    }

    Ok(injections)
}
