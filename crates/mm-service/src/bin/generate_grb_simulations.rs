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
    _distance: f64,
    _mass1: f64,
    _mass2: f64,
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
    offset: f64,      // Actual offset in degrees
    prob_at_grb: f64, // GW probability at simulated GRB position
    in_50cr: bool,
    in_90cr: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== GRB Simulation Generator ===\n");

    // Configuration
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let grb_xml_dir = "/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices";
    let output_dir = "simulated_grbs";

    // Create output directory
    fs::create_dir_all(output_dir)?;

    info!("Loading LIGO injections...");
    let injections_file = format!("{}/injections.dat", base_path);
    let injections = read_injection_params(&injections_file)?;
    info!("Loaded {} injections\n", injections.len());

    // Load GRB XMLs
    info!("Loading GRB alerts...");
    let grb_xmls: Vec<_> = fs::read_dir(grb_xml_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("xml"))
        .collect();
    info!("Found {} GRB XML files\n", grb_xmls.len());

    // Process all injections
    let mut simulations = Vec::new();
    let mut rng = rand::thread_rng();
    let mut failed_count = 0;

    info!("Generating GRB simulations...");
    for (i, injection) in injections.iter().enumerate() {
        if (i + 1) % 500 == 0 {
            info!("Processed {} / {} injections", i + 1, injections.len());
        }

        // Load skymap
        let skymap_path = format!("{}/allsky/{}.fits", base_path, injection.simulation_id);
        let skymap = match ParsedSkymap::from_fits(&skymap_path) {
            Ok(s) => s,
            Err(_) => {
                failed_count += 1;
                continue;
            }
        };

        // Pick random GRB XML, retrying until we get one without 1.0° error radius
        let grb_alert = loop {
            let grb_idx = rng.gen_range(0..grb_xmls.len());
            let grb_xml_path = grb_xmls[grb_idx].path();
            let grb_xml_content = match fs::read_to_string(&grb_xml_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let alert = match VOEventParser::parse_string(&grb_xml_content) {
                Ok(g) => g,
                Err(_) => continue,
            };

            // Skip 1.0° error radius (suspected default values) and retry
            if (alert.error_radius - 1.0).abs() < 0.01 {
                continue;
            }

            // Valid alert found
            break alert;
        };

        // True position
        let true_ra = injection.longitude.to_degrees();
        let true_dec = injection.latitude.to_degrees();

        // Sample GRB position from error distribution
        // For well-calibrated localizations, we sample uniformly within the error circle
        let (grb_ra, grb_dec) =
            sample_position_in_circle(true_ra, true_dec, grb_alert.error_radius, &mut rng);

        // Calculate offset
        let offset = angular_separation(true_ra, true_dec, grb_ra, grb_dec);

        // Query GW skymap at GRB position
        let grb_pos = SkyPosition::new(grb_ra, grb_dec, 0.1);
        let prob_at_grb = skymap.probability_at_position(&grb_pos);
        let in_50cr = skymap.is_in_credible_region(&grb_pos, 0.5);
        let in_90cr = skymap.is_in_credible_region(&grb_pos, 0.9);

        simulations.push(SimulatedGrb {
            simulation_id: injection.simulation_id,
            true_ra,
            true_dec,
            grb_ra,
            grb_dec,
            error_radius: grb_alert.error_radius,
            instrument: grb_alert.instrument.clone(),
            trigger_id: grb_alert.trigger_id.clone(),
            offset,
            prob_at_grb,
            in_50cr,
            in_90cr,
        });
    }

    info!("\n✅ Generated {} simulated GRBs", simulations.len());
    info!("Failed to process {} injections\n", failed_count);

    // Export simulations
    let output_path = format!("{}/grb_simulations.csv", output_dir);
    export_simulations(&simulations, &output_path)?;
    info!("✅ Exported: {}", output_path);

    // Calculate P-P plot data
    let pp_data_path = format!("{}/grb_pp_plot_data.csv", output_dir);
    export_pp_data(&simulations, &pp_data_path)?;
    info!("✅ Exported P-P data: {}", pp_data_path);

    // Statistics
    info!("\n=== Simulation Statistics ===");
    info!("Total simulations: {}", simulations.len());

    let avg_error =
        simulations.iter().map(|s| s.error_radius).sum::<f64>() / simulations.len() as f64;
    let min_error = simulations
        .iter()
        .map(|s| s.error_radius)
        .fold(f64::INFINITY, f64::min);
    let max_error = simulations
        .iter()
        .map(|s| s.error_radius)
        .fold(f64::NEG_INFINITY, f64::max);

    info!("GRB error radii:");
    info!("  Average: {:.2}°", avg_error);
    info!("  Min: {:.2}°", min_error);
    info!("  Max: {:.2}°", max_error);

    let avg_offset = simulations.iter().map(|s| s.offset).sum::<f64>() / simulations.len() as f64;
    info!("\nGRB offsets from true position:");
    info!("  Average: {:.2}°", avg_offset);

    let in_50cr_count = simulations.iter().filter(|s| s.in_50cr).count();
    let in_90cr_count = simulations.iter().filter(|s| s.in_90cr).count();

    info!("\nGW skymap overlap:");
    info!(
        "  In 50% CR: {} ({:.1}%)",
        in_50cr_count,
        100.0 * in_50cr_count as f64 / simulations.len() as f64
    );
    info!(
        "  In 90% CR: {} ({:.1}%)",
        in_90cr_count,
        100.0 * in_90cr_count as f64 / simulations.len() as f64
    );

    info!("\n✅ Run: python plot_grb_pp.py");

    Ok(())
}

/// Sample a position uniformly within a circle on the sphere
fn sample_position_in_circle(
    center_ra: f64,
    center_dec: f64,
    radius_deg: f64,
    rng: &mut impl Rng,
) -> (f64, f64) {
    // For uniform sampling within a circle:
    // - Sample radius from sqrt(uniform(0, R²))
    // - Sample angle uniformly from [0, 2π)

    let r = radius_deg * rng.gen::<f64>().sqrt(); // sqrt for uniform area distribution
    let theta = rng.gen::<f64>() * 2.0 * std::f64::consts::PI;

    // Offset in Cartesian coordinates (small angle approximation is fine for small offsets)
    let delta_ra = r * theta.cos() / center_dec.to_radians().cos();
    let delta_dec = r * theta.sin();

    let new_ra = (center_ra + delta_ra + 360.0) % 360.0;
    let new_dec = (center_dec + delta_dec).clamp(-90.0, 90.0);

    (new_ra, new_dec)
}

/// Angular separation between two sky positions (degrees)
fn angular_separation(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    use std::f64::consts::PI;

    let ra1_rad = ra1 * PI / 180.0;
    let dec1_rad = dec1 * PI / 180.0;
    let ra2_rad = ra2 * PI / 180.0;
    let dec2_rad = dec2 * PI / 180.0;

    let cos_sep = dec1_rad.sin() * dec2_rad.sin()
        + dec1_rad.cos() * dec2_rad.cos() * (ra1_rad - ra2_rad).cos();

    cos_sep.clamp(-1.0, 1.0).acos() * 180.0 / PI
}

fn export_simulations(simulations: &[SimulatedGrb], path: &str) -> Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "simulation_id,true_ra,true_dec,grb_ra,grb_dec,error_radius,instrument,trigger_id,offset,prob_at_grb,in_50cr,in_90cr"
    )?;

    for sim in simulations {
        writeln!(
            file,
            "{},{:.6},{:.6},{:.6},{:.6},{:.2},{},{},{:.4},{:.10e},{},{}",
            sim.simulation_id,
            sim.true_ra,
            sim.true_dec,
            sim.grb_ra,
            sim.grb_dec,
            sim.error_radius,
            sim.instrument,
            sim.trigger_id,
            sim.offset,
            sim.prob_at_grb,
            sim.in_50cr as u8,
            sim.in_90cr as u8,
        )?;
    }

    Ok(())
}

fn export_pp_data(simulations: &[SimulatedGrb], path: &str) -> Result<()> {
    // For GRB P-P plot, we want to check if the offsets are consistent with the error radii
    // A well-calibrated error circle means true positions are uniformly distributed within it

    // Calculate normalized offsets (offset / error_radius)
    let mut normalized_offsets: Vec<(u32, f64)> = simulations
        .iter()
        .map(|s| (s.simulation_id, s.offset / s.error_radius))
        .collect();

    // Sort by normalized offset
    normalized_offsets.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut file = File::create(path)?;
    writeln!(
        file,
        "simulation_id,offset,error_radius,normalized_offset,rank,expected_uniform,within_error_circle"
    )?;

    for (rank, (sim_id, norm_offset)) in normalized_offsets.iter().enumerate() {
        let expected_uniform = (rank + 1) as f64 / normalized_offsets.len() as f64;
        let within_circle = *norm_offset <= 1.0;

        // Find original simulation
        let sim = simulations
            .iter()
            .find(|s| s.simulation_id == *sim_id)
            .unwrap();

        writeln!(
            file,
            "{},{:.4},{:.2},{:.6},{},{:.6},{}",
            sim_id,
            sim.offset,
            sim.error_radius,
            norm_offset,
            rank + 1,
            expected_uniform,
            within_circle as u8,
        )?;
    }

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
            _distance: parts[4].parse()?,
            _mass1: parts[5].parse()?,
            _mass2: parts[6].parse()?,
        });
    }

    Ok(injections)
}
