/// Example program to load and analyze ZTF light curves
use mm_core::{load_lightcurves_dir, LightCurve};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_lightcurves_dir>", args[0]);
        eprintln!(
            "Example: {} /Users/mcoughlin/Code/ORIGIN/lightcurves_csv",
            args[0]
        );
        std::process::exit(1);
    }

    let dir = &args[1];
    println!("Loading ZTF light curves from: {}", dir);

    let lightcurves = load_lightcurves_dir(dir)?;
    println!("Loaded {} light curves\n", lightcurves.len());

    // Analyze sample light curves
    for (i, lc) in lightcurves.iter().take(5).enumerate() {
        println!("========================================");
        println!("Light Curve {}: {}", i + 1, lc.object_id);
        println!("========================================");

        println!("Total measurements: {}", lc.measurements.len());

        // Band breakdown
        let g_band = lc.filter_band("g");
        let r_band = lc.filter_band("r");
        let i_band = lc.filter_band("i");
        println!(
            "Filters: g={}, r={}, i={}",
            g_band.len(),
            r_band.len(),
            i_band.len()
        );

        // Time range
        if let Some((min_mjd, max_mjd)) = lc.time_range() {
            let baseline = max_mjd - min_mjd;
            println!(
                "Time range: MJD {:.2} to {:.2} (baseline: {:.2} days)",
                min_mjd, max_mjd, baseline
            );

            // Convert first measurement to GPS time
            if let Some(first) = lc.measurements.first() {
                let gps = first.to_gps_time();
                println!("First detection: MJD {:.2} = GPS {:.2}", first.mjd, gps);
            }
        }

        // Peak flux
        if let Some((peak_flux, peak_phot)) = lc.peak_flux() {
            println!(
                "Peak flux: {:.2} ± {:.2} µJy in {} band (SNR={:.1})",
                peak_flux,
                peak_phot.flux_err,
                peak_phot.filter,
                peak_phot.snr()
            );

            if let Some(mag) = peak_phot.magnitude() {
                println!("Peak magnitude: {:.2}", mag);
            }
        }

        // Average SNR
        let avg_snr: f64 =
            lc.measurements.iter().map(|p| p.snr()).sum::<f64>() / lc.measurements.len() as f64;
        println!("Average SNR: {:.1}", avg_snr);

        println!();
    }

    // Summary statistics
    println!("\n========================================");
    println!("Overall Statistics");
    println!("========================================");
    println!("Total objects: {}", lightcurves.len());

    let total_measurements: usize = lightcurves.iter().map(|lc| lc.measurements.len()).sum();
    println!("Total measurements: {}", total_measurements);

    let avg_measurements = total_measurements as f64 / lightcurves.len() as f64;
    println!("Average measurements per object: {:.1}", avg_measurements);

    // Find most detections
    if let Some(max_lc) = lightcurves.iter().max_by_key(|lc| lc.measurements.len()) {
        println!(
            "Most detections: {} ({} points)",
            max_lc.object_id,
            max_lc.measurements.len()
        );
    }

    Ok(())
}
