use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use mm_simulation::VOEventParser;
use rand::Rng;
use std::fs;
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

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== GRB + GW Skymap Overlay Demo ===\n");

    // Configuration
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let grb_xml_dir = "/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices";

    // Pick a specific injection to demo (simulation 0)
    let sim_id = 0;

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

    // Pick a random GRB XML
    let grb_xmls: Vec<_> = fs::read_dir(grb_xml_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("xml"))
        .collect();

    let mut rng = rand::thread_rng();
    let grb_idx = rng.gen_range(0..grb_xmls.len());
    let grb_xml_path = grb_xmls[grb_idx].path();

    info!("Loading GRB alert: {:?}", grb_xml_path.file_name());
    let grb_xml_content = fs::read_to_string(&grb_xml_path)?;
    let grb_alert = VOEventParser::parse_string(&grb_xml_content)?;

    info!("✅ GRB alert loaded");
    info!("  Instrument: {}", grb_alert.instrument);
    info!("  Trigger ID: {}", grb_alert.trigger_id);
    info!(
        "  Original position: (RA={:.2}°, Dec={:.2}°)",
        grb_alert.ra, grb_alert.dec
    );
    info!("  Error radius: {:.2}°\n", grb_alert.error_radius);

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

    // For this demo, we'll place the GRB at the true position
    // (In full simulation, we'd rotate the actual GRB skymap)
    let grb_ra = true_ra;
    let grb_dec = true_dec;
    let grb_error_radius = grb_alert.error_radius;

    info!("📡 Simulated GRB position (rotated to true location):");
    info!("  RA={:.4}°, Dec={:.4}°", grb_ra, grb_dec);
    info!("  Error radius: {:.2}°\n", grb_error_radius);

    // Export data for plotting
    export_overlay_data(
        &gw_skymap,
        injection,
        grb_ra,
        grb_dec,
        grb_error_radius,
        &grb_alert.instrument,
        sim_id,
    )?;

    info!("✅ Data exported to grb_overlay_demo.csv");
    info!("   Run: python plot_grb_overlay.py");

    Ok(())
}

fn export_overlay_data(
    skymap: &ParsedSkymap,
    injection: &InjectionParams,
    grb_ra: f64,
    grb_dec: f64,
    grb_error_radius: f64,
    grb_instrument: &str,
    sim_id: usize,
) -> Result<()> {
    use cdshealpix::nested::center;

    let depth = (skymap.nside as f64).log2() as u8;

    // Export skymap data (sampled)
    let output_path = "grb_overlay_demo.csv";
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
    let meta_path = "grb_overlay_demo_meta.txt";
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
    writeln!(meta_file, "grb_ra: {:.6}", grb_ra)?;
    writeln!(meta_file, "grb_dec: {:.6}", grb_dec)?;
    writeln!(meta_file, "grb_error_radius: {:.2}", grb_error_radius)?;
    writeln!(meta_file, "grb_instrument: {}", grb_instrument)?;

    Ok(())
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
