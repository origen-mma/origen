use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use std::f64::consts::PI;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    _simulation_id: u32,
    longitude: f64, // radians
    latitude: f64,  // radians
    distance: f64,
    mass1: f64,
    mass2: f64,
}

#[derive(Debug)]
struct GrbParams {
    simulation_id: u32,
    ra: f64,
    dec: f64,
    error_radius: f64,
    instrument: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== GW+GRB Credible Region Overlap Analysis ===\n");

    // Get simulation ID from command line
    let args: Vec<String> = std::env::args().collect();
    let sim_id: usize = if args.len() > 1 {
        args[1].parse()?
    } else {
        1 // Default
    };

    // Configuration
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let grb_params_file = "simulated_grbs/O4HL/bgp/grb_params.dat";

    info!("Analyzing simulation {}...", sim_id);

    // Load LIGO injection parameters
    let injections_file = format!("{}/injections.dat", base_path);
    let injections = read_injection_params(&injections_file)?;
    let injection = &injections[sim_id];

    // Load LIGO skymap
    let gw_skymap_path = format!("{}/allsky/{}.fits", base_path, sim_id);
    let gw_skymap = ParsedSkymap::from_fits(&gw_skymap_path)?;

    // Load GRB parameters
    let grb_params = read_grb_params(grb_params_file)?;
    let grb = grb_params
        .iter()
        .find(|g| g.simulation_id == sim_id as u32)
        .ok_or_else(|| anyhow::anyhow!("GRB not found for simulation {}", sim_id))?;

    info!("✅ Loaded GW skymap and GRB parameters\n");

    // Compute areas
    let gw_90cr_area = gw_skymap.area_90();
    let grb_90cr_area = PI * grb.error_radius.powi(2); // Area of circle

    info!("Credible Region Areas:");
    info!("  GW 90% CR:  {:.1} sq deg", gw_90cr_area);
    info!("  GRB 90% CR: {:.1} sq deg (error circle)\n", grb_90cr_area);

    // Load GRB skymap
    let grb_skymap_path = format!("simulated_grbs/O4HL/bgp/allsky/{}.fits", sim_id);
    let grb_skymap = ParsedSkymap::from_fits(&grb_skymap_path)?;

    // Compute overlap
    let overlap_area = compute_overlap(&gw_skymap, &grb_skymap)?;

    info!("Overlap Statistics:");
    info!("  Overlap area: {:.1} sq deg", overlap_area);
    info!(
        "  Overlap / GW CR:  {:.1}%",
        100.0 * overlap_area / gw_90cr_area
    );
    info!(
        "  Overlap / GRB CR: {:.1}%\n",
        100.0 * overlap_area / grb_90cr_area
    );

    // Check if true position is in credible regions
    let true_ra = injection.longitude.to_degrees();
    let true_dec = injection.latitude.to_degrees();
    let true_pos = SkyPosition::new(true_ra, true_dec, 0.1);

    let in_gw_90cr = gw_skymap.is_in_credible_region(&true_pos, 0.9);
    let in_grb_90cr = angular_separation(true_ra, true_dec, grb.ra, grb.dec) <= grb.error_radius;

    info!("True Position:");
    info!("  RA={:.4}°, Dec={:.4}°", true_ra, true_dec);
    info!("  In GW 90% CR:  {}", in_gw_90cr);
    info!("  In GRB 90% CR: {}", in_grb_90cr);
    info!("  In overlap:    {}\n", in_gw_90cr && in_grb_90cr);

    // Export visualization data
    export_visualization_data(
        &gw_skymap,
        injection,
        grb,
        gw_90cr_area,
        grb_90cr_area,
        overlap_area,
        sim_id,
    )?;

    info!("✅ Data exported to overlap_analysis_{}.csv", sim_id);
    info!("   Run: python plot_overlap.py {}", sim_id);

    Ok(())
}

fn compute_overlap(gw_skymap: &ParsedSkymap, grb_skymap: &ParsedSkymap) -> Result<f64> {
    // Ensure both maps are at the same resolution
    // Resample to the coarser resolution to avoid numerical issues
    let target_nside = gw_skymap.nside.min(grb_skymap.nside);

    info!(
        "GW NSIDE: {}, GRB NSIDE: {}, target NSIDE: {}",
        gw_skymap.nside, grb_skymap.nside, target_nside
    );

    // Resample GW map if needed
    let gw_probs = resample_skymap(&gw_skymap.probabilities, gw_skymap.nside, target_nside);

    // Resample GRB map if needed
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
        #[allow(clippy::needless_range_loop)]
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

fn angular_separation(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let ra1_rad = ra1.to_radians();
    let dec1_rad = dec1.to_radians();
    let ra2_rad = ra2.to_radians();
    let dec2_rad = dec2.to_radians();

    let cos_sep = dec1_rad.sin() * dec2_rad.sin()
        + dec1_rad.cos() * dec2_rad.cos() * (ra1_rad - ra2_rad).cos();

    cos_sep.clamp(-1.0, 1.0).acos().to_degrees()
}

fn export_visualization_data(
    skymap: &ParsedSkymap,
    injection: &InjectionParams,
    grb: &GrbParams,
    gw_90cr_area: f64,
    grb_90cr_area: f64,
    overlap_area: f64,
    sim_id: usize,
) -> Result<()> {
    use cdshealpix::nested::center;

    let depth = (skymap.nside as f64).log2() as u8;

    let output_path = format!("overlap_analysis_{}.csv", sim_id);
    let mut file = File::create(&output_path)?;
    writeln!(
        file,
        "pixel_idx,ra,dec,probability,in_gw_90cr,in_grb_90cr,in_overlap"
    )?;

    let gw_90cr_pixels: std::collections::HashSet<usize> = skymap.credible_regions[1]
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

        let in_gw_90 = gw_90cr_pixels.contains(&idx);
        let in_grb_90 = angular_separation(ra, dec, grb.ra, grb.dec) <= grb.error_radius;
        let in_overlap = in_gw_90 && in_grb_90;

        writeln!(
            file,
            "{},{:.6},{:.6},{:.10e},{},{},{}",
            idx, ra, dec, prob, in_gw_90 as u8, in_grb_90 as u8, in_overlap as u8
        )?;
    }

    // Export metadata
    let meta_path = format!("overlap_analysis_{}_meta.txt", sim_id);
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
    writeln!(meta_file, "gw_90cr_area: {:.1}", gw_90cr_area)?;
    writeln!(meta_file, "grb_ra: {:.6}", grb.ra)?;
    writeln!(meta_file, "grb_dec: {:.6}", grb.dec)?;
    writeln!(meta_file, "grb_error_radius: {:.2}", grb.error_radius)?;
    writeln!(meta_file, "grb_instrument: {}", grb.instrument)?;
    writeln!(meta_file, "grb_90cr_area: {:.1}", grb_90cr_area)?;
    writeln!(meta_file, "overlap_area: {:.1}", overlap_area)?;

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
            _simulation_id: parts[0].parse()?,
            longitude: parts[1].parse()?,
            latitude: parts[2].parse()?,
            distance: parts[4].parse()?,
            mass1: parts[5].parse()?,
            mass2: parts[6].parse()?,
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

        if parts.len() < 5 {
            continue;
        }

        params.push(GrbParams {
            simulation_id: parts[0].parse()?,
            ra: parts[1].parse()?,
            dec: parts[2].parse()?,
            error_radius: parts[3].parse()?,
            instrument: parts[4].to_string(),
        });
    }

    Ok(params)
}
