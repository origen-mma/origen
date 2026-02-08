//! Analyze magnitude distributions for O4 BNS + NSBH sample
//!
//! This example processes O4 gravitational wave events and reports the
//! distribution of expected afterglow and kilonova magnitudes.
//!
//! Run with:
//! ```bash
//! cargo run --release -p mm-simulation --example o4_magnitude_distributions \
//!     /Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp
//! ```

use clap::Parser;
use mm_simulation::{
    simulate_multimessenger_event, BinaryParams, GrbSimulationConfig, GwEventParams,
};
use rand::{rngs::StdRng, SeedableRng};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "o4_magnitude_distributions")]
#[command(about = "Analyze magnitude distributions for O4 sample")]
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

#[derive(Default)]
struct MagnitudeStats {
    magnitudes: Vec<f64>,
}

impl MagnitudeStats {
    fn add(&mut self, mag: f64) {
        self.magnitudes.push(mag);
    }

    fn compute_statistics(&mut self) -> Statistics {
        if self.magnitudes.is_empty() {
            return Statistics::default();
        }

        self.magnitudes.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let count = self.magnitudes.len();
        let mean = self.magnitudes.iter().sum::<f64>() / count as f64;
        let median = self.magnitudes[count / 2];
        let min = self.magnitudes[0];
        let max = self.magnitudes[count - 1];
        let p10 = self.magnitudes[(count as f64 * 0.1) as usize];
        let p90 = self.magnitudes[(count as f64 * 0.9) as usize];

        // Count detectable at different survey depths
        let ztf_detectable = self.magnitudes.iter().filter(|&&m| m < 21.0).count();
        let decam_detectable = self.magnitudes.iter().filter(|&&m| m < 23.5).count();
        let lsst_detectable = self.magnitudes.iter().filter(|&&m| m < 24.5).count();

        Statistics {
            count,
            mean,
            median,
            min,
            max,
            p10,
            p90,
            ztf_detectable,
            decam_detectable,
            lsst_detectable,
        }
    }

    fn compute_histogram(&self, bin_width: f64) -> Vec<(f64, usize)> {
        if self.magnitudes.is_empty() {
            return vec![];
        }

        let min_mag = self
            .magnitudes
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let max_mag = self
            .magnitudes
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        let n_bins = ((max_mag - min_mag) / bin_width).ceil() as usize + 1;
        let mut bins = vec![0usize; n_bins];

        for &mag in &self.magnitudes {
            let bin_idx = ((mag - min_mag) / bin_width).floor() as usize;
            if bin_idx < n_bins {
                bins[bin_idx] += 1;
            }
        }

        bins.into_iter()
            .enumerate()
            .map(|(i, count)| (min_mag + i as f64 * bin_width, count))
            .filter(|(_, count)| *count > 0)
            .collect()
    }
}

#[derive(Default, Debug)]
struct Statistics {
    count: usize,
    mean: f64,
    median: f64,
    min: f64,
    max: f64,
    p10: f64,
    p90: f64,
    ztf_detectable: usize,
    decam_detectable: usize,
    lsst_detectable: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     O4 Multi-Messenger Magnitude Distribution Analysis      ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Initialize RNG
    let mut rng = StdRng::seed_from_u64(args.seed);

    // Read injections.dat file
    let injections_file = args.bgp_path.join("injections.dat");
    println!(
        "Reading O4 gravitational wave events from: {:?}",
        injections_file
    );

    let file = File::open(&injections_file)
        .map_err(|e| anyhow::anyhow!("Failed to open injections.dat: {}", e))?;
    let reader = BufReader::new(file);

    // Statistics collectors
    let mut afterglow_all = MagnitudeStats::default();
    let mut afterglow_onaxis = MagnitudeStats::default(); // Only for on-axis GRBs

    // Distance bins for stratification
    let mut distance_bins: HashMap<String, (MagnitudeStats, MagnitudeStats)> = HashMap::new();
    distance_bins.insert("40-100 Mpc".to_string(), Default::default());
    distance_bins.insert("100-200 Mpc".to_string(), Default::default());
    distance_bins.insert("200-400 Mpc".to_string(), Default::default());
    distance_bins.insert("400-800 Mpc".to_string(), Default::default());

    // Event counters
    let mut n_events = 0;
    let mut n_bns = 0;
    let mut n_nsbh = 0;
    let mut n_grb = 0;
    let mut n_afterglow = 0;
    let mut n_kilonova = 0;

    let mut total_distance = 0.0;

    // GRB simulation config
    let grb_config = GrbSimulationConfig::default();

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
            eprintln!(
                "Warning: Line {} has insufficient columns, skipping",
                line_num + 1
            );
            continue;
        }

        // Parse parameters from injections.dat format
        // simulation_id	longitude	latitude	inclination	distance	mass1	mass2	spin1z	spin2z
        let mass1: f64 = parts[5].parse()?;
        let mass2: f64 = parts[6].parse()?;
        let distance: f64 = parts[4].parse()?;
        let inclination: f64 = parts[3].parse()?;
        let spin1z: f64 = parts[7].parse()?;
        let spin2z: f64 = parts[8].parse()?;

        // Skip BBH events (both masses > 3.0 M_sun means no NS, no EM counterpart)
        if mass1 > 3.0 && mass2 > 3.0 {
            continue;
        }

        // BNS or NSBH?
        let is_bns = mass1 < 3.0 && mass2 < 3.0;
        if is_bns {
            n_bns += 1;
        } else {
            n_nsbh += 1;
        }

        total_distance += distance;

        // Determine distance bin
        let distance_bin = if distance < 100.0 {
            "40-100 Mpc"
        } else if distance < 200.0 {
            "100-200 Mpc"
        } else if distance < 400.0 {
            "200-400 Mpc"
        } else {
            "400-800 Mpc"
        };

        let binary_params = BinaryParams {
            mass_1_source: mass1,
            mass_2_source: mass2,
            radius_1: 12.0, // Typical NS radius
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
            z: distance / 4500.0, // Approximate redshift from distance
        };

        // Simulate multi-messenger event
        let mm_event =
            simulate_multimessenger_event(&binary_params, &gw_params, &grb_config, &mut rng);

        n_events += 1;

        // Collect afterglow magnitudes
        if let Some(mag) = mm_event.afterglow.peak_magnitude {
            afterglow_all.add(mag);

            // Add to distance bin
            if let Some((ag_stats, _)) = distance_bins.get_mut(distance_bin) {
                ag_stats.add(mag);
            }

            // If on-axis GRB, add to on-axis stats
            if mm_event.has_grb() {
                n_grb += 1;
                afterglow_onaxis.add(mag);
            }
        }

        // Count kilonova
        if mm_event.has_kilonova() {
            n_kilonova += 1;
            // TODO: Add kilonova magnitude once implemented
        }

        if mm_event.has_afterglow() {
            n_afterglow += 1;
        }

        // Progress indicator
        if n_events % 1000 == 0 {
            println!("Processed {} events...", n_events);
        }
    }

    let mean_distance = total_distance / n_events as f64;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                       Event Summary                          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("Total events processed: {}", n_events);
    println!(
        "  BNS:  {} ({:.1}%)",
        n_bns,
        100.0 * n_bns as f64 / n_events as f64
    );
    println!(
        "  NSBH: {} ({:.1}%)",
        n_nsbh,
        100.0 * n_nsbh as f64 / n_events as f64
    );
    println!("  Mean distance: {:.0} Mpc", mean_distance);
    println!();
    println!("Detection rates:");
    println!(
        "  GRBs (on-axis):        {} ({:.1}%)",
        n_grb,
        100.0 * n_grb as f64 / n_events as f64
    );
    println!(
        "  Afterglows (ZTF 21 mag): {} ({:.1}%)",
        n_afterglow,
        100.0 * n_afterglow as f64 / n_events as f64
    );
    println!(
        "  Kilonovae:             {} ({:.1}%)",
        n_kilonova,
        100.0 * n_kilonova as f64 / n_events as f64
    );

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              Afterglow Magnitude Distribution                ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("### All Events with Afterglows ###");
    let stats_all = afterglow_all.compute_statistics();
    print_statistics(&stats_all);

    println!("\n### On-Axis GRBs Only ###");
    let stats_onaxis = afterglow_onaxis.compute_statistics();
    print_statistics(&stats_onaxis);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║          Afterglow Magnitudes by Distance Range             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    for distance_range in ["40-100 Mpc", "100-200 Mpc", "200-400 Mpc", "400-800 Mpc"] {
        if let Some((ag_stats, _)) = distance_bins.get_mut(distance_range) {
            let stats = ag_stats.compute_statistics();
            if stats.count > 0 {
                println!("### {} ###", distance_range);
                print_statistics(&stats);
                println!();
            }
        }
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              Afterglow Magnitude Histogram                   ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let histogram = afterglow_all.compute_histogram(1.0); // 1 mag bins
    println!("Magnitude distribution (1 mag bins):\n");
    println!("  Mag Range    | Count  | Fraction | Bar");
    println!("  -------------|--------|----------|{}", "-".repeat(50));

    for (mag, count) in histogram {
        let fraction = count as f64 / stats_all.count as f64;
        let bar_length = (fraction * 50.0) as usize;
        let bar = "█".repeat(bar_length);
        println!(
            "  {:.1} - {:.1} | {:6} | {:6.1}% | {}",
            mag,
            mag + 1.0,
            count,
            100.0 * fraction,
            bar
        );
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                   Survey Comparison                          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("Afterglow detection rates with different survey depths:\n");
    println!("  Survey | Limit | Detected (All) | Detected (On-Axis GRBs) |");
    println!("  -------|-------|----------------|-------------------------|");
    println!(
        "  ZTF    | 21.0  | {:6} ({:4.1}%) | {:6} ({:4.1}%)          |",
        stats_all.ztf_detectable,
        100.0 * stats_all.ztf_detectable as f64 / stats_all.count as f64,
        stats_onaxis.ztf_detectable,
        100.0 * stats_onaxis.ztf_detectable as f64 / stats_onaxis.count.max(1) as f64
    );
    println!(
        "  DECam  | 23.5  | {:6} ({:4.1}%) | {:6} ({:4.1}%)          |",
        stats_all.decam_detectable,
        100.0 * stats_all.decam_detectable as f64 / stats_all.count as f64,
        stats_onaxis.decam_detectable,
        100.0 * stats_onaxis.decam_detectable as f64 / stats_onaxis.count.max(1) as f64
    );
    println!(
        "  LSST   | 24.5  | {:6} ({:4.1}%) | {:6} ({:4.1}%)          |",
        stats_all.lsst_detectable,
        100.0 * stats_all.lsst_detectable as f64 / stats_all.count as f64,
        stats_onaxis.lsst_detectable,
        100.0 * stats_onaxis.lsst_detectable as f64 / stats_onaxis.count.max(1) as f64
    );

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                     Key Insights                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if stats_onaxis.count > 0 {
        println!(
            "• Mean on-axis afterglow magnitude: {:.1} mag",
            stats_onaxis.mean
        );
        println!(
            "• Median on-axis afterglow magnitude: {:.1} mag",
            stats_onaxis.median
        );
        println!(
            "• 80% of on-axis afterglows fall between {:.1} and {:.1} mag",
            stats_onaxis.p10, stats_onaxis.p90
        );
        println!();
        println!(
            "• ZTF (21 mag) detects {:.0}% of on-axis afterglows",
            100.0 * stats_onaxis.ztf_detectable as f64 / stats_onaxis.count as f64
        );
        println!(
            "• DECam (23.5 mag) detects {:.0}% of on-axis afterglows",
            100.0 * stats_onaxis.decam_detectable as f64 / stats_onaxis.count as f64
        );
        println!(
            "• LSST (24.5 mag) detects {:.0}% of on-axis afterglows",
            100.0 * stats_onaxis.lsst_detectable as f64 / stats_onaxis.count as f64
        );
    }
    println!();
    println!(
        "💡 O4 events are typically at {:.0} Mpc (mean), pushing even on-axis",
        mean_distance
    );
    println!(
        "   afterglows to {:.1}-{:.1} mag, requiring deep surveys like LSST.",
        stats_onaxis.p10, stats_onaxis.p90
    );

    Ok(())
}

fn print_statistics(stats: &Statistics) {
    println!("  Count:      {}", stats.count);
    println!("  Mean:       {:.2} mag", stats.mean);
    println!("  Median:     {:.2} mag", stats.median);
    println!("  Range:      {:.2} - {:.2} mag", stats.min, stats.max);
    println!("  10th %ile:  {:.2} mag", stats.p10);
    println!("  90th %ile:  {:.2} mag", stats.p90);
    println!();
    println!("  Detectable with:");
    println!(
        "    ZTF (21.0 mag):   {} ({:.1}%)",
        stats.ztf_detectable,
        100.0 * stats.ztf_detectable as f64 / stats.count as f64
    );
    println!(
        "    DECam (23.5 mag): {} ({:.1}%)",
        stats.decam_detectable,
        100.0 * stats.decam_detectable as f64 / stats.count as f64
    );
    println!(
        "    LSST (24.5 mag):  {} ({:.1}%)",
        stats.lsst_detectable,
        100.0 * stats.lsst_detectable as f64 / stats.count as f64
    );
}
