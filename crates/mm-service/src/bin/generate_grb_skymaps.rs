use anyhow::Result;
use fitsio::tables::{ColumnDataType, ColumnDescription};
use fitsio::FitsFile;
use rand::seq::SliceRandom;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tracing::info;

#[derive(Debug)]
struct SimulatedGrb {
    simulation_id: u32,
    grb_ra: f64,
    grb_dec: f64,
    error_radius: f64,
    instrument: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== GRB Skymap Generator ===\n");

    // Configuration
    let grb_sim_file = "simulated_grbs/grb_simulations.csv";
    let output_dir = "simulated_grbs/O4HL/bgp/allsky";

    // Create output directory
    fs::create_dir_all(output_dir)?;
    info!("Created directory: {}\n", output_dir);

    // Load simulated GRBs
    info!("Loading simulated GRBs...");
    let grbs = load_simulated_grbs(grb_sim_file)?;
    info!("Loaded {} simulated GRBs\n", grbs.len());

    // Collect valid error radii for sampling
    let valid_error_radii: Vec<f64> = grbs
        .iter()
        .filter(|g| g.error_radius > 0.0)
        .map(|g| g.error_radius)
        .collect();

    info!(
        "Found {} valid error radii for sampling",
        valid_error_radii.len()
    );

    if valid_error_radii.is_empty() {
        return Err(anyhow::anyhow!("No valid error radii found in GRB data"));
    }

    let mut rng = rand::thread_rng();

    // Generate skymaps
    let mut success_count = 0;
    let mut failed_count = 0;

    info!("Generating GRB skymaps...");
    for (i, grb) in grbs.iter().enumerate() {
        if (i + 1) % 500 == 0 {
            info!("Generated {} / {} skymaps", i + 1, grbs.len());
        }

        let skymap_path = format!("{}/{}.fits", output_dir, grb.simulation_id);

        // Sample from distribution of valid error radii for zero-error GRBs
        let effective_error_radius = if grb.error_radius <= 0.0 {
            use rand::seq::SliceRandom;
            *valid_error_radii.choose(&mut rng).unwrap()
        } else {
            grb.error_radius
        };

        let modified_grb = SimulatedGrb {
            simulation_id: grb.simulation_id,
            grb_ra: grb.grb_ra,
            grb_dec: grb.grb_dec,
            error_radius: effective_error_radius,
            instrument: grb.instrument.clone(),
        };

        match generate_grb_skymap(&modified_grb, &skymap_path) {
            Ok(_) => success_count += 1,
            Err(e) => {
                eprintln!(
                    "Failed to generate skymap for simulation {}: {}",
                    grb.simulation_id, e
                );
                failed_count += 1;
            }
        }
    }

    info!("\n✅ Generated {} GRB skymaps", success_count);
    info!("Failed: {}\n", failed_count);

    // Create grb_params.dat file
    let params_path = "simulated_grbs/O4HL/bgp/grb_params.dat";
    create_grb_params_file(&grbs, params_path)?;
    info!("✅ Created: {}", params_path);

    Ok(())
}

fn generate_grb_skymap(grb: &SimulatedGrb, output_path: &str) -> Result<()> {
    use cdshealpix::nested::center;

    // Generate MOC skymap with adaptive resolution
    // Use finer resolution near center, coarser far away
    let max_order = 7; // NSIDE=128
    let min_order = 3; // NSIDE=8

    let sigma_deg = grb.error_radius / 2.146;
    let sigma_rad = sigma_deg.to_radians();
    let center_ra_rad = grb.grb_ra.to_radians();
    let center_dec_rad = grb.grb_dec.to_radians();

    // Collect MOC cells with adaptive resolution
    let mut moc_cells: Vec<(i64, f64)> = Vec::new(); // (UNIQ, PROBDENSITY)

    // For each order level, compute cells
    for order in min_order..=max_order {
        let nside = 2_i64.pow(order as u32);
        let npix = 12 * nside * nside;
        let pixel_area_sr = 4.0 * std::f64::consts::PI / npix as f64; // steradians

        for ipix in 0..npix {
            let (lon, lat) = center(order, ipix as u64);
            let angular_sep = haversine(center_ra_rad, center_dec_rad, lon, lat);

            // 2D Gaussian probability density (per steradian)
            let prob_density = (1.0 / (2.0 * std::f64::consts::PI * sigma_rad.powi(2)))
                * (-0.5 * (angular_sep / sigma_rad).powi(2)).exp();

            // Decide whether to include this cell
            // Use adaptive resolution: finer near center, coarser far away
            let distance_in_sigma = angular_sep / sigma_rad;

            let use_this_order = if order == max_order {
                // Always use finest resolution for core (within 2 sigma)
                distance_in_sigma < 2.0
            } else {
                // Use this order for annulus between current and next order
                let next_order_threshold = 2.0_f64.powf((max_order - order - 1) as f64);
                let current_order_threshold = 2.0_f64.powf((max_order - order) as f64);
                distance_in_sigma >= next_order_threshold
                    && distance_in_sigma < current_order_threshold
            };

            if use_this_order && prob_density * pixel_area_sr > 1e-12 {
                // UNIQ encoding: uniq = 4 * 4^order + ipix
                let uniq = 4 * (4_i64.pow(order as u32)) + ipix;
                moc_cells.push((uniq, prob_density));
            }
        }
    }

    // Write MOC FITS file
    write_moc_fits(output_path, &moc_cells)?;

    Ok(())
}

fn haversine(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let cos_sep = dec1.sin() * dec2.sin() + dec1.cos() * dec2.cos() * (ra1 - ra2).cos();
    cos_sep.clamp(-1.0, 1.0).acos()
}

fn write_moc_fits(path: &str, moc_cells: &[(i64, f64)]) -> Result<()> {
    // Create FITS file
    let mut fptr = FitsFile::create(path).open()?;

    // Create primary HDU (empty)
    let hdu = fptr.primary_hdu()?;
    hdu.write_key(&mut fptr, "BITPIX", 8)?;
    hdu.write_key(&mut fptr, "NAXIS", 0)?;

    // Separate UNIQ and PROBDENSITY
    let uniq: Vec<i64> = moc_cells.iter().map(|(u, _)| *u).collect();
    let probdensity: Vec<f64> = moc_cells.iter().map(|(_, p)| *p).collect();

    // Create binary table extension with MOC format
    let table_description = vec![
        ColumnDescription::new("UNIQ")
            .with_type(ColumnDataType::Long)
            .create()?,
        ColumnDescription::new("PROBDENSITY")
            .with_type(ColumnDataType::Double)
            .create()?,
    ];

    let hdu = fptr.create_table("SKYMAP", &table_description)?;

    // Write MOC-specific headers
    hdu.write_key(&mut fptr, "PIXTYPE", "HEALPIX")?;
    hdu.write_key(&mut fptr, "ORDERING", "NUNIQ")?; // MOC uses NUNIQ ordering
    hdu.write_key(&mut fptr, "COORDSYS", "C")?;
    hdu.write_key(&mut fptr, "MOCORDER", 7)?; // Max order
    hdu.write_key(&mut fptr, "INDXSCHM", "EXPLICIT")?;

    // Write columns
    hdu.write_col(&mut fptr, "UNIQ", &uniq)?;
    hdu.write_col(&mut fptr, "PROBDENSITY", &probdensity)?;

    Ok(())
}

fn load_simulated_grbs(path: &str) -> Result<Vec<SimulatedGrb>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut grbs = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue; // Skip header
        }

        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();

        if parts.len() < 7 {
            continue;
        }

        grbs.push(SimulatedGrb {
            simulation_id: parts[0].parse()?,
            grb_ra: parts[3].parse()?,
            grb_dec: parts[4].parse()?,
            error_radius: parts[5].parse()?,
            instrument: parts[6].to_string(),
        });
    }

    Ok(grbs)
}

fn create_grb_params_file(grbs: &[SimulatedGrb], path: &str) -> Result<()> {
    use rand::seq::SliceRandom;
    use std::io::Write;

    let mut file = File::create(path)?;

    // Write header
    writeln!(file, "simulation_id\tra\tdec\terror_radius\tinstrument")?;

    // Collect valid error radii for sampling
    let valid_error_radii: Vec<f64> = grbs
        .iter()
        .filter(|g| g.error_radius > 0.0)
        .map(|g| g.error_radius)
        .collect();

    let mut rng = rand::thread_rng();

    // Write data for all simulations (sample from distribution for zero error radius)
    for grb in grbs {
        let effective_error_radius = if grb.error_radius <= 0.0 {
            *valid_error_radii.choose(&mut rng).unwrap()
        } else {
            grb.error_radius
        };

        writeln!(
            file,
            "{}\t{:.6}\t{:.6}\t{:.2}\t{}",
            grb.simulation_id, grb.grb_ra, grb.grb_dec, effective_error_radius, grb.instrument,
        )?;
    }

    Ok(())
}
