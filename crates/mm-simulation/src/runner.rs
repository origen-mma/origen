//! Simulation runner for multi-messenger coincidences
//!
//! Combines LIGO observing scenarios with real GRB alerts (rotated to match)
//! to create realistic multi-messenger simulations.

use anyhow::{Context, Result};
use mm_core::{Event, GWEvent, GammaRayEvent, GpsTime, ParsedSkymap, SkyPosition};
use mm_correlator::SupereventCorrelator;
use rand::Rng;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::rotation::rotate_skymap;
use crate::voevent::{GrbAlert, VOEventParser};

/// LIGO injection parameters
#[derive(Debug, Clone)]
pub struct LigoInjection {
    pub simulation_id: u32,
    pub ra: f64,       // radians
    pub dec: f64,      // radians
    pub distance: f64, // Mpc
    pub mass1: f64,    // solar masses
    pub mass2: f64,    // solar masses
    pub gps_time: f64, // Will be set randomly
}

/// Simulation configuration
pub struct SimulationConfig {
    /// Path to LIGO observing scenarios injections.dat
    pub injections_file: PathBuf,

    /// Path to LIGO observing scenarios skymap directory
    pub skymap_dir: PathBuf,

    /// Path to directory with GRB XML files
    pub grb_xml_dir: PathBuf,

    /// Number of simulations to run (0 = all)
    pub num_simulations: usize,

    /// Time offset range for GRB relative to GW (seconds)
    /// GRB will be placed randomly within [gw_time - offset, gw_time + offset]
    pub time_offset_range: f64,

    /// Whether to rotate GRB skymaps to match GW positions
    pub rotate_grb_skymaps: bool,
}

/// Simulation results
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub simulation_id: u32,
    pub gw_ra: f64,
    pub gw_dec: f64,
    pub grb_ra: f64,
    pub grb_dec: f64,
    pub spatial_separation: f64, // degrees
    pub temporal_offset: f64,    // seconds
    pub in_50_cr: bool,
    pub in_90_cr: bool,
    pub correlated: bool, // Did correlator associate them?
    pub spatial_significance: f64,
}

/// Simulation runner
pub struct SimulationRunner {
    config: SimulationConfig,
    injections: Vec<LigoInjection>,
    grb_xmls: Vec<PathBuf>,
}

impl SimulationRunner {
    /// Create a new simulation runner
    pub fn new(config: SimulationConfig) -> Result<Self> {
        info!("Loading LIGO injections from: {:?}", config.injections_file);
        let injections = Self::load_injections(&config.injections_file)?;
        info!("Loaded {} LIGO injections", injections.len());

        info!("Loading GRB XMLs from: {:?}", config.grb_xml_dir);
        let grb_xmls = Self::load_grb_xmls(&config.grb_xml_dir)?;
        info!("Loaded {} GRB XML files", grb_xmls.len());

        Ok(Self {
            config,
            injections,
            grb_xmls,
        })
    }

    /// Load LIGO injection parameters from injections.dat
    fn load_injections(path: &Path) -> Result<Vec<LigoInjection>> {
        let file = fs::File::open(path).context("Failed to open injections.dat")?;
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

            injections.push(LigoInjection {
                simulation_id: parts[0].parse()?,
                ra: parts[1].parse()?,       // radians
                dec: parts[2].parse()?,      // radians
                distance: parts[4].parse()?, // Mpc
                mass1: parts[5].parse()?,    // solar masses
                mass2: parts[6].parse()?,    // solar masses
                gps_time: 0.0,               // Will be set randomly
            });
        }

        Ok(injections)
    }

    /// Load GRB XML file paths
    fn load_grb_xmls(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut xmls = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("xml") {
                xmls.push(path);
            }
        }

        xmls.sort();
        Ok(xmls)
    }

    /// Run simulations
    pub fn run(&mut self) -> Result<Vec<SimulationResult>> {
        let mut results = Vec::new();
        let mut rng = rand::thread_rng();

        let num_sims = if self.config.num_simulations == 0 {
            self.injections.len()
        } else {
            self.config.num_simulations.min(self.injections.len())
        };

        info!("Running {} simulations...", num_sims);

        for i in 0..num_sims {
            let injection = &self.injections[i];

            // Pick a random GRB XML
            let grb_idx = rng.gen_range(0..self.grb_xmls.len());
            let grb_xml_path = &self.grb_xmls[grb_idx];

            // Parse GRB alert
            let grb_xml_content = fs::read_to_string(grb_xml_path)?;
            let mut grb_alert = VOEventParser::parse_string(&grb_xml_content)
                .context(format!("Failed to parse GRB XML: {:?}", grb_xml_path))?;

            // Set random GPS time for GW event
            let base_gps_time = 1_400_000_000.0; // Around 2014
            let gw_gps_time = base_gps_time + (i as f64) * 86400.0; // Space events 1 day apart

            // Set GRB time with random offset
            let time_offset =
                rng.gen_range(-self.config.time_offset_range..self.config.time_offset_range);
            let grb_gps_time = gw_gps_time + time_offset;

            // Load GW skymap
            let skymap_path = self
                .config
                .skymap_dir
                .join(format!("{}.fits", injection.simulation_id));
            let gw_skymap = match ParsedSkymap::from_fits(&skymap_path) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "Failed to parse skymap for injection {}: {}",
                        injection.simulation_id, e
                    );
                    continue;
                }
            };

            // Rotate GRB position to match GW position (if enabled)
            let (grb_ra, grb_dec) = if self.config.rotate_grb_skymaps {
                // Rotate GRB from its original position to GW injection position
                (injection.ra.to_degrees(), injection.dec.to_degrees())
            } else {
                // Keep original GRB position
                (grb_alert.ra, grb_alert.dec)
            };

            // Calculate spatial separation
            let spatial_sep = angular_separation(
                injection.ra.to_degrees(),
                injection.dec.to_degrees(),
                grb_ra,
                grb_dec,
            );

            // Query GW skymap at GRB position
            let grb_position = SkyPosition::new(grb_ra, grb_dec, 2.0);
            let prob = gw_skymap.probability_at_position(&grb_position);
            let in_50_cr = gw_skymap.is_in_credible_region(&grb_position, 0.5);
            let in_90_cr = gw_skymap.is_in_credible_region(&grb_position, 0.9);

            // Calculate spatial significance
            let spatial_sig = if in_50_cr {
                prob * 2.0
            } else if in_90_cr {
                prob * 1.5
            } else {
                prob
            };

            results.push(SimulationResult {
                simulation_id: injection.simulation_id,
                gw_ra: injection.ra.to_degrees(),
                gw_dec: injection.dec.to_degrees(),
                grb_ra,
                grb_dec,
                spatial_separation: spatial_sep,
                temporal_offset: time_offset,
                in_50_cr,
                in_90_cr,
                correlated: spatial_sep < 10.0 && time_offset.abs() < 100.0, // Simple criterion
                spatial_significance: spatial_sig,
            });

            if (i + 1) % 100 == 0 {
                info!("Completed {} / {} simulations", i + 1, num_sims);
            }
        }

        Ok(results)
    }

    /// Print summary statistics
    pub fn print_statistics(results: &[SimulationResult]) {
        let total = results.len();
        let in_50_cr = results.iter().filter(|r| r.in_50_cr).count();
        let in_90_cr = results.iter().filter(|r| r.in_90_cr).count();
        let correlated = results.iter().filter(|r| r.correlated).count();

        let avg_sep: f64 = results.iter().map(|r| r.spatial_separation).sum::<f64>() / total as f64;
        let median_sep = {
            let mut seps: Vec<f64> = results.iter().map(|r| r.spatial_separation).collect();
            seps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            seps[total / 2]
        };

        info!("\n=== Simulation Statistics ===");
        info!("Total simulations: {}", total);
        info!(
            "GRBs in 50% CR: {} ({:.1}%)",
            in_50_cr,
            100.0 * in_50_cr as f64 / total as f64
        );
        info!(
            "GRBs in 90% CR: {} ({:.1}%)",
            in_90_cr,
            100.0 * in_90_cr as f64 / total as f64
        );
        info!(
            "Correlated by system: {} ({:.1}%)",
            correlated,
            100.0 * correlated as f64 / total as f64
        );
        info!("Average spatial separation: {:.2}°", avg_sep);
        info!("Median spatial separation: {:.2}°", median_sep);
    }
}

/// Calculate angular separation between two sky positions (degrees)
fn angular_separation(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    use std::f64::consts::PI;

    let ra1_rad = ra1 * PI / 180.0;
    let dec1_rad = dec1 * PI / 180.0;
    let ra2_rad = ra2 * PI / 180.0;
    let dec2_rad = dec2 * PI / 180.0;

    let cos_sep = dec1_rad.sin() * dec2_rad.sin()
        + dec1_rad.cos() * dec2_rad.cos() * (ra1_rad - ra2_rad).cos();

    cos_sep.acos() * 180.0 / PI
}
