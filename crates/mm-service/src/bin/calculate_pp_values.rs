use anyhow::Result;
use mm_core::{ParsedSkymap, SkyPosition};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing::info;

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    longitude: f64,  // radians
    latitude: f64,   // radians
    distance: f64,
    mass1: f64,
    mass2: f64,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== P-P Plot Data Generator ===\n");

    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let injections_file = format!("{}/injections.dat", base_path);
    let skymap_dir = format!("{}/allsky", base_path);

    // Read all injections
    info!("Reading injection parameters...");
    let injections = read_injection_params(&injections_file)?;
    info!("Loaded {} injections\n", injections.len());

    // Process all injections
    let mut results = Vec::new();
    let mut failed_count = 0;

    info!("Processing all injections...");
    for (i, injection) in injections.iter().enumerate() {
        if (i + 1) % 500 == 0 {
            info!("Processed {} / {} injections", i + 1, injections.len());
        }

        let skymap_path = format!("{}/{}.fits", skymap_dir, injection.simulation_id);

        // Parse skymap
        let skymap = match ParsedSkymap::from_fits(&skymap_path) {
            Ok(s) => s,
            Err(_) => {
                failed_count += 1;
                continue;
            }
        };

        // Get injection position
        let inj_ra = injection.longitude.to_degrees();
        let inj_dec = injection.latitude.to_degrees();
        let inj_pos = SkyPosition::new(inj_ra, inj_dec, 0.1);

        // Calculate probability at injection position
        let prob_at_injection = skymap.probability_at_position(&inj_pos);

        // Calculate integrated probability (percentile)
        // This is the fraction of the sky with probability >= prob_at_injection
        let integrated_prob = calculate_integrated_probability(&skymap, prob_at_injection);

        // Check credible region membership
        let in_50cr = skymap.is_in_credible_region(&inj_pos, 0.5);
        let in_90cr = skymap.is_in_credible_region(&inj_pos, 0.9);

        results.push((
            injection.simulation_id,
            prob_at_injection,
            integrated_prob,
            in_50cr,
            in_90cr,
            inj_ra,
            inj_dec,
            injection.distance,
        ));
    }

    info!("\nProcessed {} / {} injections successfully", results.len(), injections.len());
    info!("Failed to parse {} skymaps\n", failed_count);

    // Sort by integrated probability for P-P plot
    results.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    // Export results
    let output_path = "pp_plot_data.csv";
    let mut file = File::create(output_path)?;
    writeln!(file, "simulation_id,prob_at_injection,integrated_prob,in_50cr,in_90cr,ra,dec,distance,rank,expected_uniform")?;

    for (rank, result) in results.iter().enumerate() {
        let expected_uniform = (rank + 1) as f64 / results.len() as f64;
        writeln!(
            file,
            "{},{:.10e},{:.6},{},{},{:.4},{:.4},{:.1},{},{}",
            result.0,  // simulation_id
            result.1,  // prob_at_injection
            result.2,  // integrated_prob
            result.3 as u8,  // in_50cr
            result.4 as u8,  // in_90cr
            result.5,  // ra
            result.6,  // dec
            result.7,  // distance
            rank + 1,
            expected_uniform,
        )?;
    }

    info!("✅ Exported: {}", output_path);

    // Calculate statistics
    let in_50cr_count = results.iter().filter(|r| r.3).count();
    let in_90cr_count = results.iter().filter(|r| r.4).count();
    let total = results.len();

    info!("\n=== Calibration Statistics ===");
    info!("Total successful: {}", total);
    info!("Injections in 50% CR: {} ({:.1}%)", in_50cr_count, 100.0 * in_50cr_count as f64 / total as f64);
    info!("Injections in 90% CR: {} ({:.1}%)", in_90cr_count, 100.0 * in_90cr_count as f64 / total as f64);

    // Expected values for well-calibrated localizations:
    info!("\nExpected for perfect calibration:");
    info!("  50% CR should contain: 50.0%");
    info!("  90% CR should contain: 90.0%");

    // Calculate Kolmogorov-Smirnov statistic
    let mut max_deviation = 0.0;
    for (i, result) in results.iter().enumerate() {
        let expected = (i + 1) as f64 / total as f64;
        let deviation = (result.2 - expected).abs();
        if deviation > max_deviation {
            max_deviation = deviation;
        }
    }
    info!("\nKolmogorov-Smirnov statistic: {:.4}", max_deviation);
    info!("(Lower is better; <0.05 suggests good calibration)");

    Ok(())
}

/// Calculate integrated probability (fraction of sky with prob >= threshold)
fn calculate_integrated_probability(skymap: &ParsedSkymap, threshold: f64) -> f64 {
    let mut integrated = 0.0;

    for &prob in &skymap.probabilities {
        if prob >= threshold {
            integrated += prob;
        }
    }

    integrated
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
