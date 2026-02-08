//! Calculate joint False Alarm Rates for O4 BNS + NSBH sample
//!
//! This example computes the statistical significance of multi-messenger
//! associations by calculating joint FARs that account for:
//! - GW detection significance
//! - GRB detection rates
//! - Optical transient background rates
//! - Spatial and temporal coincidence probabilities
//!
//! Run with:
//! ```bash
//! cargo run --release -p mm-simulation --example o4_joint_far_analysis \
//!     /Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp
//! ```

use clap::Parser;
use mm_simulation::{
    calculate_joint_far, calculate_pastro, simulate_multimessenger_event, BinaryParams,
    FarAssociation, GrbSimulationConfig, GwEventParams, JointFarConfig,
};
use rand::{rngs::StdRng, SeedableRng};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "o4_joint_far_analysis")]
#[command(about = "Calculate joint FARs for O4 multi-messenger events")]
struct Args {
    /// Path to O4HL bgp directory
    bgp_path: PathBuf,

    /// Maximum number of events to process (0 = all)
    #[arg(long, default_value = "0")]
    max_events: usize,

    /// Random seed for reproducibility
    #[arg(long, default_value = "42")]
    seed: u64,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         O4 Multi-Messenger Joint FAR Analysis               ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Initialize RNG
    let mut rng = StdRng::seed_from_u64(args.seed);

    // Read injections.dat file
    let injections_file = args.bgp_path.join("injections.dat");
    println!("Reading O4 gravitational wave events from: {:?}", injections_file);

    let file = File::open(&injections_file)
        .map_err(|e| anyhow::anyhow!("Failed to open injections.dat: {}", e))?;
    let reader = BufReader::new(file);

    // FAR configuration
    let far_config = JointFarConfig {
        gw_observing_time: 1.0,                    // 1 year O4 run
        grb_rate_per_year: 300.0,                  // ~300 SGRBs/year all sky
        optical_rate_per_sqdeg_per_year: 500.0,   // ZTF-like transient rate
        optical_time_window_days: 14.0,           // 2 week search window
        grb_time_window_seconds: 10.0,            // ±5 seconds
    };

    // Event counters and statistics
    let mut n_events = 0;
    let mut n_mm_associations = 0;  // GW + GRB + optical
    let mut n_gw_grb = 0;            // GW + GRB only
    let mut n_gw_optical = 0;        // GW + optical only

    let mut far_values = Vec::new();
    let mut significance_values = Vec::new();

    // GRB simulation config
    let grb_config = GrbSimulationConfig::default();

    println!("\nProcessing events...\n");

    // Process each line
    for (line_num, line) in reader.lines().enumerate() {
        // Skip header
        if line_num == 0 {
            continue;
        }

        // Check max events limit
        if args.max_events > 0 && n_events >= args.max_events {
            break;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 9 {
            eprintln!("Warning: Line {} has insufficient columns, skipping", line_num + 1);
            continue;
        }

        // Parse parameters from injections.dat format
        let mass1: f64 = parts[5].parse()?;
        let mass2: f64 = parts[6].parse()?;
        let distance: f64 = parts[4].parse()?;
        let inclination: f64 = parts[3].parse()?;
        let spin1z: f64 = parts[7].parse()?;
        let spin2z: f64 = parts[8].parse()?;

        // Skip BBH events
        if mass1 > 3.0 && mass2 > 3.0 {
            continue;
        }

        let binary_params = BinaryParams {
            mass_1_source: mass1,
            mass_2_source: mass2,
            radius_1: 12.0,
            radius_2: 12.0,
            chi_1: spin1z,
            chi_2: spin2z,
            tov_mass: 2.17,
            r_16: 12.0,
            ratio_zeta: 0.2,
            alpha: 1.0,
            ratio_epsilon: 0.1,
        };

        let gw_params = GwEventParams {
            inclination,
            distance,
            z: distance / 4500.0,
        };

        // Simulate multi-messenger event
        let mm_event = simulate_multimessenger_event(
            &binary_params,
            &gw_params,
            &grb_config,
            &mut rng,
        );

        n_events += 1;

        // Calculate GW SNR (simplified - would use real LIGO SNR formula)
        let gw_snr = 8.0 + (1.0 / distance * 100.0);  // Rough approximation

        // Calculate GW FAR based on SNR (simplified)
        // Real calculation would use LIGO pipeline FAR estimates
        let gw_far_per_year = if gw_snr > 12.0 {
            0.01  // High SNR
        } else if gw_snr > 10.0 {
            0.1   // Medium SNR
        } else {
            1.0   // Low SNR
        };

        // Skymap area (simplified - would use real skymap from LIGO)
        // Area scales roughly as distance^2 for fixed SNR
        let skymap_area_90 = (distance / 100.0).powi(2) * 100.0;  // sq deg

        // Only calculate FAR if there's an EM counterpart
        if mm_event.has_grb() || mm_event.has_afterglow() || mm_event.has_kilonova() {
            let has_grb = mm_event.has_grb();
            let has_optical = mm_event.has_afterglow() || mm_event.has_kilonova();

            // Count association types
            if has_grb && has_optical {
                n_mm_associations += 1;
            } else if has_grb {
                n_gw_grb += 1;
            } else if has_optical {
                n_gw_optical += 1;
            }

            // Create FAR association
            let far_assoc = FarAssociation {
                gw_snr,
                gw_far_per_year,
                skymap_area_90,
                has_grb,
                grb_fluence: if has_grb { Some(1e-6) } else { None },
                grb_time_offset: if has_grb { Some(0.5) } else { None },
                has_optical,
                optical_magnitude: mm_event.afterglow.peak_magnitude,
                optical_time_offset: if has_optical { Some(1.0) } else { None },
            };

            // Calculate joint FAR
            let far_result = calculate_joint_far(&far_assoc, &far_config);

            far_values.push(far_result.far_per_year);
            significance_values.push(far_result.significance_sigma);

            // Print details for highly significant events
            if far_result.significance_sigma > 5.0 {
                println!("Event {} - HIGHLY SIGNIFICANT (>5σ):", n_events);
                println!("  Distance: {:.0} Mpc", distance);
                println!("  GW SNR: {:.1}", gw_snr);
                println!("  Skymap area: {:.0} sq deg", skymap_area_90);
                println!("  Has GRB: {}", has_grb);
                println!("  Has optical: {}", has_optical);
                if let Some(mag) = mm_event.afterglow.peak_magnitude {
                    println!("  Optical mag: {:.1}", mag);
                }
                println!("  Joint FAR: {:.2e} per year", far_result.far_per_year);
                println!("  Significance: {:.1} sigma", far_result.significance_sigma);
                println!("  P_astro: {:.1}%", 100.0 * calculate_pastro(far_result.far_per_year, 1.0));
                println!();
            }
        }

        if n_events % 50 == 0 {
            println!("Processed {} events...", n_events);
        }
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                       Summary Statistics                     ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("Total events processed: {}", n_events);
    println!("  GW + GRB + Optical: {} ({:.1}%)", n_mm_associations,
             100.0 * n_mm_associations as f64 / n_events as f64);
    println!("  GW + GRB only:      {} ({:.1}%)", n_gw_grb,
             100.0 * n_gw_grb as f64 / n_events as f64);
    println!("  GW + Optical only:  {} ({:.1}%)", n_gw_optical,
             100.0 * n_gw_optical as f64 / n_events as f64);

    if !far_values.is_empty() {
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                  Joint FAR Distribution                      ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");

        let mut sorted_fars = far_values.clone();
        sorted_fars.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mean_far = far_values.iter().sum::<f64>() / far_values.len() as f64;
        let median_far = sorted_fars[sorted_fars.len() / 2];
        let min_far = sorted_fars[0];
        let max_far = sorted_fars[sorted_fars.len() - 1];

        println!("Joint FAR statistics (per year):");
        println!("  Count:  {}", far_values.len());
        println!("  Mean:   {:.2e}", mean_far);
        println!("  Median: {:.2e}", median_far);
        println!("  Min:    {:.2e}", min_far);
        println!("  Max:    {:.2e}", max_far);

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                Significance Distribution                     ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");

        let mut sorted_sigma = significance_values.clone();
        sorted_sigma.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mean_sigma = significance_values.iter().sum::<f64>() / significance_values.len() as f64;
        let median_sigma = sorted_sigma[sorted_sigma.len() / 2];
        let max_sigma = sorted_sigma[sorted_sigma.len() - 1];

        println!("Significance (sigma):");
        println!("  Mean:   {:.2} σ", mean_sigma);
        println!("  Median: {:.2} σ", median_sigma);
        println!("  Max:    {:.2} σ", max_sigma);

        // Count by significance thresholds
        let n_3sigma = significance_values.iter().filter(|&&s| s > 3.0).count();
        let n_5sigma = significance_values.iter().filter(|&&s| s > 5.0).count();

        println!("\nSignificance thresholds:");
        println!("  > 3σ: {} ({:.1}%)", n_3sigma,
                 100.0 * n_3sigma as f64 / significance_values.len() as f64);
        println!("  > 5σ: {} ({:.1}%)", n_5sigma,
                 100.0 * n_5sigma as f64 / significance_values.len() as f64);

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                    Key Insights                              ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");

        println!("• {} multi-messenger associations detected", far_values.len());
        println!("• Median significance: {:.1}σ", median_sigma);
        println!("• {:.0}% of associations are >3σ significant",
                 100.0 * n_3sigma as f64 / significance_values.len() as f64);
        println!("• {:.0}% of associations are >5σ (discovery level)",
                 100.0 * n_5sigma as f64 / significance_values.len() as f64);
        println!();
        println!("💡 Joint FAR accounts for:");
        println!("   - GW detection significance (network SNR, FAR)");
        println!("   - Spatial localization uncertainty (skymap area)");
        println!("   - EM counterpart background rates (GRB, optical)");
        println!("   - Temporal coincidence windows");
    } else {
        println!("\nNo multi-messenger associations found in this sample.");
    }

    Ok(())
}
