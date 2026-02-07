use anyhow::Result;
use mm_core::ParsedSkymap;
use std::f64::consts::PI;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    longitude: f64, // radians
    latitude: f64,  // radians
}

#[derive(Debug)]
struct GrbParams {
    simulation_id: u32,
    error_radius: f64,
}

#[derive(Debug)]
struct OverlapStats {
    simulation_id: u32,
    gw_90cr_area: f64,
    grb_90cr_area: f64,
    overlap_area: f64,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Batch GW+GRB Overlap Analysis ===\n");

    // Configuration
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let grb_params_file = "simulated_grbs/O4HL/bgp/grb_params.dat";
    let output_file = "overlap_statistics.csv";

    // Load injection parameters
    info!("Loading injection parameters...");
    let injections_file = format!("{}/injections.dat", base_path);
    let injections = read_injection_params(&injections_file)?;
    info!("Loaded {} injections", injections.len());

    // Load GRB parameters
    info!("Loading GRB parameters...");
    let grb_params = read_grb_params(grb_params_file)?;
    info!("Loaded {} GRB parameters\n", grb_params.len());

    // Process all simulations
    let mut stats = Vec::new();
    let mut failed = 0;

    info!("Computing overlaps...");
    for (i, injection) in injections.iter().enumerate() {
        if (i + 1) % 500 == 0 {
            info!("Processed {} / {} simulations", i + 1, injections.len());
        }

        let sim_id = injection.simulation_id;

        // Load GW skymap
        let gw_skymap_path = format!("{}/allsky/{}.fits", base_path, sim_id);
        let gw_skymap = match ParsedSkymap::from_fits(&gw_skymap_path) {
            Ok(s) => s,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        // Load GRB parameters
        let grb = match grb_params.iter().find(|g| g.simulation_id == sim_id) {
            Some(g) => g,
            None => {
                failed += 1;
                continue;
            }
        };

        // Load GRB skymap
        let grb_skymap_path = format!("simulated_grbs/O4HL/bgp/allsky/{}.fits", sim_id);
        let grb_skymap = match ParsedSkymap::from_fits(&grb_skymap_path) {
            Ok(s) => s,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        // Compute areas
        let gw_90cr_area = gw_skymap.area_90();
        let grb_90cr_area = PI * grb.error_radius.powi(2);
        let overlap_area = match compute_overlap(&gw_skymap, &grb_skymap) {
            Ok(a) => a,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        stats.push(OverlapStats {
            simulation_id: sim_id,
            gw_90cr_area,
            grb_90cr_area,
            overlap_area,
        });
    }

    info!("\n✅ Processed {} simulations", stats.len());
    info!("Failed: {}\n", failed);

    // Export statistics
    info!("Exporting statistics to {}...", output_file);
    export_stats(&stats, output_file)?;
    info!("✅ Done!\n");

    // Print summary statistics
    let avg_gw = stats.iter().map(|s| s.gw_90cr_area).sum::<f64>() / stats.len() as f64;
    let avg_grb = stats.iter().map(|s| s.grb_90cr_area).sum::<f64>() / stats.len() as f64;
    let avg_overlap = stats.iter().map(|s| s.overlap_area).sum::<f64>() / stats.len() as f64;

    info!("Summary Statistics:");
    info!("  Average GW 90% CR:    {:.1} sq deg", avg_gw);
    info!("  Average GRB 90% CR:   {:.1} sq deg", avg_grb);
    info!("  Average Overlap:      {:.1} sq deg", avg_overlap);
    info!(
        "  Overlap/GW ratio:     {:.1}%",
        100.0 * avg_overlap / avg_gw
    );
    info!(
        "  Overlap/GRB ratio:    {:.1}%",
        100.0 * avg_overlap / avg_grb
    );

    info!("\n✅ Run: python plot_overlap_histogram.py");

    Ok(())
}

fn compute_overlap(gw_skymap: &ParsedSkymap, grb_skymap: &ParsedSkymap) -> Result<f64> {
    // Resample to common resolution
    let target_nside = gw_skymap.nside.min(grb_skymap.nside);

    let gw_probs = resample_skymap(&gw_skymap.probabilities, gw_skymap.nside, target_nside);
    let grb_probs = resample_skymap(&grb_skymap.probabilities, grb_skymap.nside, target_nside);

    // Multiply probability maps: combined = GW × GRB
    let mut combined_probs: Vec<f64> = gw_probs
        .iter()
        .zip(grb_probs.iter())
        .map(|(gw_p, grb_p)| gw_p * grb_p)
        .collect();

    // Normalize combined map
    let combined_sum: f64 = combined_probs.iter().sum();
    if combined_sum <= 0.0 {
        return Ok(0.0);
    }

    for p in &mut combined_probs {
        *p /= combined_sum;
    }

    // Find 90% credible region
    let mut indexed_probs: Vec<(usize, f64)> = combined_probs
        .iter()
        .enumerate()
        .map(|(i, &p)| (i, p))
        .collect();
    indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut cumulative_prob = 0.0;
    let mut cr_90_pixels = 0;

    for &(_idx, prob) in &indexed_probs {
        cumulative_prob += prob;
        cr_90_pixels += 1;
        if cumulative_prob >= 0.9 {
            break;
        }
    }

    // Calculate area
    let npix = 12 * target_nside * target_nside;
    let pixel_area = 4.0 * PI / (npix as f64);
    let area_sq_deg = (cr_90_pixels as f64) * pixel_area * (180.0 / PI).powi(2);

    Ok(area_sq_deg)
}

fn resample_skymap(probs: &[f64], from_nside: i64, to_nside: i64) -> Vec<f64> {
    if from_nside == to_nside {
        return probs.to_vec();
    }

    let to_npix = (12 * to_nside * to_nside) as usize;

    if from_nside > to_nside {
        // Downsample: sum child pixels
        let ratio = ((from_nside / to_nside).pow(2)) as usize;
        let mut resampled = vec![0.0; to_npix];
        for i in 0..to_npix {
            let start_idx = i * ratio;
            resampled[i] = probs[start_idx..start_idx + ratio].iter().sum();
        }
        resampled
    } else {
        // Upsample: distribute parent probability equally
        let ratio = ((to_nside / from_nside).pow(2)) as usize;
        let mut resampled = vec![0.0; to_npix];
        for (i, &p) in probs.iter().enumerate() {
            let start_idx = i * ratio;
            let child_prob = p / ratio as f64;
            for j in 0..ratio {
                resampled[start_idx + j] = child_prob;
            }
        }
        resampled
    }
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

        if parts.len() < 3 {
            continue;
        }

        injections.push(InjectionParams {
            simulation_id: parts[0].parse()?,
            longitude: parts[1].parse()?,
            latitude: parts[2].parse()?,
        });
    }

    Ok(injections)
}

fn read_grb_params(path: &str) -> Result<Vec<GrbParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut params = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 4 {
            continue;
        }

        params.push(GrbParams {
            simulation_id: parts[0].parse()?,
            error_radius: parts[3].parse()?,
        });
    }

    Ok(params)
}

fn export_stats(stats: &[OverlapStats], path: &str) -> Result<()> {
    let mut file = File::create(path)?;

    writeln!(
        file,
        "simulation_id,gw_90cr_area,grb_90cr_area,overlap_area"
    )?;

    for stat in stats {
        writeln!(
            file,
            "{},{:.6},{:.6},{:.6}",
            stat.simulation_id, stat.gw_90cr_area, stat.grb_90cr_area, stat.overlap_area
        )?;
    }

    Ok(())
}
