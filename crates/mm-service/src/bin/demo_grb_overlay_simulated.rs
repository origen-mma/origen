use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    longitude: f64, // radians
    latitude: f64,  // radians
    distance: f64,
    mass1: f64,
    mass2: f64,
}

#[derive(Debug)]
struct SimulatedGrb {
    simulation_id: u32,
    true_ra: f64,
    true_dec: f64,
    grb_ra: f64,
    grb_dec: f64,
    error_radius: f64,
    instrument: String,
    trigger_id: String,
    offset: f64,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== GRB + GW Skymap Overlay Demo (Simulated Positions) ===\n");

    // Configuration
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let grb_sim_file = "simulated_grbs/grb_simulations.csv";

    // Pick a specific injection to demo
    let sim_id = 1; // Use simulation 1 which has a valid GRB

    info!("Loading LIGO injection {}...", sim_id);
    let injections_file = format!("{}/injections.dat", base_path);
    let injections = read_injection_params(&injections_file)?;
    let injection = &injections[sim_id];

    // Parse LIGO skymap
    let skymap_path = format!("{}/allsky/{}.fits", base_path, sim_id);
    let gw_skymap = ParsedSkymap::from_fits(&skymap_path)?;

    info!("✅ LIGO skymap loaded");
    info!("  NSIDE: {}", gw_skymap.nside);
    info!("  50% CR: {:.1} sq deg", gw_skymap.area_50());
    info!("  90% CR: {:.1} sq deg\n", gw_skymap.area_90());

    // Load simulated GRB for this injection
    info!("Loading simulated GRB for injection {}...", sim_id);
    let grb = load_simulated_grb(grb_sim_file, sim_id)?;

    info!("✅ Simulated GRB loaded");
    info!("  Instrument: {}", grb.instrument);
    info!("  Trigger ID: {}", grb.trigger_id);
    info!(
        "  Simulated position: (RA={:.2}°, Dec={:.2}°)",
        grb.grb_ra, grb.grb_dec
    );
    info!("  Error radius: {:.2}°", grb.error_radius);
    info!("  Offset from true position: {:.2}°\n", grb.offset);

    // Get true injection position
    let true_ra = injection.longitude.to_degrees();
    let true_dec = injection.latitude.to_degrees();

    info!("🎯 True GW injection position:");
    info!("  RA={:.4}°, Dec={:.4}°", true_ra, true_dec);
    info!("  Distance: {:.1} Mpc", injection.distance);
    info!(
        "  Masses: {:.1} + {:.1} M☉\n",
        injection.mass1, injection.mass2
    );

    // Query GW skymap at simulated GRB position
    let grb_pos = SkyPosition::new(grb.grb_ra, grb.grb_dec, 0.1);
    let prob_at_grb = gw_skymap.probability_at_position(&grb_pos);
    let in_50cr = gw_skymap.is_in_credible_region(&grb_pos, 0.5);
    let in_90cr = gw_skymap.is_in_credible_region(&grb_pos, 0.9);

    info!("📊 GW skymap probability at simulated GRB position:");
    info!("  Probability: {:.2e}", prob_at_grb);
    info!("  In 50% CR: {}", in_50cr);
    info!("  In 90% CR: {}\n", in_90cr);

    // Export data for plotting
    export_overlay_data(&gw_skymap, injection, &grb, sim_id)?;

    info!("✅ Data exported to grb_overlay_simulated_demo.csv");
    info!("   Run: python plot_grb_overlay_simulated.py");

    Ok(())
}

fn export_overlay_data(
    skymap: &ParsedSkymap,
    injection: &InjectionParams,
    grb: &SimulatedGrb,
    sim_id: usize,
) -> Result<()> {
    use cdshealpix::nested::center;

    let depth = (skymap.nside as f64).log2() as u8;

    // Export skymap data (sampled)
    let output_path = "grb_overlay_simulated_demo.csv";
    let mut file = File::create(output_path)?;
    writeln!(file, "pixel_idx,ra,dec,probability,in_50cr,in_90cr")?;

    let cr_50_pixels: std::collections::HashSet<usize> = skymap.credible_regions[0]
        .pixel_indices
        .iter()
        .copied()
        .collect();
    let cr_90_pixels: std::collections::HashSet<usize> = skymap.credible_regions[1]
        .pixel_indices
        .iter()
        .copied()
        .collect();

    let sample_rate = if skymap.probabilities.len() > 50000 {
        8
    } else {
        1
    };

    for (idx, &prob) in skymap.probabilities.iter().enumerate() {
        if idx % sample_rate != 0 {
            continue;
        }

        let (lon, lat) = center(depth, idx as u64);
        let ra = lon.to_degrees();
        let dec = lat.to_degrees();

        let in_50 = cr_50_pixels.contains(&idx);
        let in_90 = cr_90_pixels.contains(&idx);

        writeln!(
            file,
            "{},{:.6},{:.6},{:.10e},{},{}",
            idx, ra, dec, prob, in_50 as u8, in_90 as u8
        )?;
    }

    // Export metadata
    let meta_path = "grb_overlay_simulated_demo_meta.txt";
    let mut meta_file = File::create(meta_path)?;
    writeln!(meta_file, "simulation_id: {}", sim_id)?;
    writeln!(meta_file, "gw_ra: {:.6}", injection.longitude.to_degrees())?;
    writeln!(meta_file, "gw_dec: {:.6}", injection.latitude.to_degrees())?;
    writeln!(meta_file, "gw_distance: {:.1}", injection.distance)?;
    writeln!(meta_file, "gw_mass1: {:.1}", injection.mass1)?;
    writeln!(meta_file, "gw_mass2: {:.1}", injection.mass2)?;
    writeln!(
        meta_file,
        "gw_max_prob_ra: {:.6}",
        skymap.max_prob_position.ra
    )?;
    writeln!(
        meta_file,
        "gw_max_prob_dec: {:.6}",
        skymap.max_prob_position.dec
    )?;
    writeln!(meta_file, "grb_ra: {:.6}", grb.grb_ra)?;
    writeln!(meta_file, "grb_dec: {:.6}", grb.grb_dec)?;
    writeln!(meta_file, "grb_error_radius: {:.2}", grb.error_radius)?;
    writeln!(meta_file, "grb_instrument: {}", grb.instrument)?;
    writeln!(meta_file, "grb_trigger_id: {}", grb.trigger_id)?;
    writeln!(meta_file, "grb_offset_from_true: {:.2}", grb.offset)?;

    Ok(())
}

fn load_simulated_grb(path: &str, simulation_id: usize) -> Result<SimulatedGrb> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue; // Skip header
        }

        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();

        if parts.len() < 12 {
            continue;
        }

        let sim_id: u32 = parts[0].parse()?;
        if sim_id == simulation_id as u32 {
            return Ok(SimulatedGrb {
                simulation_id: sim_id,
                true_ra: parts[1].parse()?,
                true_dec: parts[2].parse()?,
                grb_ra: parts[3].parse()?,
                grb_dec: parts[4].parse()?,
                error_radius: parts[5].parse()?,
                instrument: parts[6].to_string(),
                trigger_id: parts[7].to_string(),
                offset: parts[8].parse()?,
            });
        }
    }

    Err(anyhow::anyhow!("Simulation ID {} not found", simulation_id))
}

fn read_injection_params(path: &str) -> Result<Vec<InjectionParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut injections = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue;
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
