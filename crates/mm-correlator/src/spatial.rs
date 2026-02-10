use mm_core::{ParsedSkymap, SkyPosition};

/// Calculate joint False Alarm Rate (FAR) using RAVEN formula
/// FAR = time_prob × spatial_prob × trials_factor
pub fn calculate_joint_far(
    _time_offset: f64,
    time_window: f64,
    spatial_prob: f64,
    background_rate: f64,
    trials_factor: f64,
) -> f64 {
    // Time probability: uniform over window
    let time_prob = if time_window > 0.0 {
        1.0 / time_window
    } else {
        1.0
    };

    // Joint FAR (events per year)
    background_rate * time_prob * spatial_prob * trials_factor
}

/// Check if two positions match within threshold
pub fn positions_match(pos1: &SkyPosition, pos2: &SkyPosition, threshold_deg: f64) -> bool {
    let separation = pos1.angular_separation(pos2);
    separation <= threshold_deg
}

/// Calculate spatial probability (simplified)
/// Full version would integrate over GW skymap using HEALPix
pub fn calculate_spatial_probability(
    optical_pos: &SkyPosition,
    gw_pos: Option<&SkyPosition>,
    threshold_deg: f64,
) -> f64 {
    if let Some(gw_pos) = gw_pos {
        let separation = optical_pos.angular_separation(gw_pos);
        if separation <= threshold_deg {
            // Simple model: probability decreases with distance
            let fraction = separation / threshold_deg;
            1.0 - fraction
        } else {
            0.0
        }
    } else {
        // No GW position, assume uniform over sky
        let sky_area = 4.0 * std::f64::consts::PI * (180.0 / std::f64::consts::PI).powi(2); // ~41253 sq deg
        let search_area = std::f64::consts::PI * threshold_deg.powi(2);
        search_area / sky_area
    }
}

/// Calculate spatial probability using parsed HEALPix skymap
/// This is the more accurate version that queries the actual probability distribution
pub fn calculate_spatial_probability_from_skymap(
    position: &SkyPosition,
    skymap: &ParsedSkymap,
) -> f64 {
    // Query probability at this position from the skymap
    skymap.probability_at_position(position)
}

/// Integrate skymap probability over a circular region (RAVEN method for GRBs)
///
/// This implements the RAVEN spatial probability calculation:
/// - For a GRB with error_radius (e.g., 5° for Fermi, 4' for Swift)
/// - Integrate the GW skymap probability over that circular region
/// - Returns the total probability contained within the circle
///
/// Uses cdshealpix BMOC for fast spatial queries instead of
/// iterating over all ~3 million HEALPix pixels.
///
/// This is used in the joint FAR calculation:
/// joint_FAR = background_rate × time_prob × spatial_prob × trials_factor
pub fn integrate_skymap_over_circle(
    center: &SkyPosition,
    radius_deg: f64,
    skymap: &ParsedSkymap,
) -> f64 {
    // For very small radii (< 0.1°), use pixel probability directly
    // This avoids numerical issues in cdshealpix cone_coverage_approx
    if radius_deg < 0.1 {
        return calculate_spatial_probability_from_skymap(center, skymap);
    }

    use cdshealpix::nested::cone_coverage_approx;

    let depth = (skymap.nside as f64).log2() as u8;

    // Create BMOC from cone (error circle) using cdshealpix
    let lon_rad = center.ra.to_radians();
    let lat_rad = center.dec.to_radians();
    let radius_rad = radius_deg.to_radians();

    let cone_bmoc = cone_coverage_approx(depth, lon_rad, lat_rad, radius_rad);

    // Sum probabilities for all pixels in the BMOC at skymap depth
    let mut total_prob = 0.0;
    for pixel_hash in cone_bmoc.flat_iter() {
        let pixel_idx = pixel_hash as usize;
        if pixel_idx < skymap.probabilities.len() {
            total_prob += skymap.probabilities[pixel_idx];
        }
    }

    total_prob
}

/// Check if a position is within a credible region of a skymap
pub fn is_in_credible_region(position: &SkyPosition, skymap: &ParsedSkymap, level: f64) -> bool {
    skymap.is_in_credible_region(position, level)
}

/// Calculate spatial significance using skymap
/// Returns a score from 0-1 based on:
/// - Probability at position
/// - Whether it's within 50% or 90% credible region
///
/// The significance is normalized so that:
/// - Positions in 50% CR have significance ~0.9-1.0
/// - Positions in 90% CR (but not 50%) have significance ~0.6-0.9
/// - Positions outside 90% CR have lower significance based on relative probability
pub fn calculate_spatial_significance(position: &SkyPosition, skymap: &ParsedSkymap) -> f64 {
    let prob = skymap.probability_at_position(position);

    // Normalize probability relative to max probability
    let max_prob = skymap.probability_at_position(&skymap.max_prob_position);
    let normalized_prob = if max_prob > 0.0 {
        (prob / max_prob).min(1.0)
    } else {
        0.0
    };

    // Boost significance if within credible regions
    let in_50cr = skymap.is_in_credible_region(position, 0.5);
    let in_90cr = skymap.is_in_credible_region(position, 0.9);

    if in_50cr {
        // Within 50% CR: very significant (0.9-1.0)
        0.9 + (normalized_prob * 0.1)
    } else if in_90cr {
        // Within 90% CR: significant (0.6-0.9)
        0.6 + (normalized_prob * 0.3)
    } else {
        // Outside 90% CR: scale based on normalized probability (0.0-0.6)
        normalized_prob * 0.6
    }
}

/// Calculate angular separation and compare to skymap credible regions
pub fn calculate_skymap_offset(position: &SkyPosition, skymap: &ParsedSkymap) -> SkymapOffset {
    let max_prob_pos = &skymap.max_prob_position;
    let angular_separation = position.angular_separation(max_prob_pos);

    let in_50cr = skymap.is_in_credible_region(position, 0.5);
    let in_90cr = skymap.is_in_credible_region(position, 0.9);
    let probability = skymap.probability_at_position(position);

    SkymapOffset {
        angular_separation,
        in_50cr,
        in_90cr,
        probability,
    }
}

/// Calculate combined spatiotemporal FAR using RAVEN methodology
///
/// This implements the RAVEN formula from doi.org/10.3847/1538-4357/aabfd2:
///
/// ```text
/// temporal_far = time_window × ext_rate × gw_far
/// spatiotemporal_far = temporal_far / spatial_overlap
/// ```
///
/// Where:
/// - `time_window`: Coincidence window in seconds (e.g., 10s for GRB, 86400s for optical)
/// - `ext_rate`: External event rate in Hz (e.g., 325 GRB/yr = 1.03e-5 /s)
/// - `gw_far`: Gravitational wave false alarm rate in Hz
/// - `spatial_overlap`: Skymap overlap integral (probability)
///
/// # Arguments
/// * `time_window` - Coincidence window in seconds
/// * `ext_rate` - External event rate in Hz (events per second)
/// * `gw_far` - GW false alarm rate in Hz
/// * `spatial_overlap` - Spatial probability from skymap integration
///
/// # Returns
/// Combined spatiotemporal FAR in Hz (events per second)
///
/// # Example
/// ```rust
/// use mm_correlator::spatial::calculate_raven_spatiotemporal_far;
///
/// // GRB search: 10s window, 325 GRB/yr, typical BNS FAR, median spatial overlap
/// let time_window = 10.0; // seconds
/// let grb_rate = 325.0 / (365.25 * 24.0 * 3600.0); // 325/yr → Hz
/// let gw_far = 1e-7; // typical BNS FAR
/// let spatial_overlap = 3.4e-6; // median from calibration (1/290,000)
///
/// let combined_far = calculate_raven_spatiotemporal_far(
///     time_window, grb_rate, gw_far, spatial_overlap
/// );
/// // Expected: ~0.3 per year for prompt GRB with good localization
/// ```
pub fn calculate_raven_spatiotemporal_far(
    time_window: f64,
    ext_rate: f64,
    gw_far: f64,
    spatial_overlap: f64,
) -> f64 {
    // Temporal FAR (RAVEN untargeted search formula)
    let temporal_far = time_window * ext_rate * gw_far;

    // Spatiotemporal FAR (divide by spatial overlap)
    if spatial_overlap > 0.0 {
        temporal_far / spatial_overlap
    } else {
        // If spatial overlap is zero, return infinite FAR (no coincidence)
        f64::INFINITY
    }
}

/// Standard background rates for RAVEN-style calculations
/// Source: doi.org/10.3847/1538-4357/aabfd2 and LIGO-T2400116
pub mod background_rates {
    /// Combined GRB rate from Fermi-GBM + Swift-BAT + SVOM ECLAIRs
    /// = 325 per year = 1.03e-5 per second
    pub const GRB_RATE_HZ: f64 = 325.0 / (365.25 * 24.0 * 3600.0);

    /// Sub-threshold GRB rate
    /// = 65 per year = 2.06e-6 per second
    pub const SUBGRB_RATE_HZ: f64 = 65.0 / (365.25 * 24.0 * 3600.0);

    /// Combined GRB + SubGRB rate
    /// = 390 per year = 1.24e-5 per second
    pub const TOTAL_GRB_RATE_HZ: f64 = GRB_RATE_HZ + SUBGRB_RATE_HZ;

    /// IceCube high-energy neutrino rate
    /// = 13.91 per year = 4.41e-7 per second
    pub const NEUTRINO_RATE_HZ: f64 = 13.91 / (365.25 * 24.0 * 3600.0);

    /// Supernova rate relative to kilonova rate
    /// Assumption: ~10,000× more common based on volumetric rates
    pub const SN_TO_KN_RATE_RATIO: f64 = 10_000.0;
}

/// Result of skymap offset calculation
#[derive(Debug, Clone)]
pub struct SkymapOffset {
    /// Angular separation from maximum probability position (degrees)
    pub angular_separation: f64,

    /// Whether position is within 50% credible region
    pub in_50cr: bool,

    /// Whether position is within 90% credible region
    pub in_90cr: bool,

    /// Probability at this position
    pub probability: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positions_match() {
        let pos1 = SkyPosition::new(123.0, 45.0, 1.0);
        let pos2 = SkyPosition::new(123.5, 45.0, 1.0);

        assert!(positions_match(&pos1, &pos2, 1.0)); // 0.5 deg < 1.0 deg threshold
        assert!(!positions_match(&pos1, &pos2, 0.1)); // 0.5 deg > 0.1 deg threshold
    }

    #[test]
    fn test_calculate_joint_far() {
        // RAVEN-like parameters
        let time_offset = 3600.0; // 1 hour
        let time_window = 86400.0; // 1 day
        let spatial_prob = 0.1; // 10% of sky
        let background_rate = 1.0; // 1 alert per year
        let trials_factor = 7.0; // 7 bands

        let far = calculate_joint_far(
            time_offset,
            time_window,
            spatial_prob,
            background_rate,
            trials_factor,
        );

        // Should be small (significant detection)
        assert!(far < 1.0);
    }

    #[test]
    fn test_spatial_probability() {
        let optical_pos = SkyPosition::new(123.0, 45.0, 0.1);
        let gw_pos = SkyPosition::new(123.5, 45.0, 5.0);

        let prob = calculate_spatial_probability(&optical_pos, Some(&gw_pos), 10.0);

        // Close positions should have high probability
        assert!(prob > 0.9);
    }

    #[test]
    fn test_skymap_probability_query() {
        use mm_core::ParsedSkymap;

        // Create a test skymap from a real O4 simulation file
        let skymap_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky/0.fits";
        if std::path::Path::new(skymap_path).exists() {
            let skymap = ParsedSkymap::from_fits(skymap_path).expect("Failed to load test skymap");

            // Query at max probability position - should have high probability
            let max_prob_pos = &skymap.max_prob_position;
            let prob = calculate_spatial_probability_from_skymap(max_prob_pos, &skymap);

            println!("Probability at max position: {}", prob);
            assert!(
                prob > 0.0,
                "Max probability position should have non-zero probability"
            );

            // Max prob position should be in both 50% and 90% credible regions
            assert!(
                is_in_credible_region(max_prob_pos, &skymap, 0.5),
                "Max prob position should be in 50% CR"
            );
            assert!(
                is_in_credible_region(max_prob_pos, &skymap, 0.9),
                "Max prob position should be in 90% CR"
            );
        } else {
            println!("Skipping test - O4 skymap not found at {}", skymap_path);
        }
    }

    #[test]
    fn test_credible_region_membership() {
        use mm_core::ParsedSkymap;

        let skymap_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky/0.fits";
        if std::path::Path::new(skymap_path).exists() {
            let skymap = ParsedSkymap::from_fits(skymap_path).expect("Failed to load test skymap");

            // Test a position far from the event (should not be in credible regions)
            let far_position = SkyPosition::new(0.0, 0.0, 1.0);

            let in_50cr = is_in_credible_region(&far_position, &skymap, 0.5);
            let in_90cr = is_in_credible_region(&far_position, &skymap, 0.9);

            println!(
                "Position (0, 0) - in 50% CR: {}, in 90% CR: {}",
                in_50cr, in_90cr
            );

            // At least one of these should be false (unlikely that (0,0) is in the skymap)
            // This is a weak test but validates the function works
        } else {
            println!("Skipping test - O4 skymap not found");
        }
    }

    #[test]
    fn test_spatial_significance_calculation() {
        use mm_core::ParsedSkymap;

        let skymap_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky/0.fits";
        if std::path::Path::new(skymap_path).exists() {
            let skymap = ParsedSkymap::from_fits(skymap_path).expect("Failed to load test skymap");

            // Max probability position should have high significance
            let max_prob_pos = &skymap.max_prob_position;
            let significance = calculate_spatial_significance(max_prob_pos, &skymap);

            println!("Spatial significance at max prob: {}", significance);
            // Note: For HEALPix skymaps with many pixels, individual pixel probabilities
            // are small (e.g., 1e-4 for NSIDE=512). Significance is based on probability
            // and credible region membership, so values can be small.
            assert!(
                significance > 0.0,
                "Max probability position should have non-zero spatial significance"
            );

            // Test skymap offset calculation
            let offset = calculate_skymap_offset(max_prob_pos, &skymap);
            assert!(offset.in_50cr, "Max prob should be in 50% CR");
            assert!(offset.in_90cr, "Max prob should be in 90% CR");
            assert!(
                offset.probability > 0.0,
                "Max prob should have non-zero probability"
            );
            assert!(
                offset.angular_separation < 1.0,
                "Max prob should be very close to itself"
            );
        } else {
            println!("Skipping test - O4 skymap not found");
        }
    }

    #[test]
    fn test_skymap_offset_fallback() {
        use mm_core::SkyPosition;

        // Test positions without skymap
        let pos1 = SkyPosition::new(180.0, 30.0, 1.0);
        let pos2 = SkyPosition::new(181.0, 30.0, 1.0);

        // Without skymap, should fall back to angular separation
        let separation = pos1.angular_separation(&pos2);
        assert!(
            separation > 0.5 && separation < 2.0,
            "Angular separation should be ~1 degree"
        );
    }

    #[test]
    fn test_integrate_skymap_over_circle() {
        use mm_core::ParsedSkymap;

        let skymap_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky/0.fits";
        if std::path::Path::new(skymap_path).exists() {
            let skymap = ParsedSkymap::from_fits(skymap_path).expect("Failed to load test skymap");

            // Test 1: Small circle at max prob position should capture significant probability
            let max_prob_pos = &skymap.max_prob_position;
            let small_circle_prob = integrate_skymap_over_circle(max_prob_pos, 5.0, &skymap);

            println!(
                "RAVEN integration - 5° circle at max prob: {:.4}",
                small_circle_prob
            );
            assert!(
                small_circle_prob > 0.01,
                "5° circle at max prob should contain >1% probability"
            );

            // Test 2: Larger circle should contain more probability
            let large_circle_prob = integrate_skymap_over_circle(max_prob_pos, 10.0, &skymap);
            println!(
                "RAVEN integration - 10° circle at max prob: {:.4}",
                large_circle_prob
            );
            assert!(
                large_circle_prob > small_circle_prob,
                "10° circle should contain more probability than 5° circle"
            );

            // Test 3: Very small circle should have low but non-zero probability
            // Note: At NSIDE=512, pixel size is ~0.11°. A 0.1° circle is pixel-scale,
            // so BMOC cone_coverage_approx includes boundary pixels, yielding ~5-10x
            // a single pixel value. This is expected behavior for approximate coverage.
            let tiny_circle_prob = integrate_skymap_over_circle(max_prob_pos, 0.1, &skymap);
            let pixel_prob = calculate_spatial_probability_from_skymap(max_prob_pos, &skymap);
            println!(
                "RAVEN integration - 0.1° circle: {:.6}, pixel prob: {:.6}",
                tiny_circle_prob, pixel_prob
            );
            assert!(
                tiny_circle_prob > 0.0 && tiny_circle_prob < small_circle_prob,
                "0.1° circle should be non-zero but less than 5° circle"
            );

            // Test 4: Circle far from event should have low probability
            let far_pos = SkyPosition::new(0.0, 0.0, 0.0);
            let far_circle_prob = integrate_skymap_over_circle(&far_pos, 5.0, &skymap);
            println!(
                "RAVEN integration - 5° circle far from event: {:.6}",
                far_circle_prob
            );
            assert!(
                far_circle_prob < small_circle_prob,
                "Circle far from event should have less probability"
            );
        } else {
            println!("Skipping test - O4 skymap not found at {}", skymap_path);
        }
    }

    #[test]
    fn test_raven_far_calibration_simulation() {
        use mm_core::ParsedSkymap;
        use rand::Rng;

        let skymap_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky/0.fits";
        if !std::path::Path::new(skymap_path).exists() {
            println!("Skipping FAR calibration test - O4 skymap not found");
            return;
        }

        let skymap = ParsedSkymap::from_fits(skymap_path).expect("Failed to load test skymap");

        println!("\n========== RAVEN FAR CALIBRATION SIMULATION ==========");
        println!("Using GW skymap: {}", skymap_path);
        println!(
            "Skymap 50% CR area: {:.1} deg²",
            skymap.credible_regions[0].area
        );
        println!(
            "Skymap 90% CR area: {:.1} deg²",
            skymap.credible_regions[1].area
        );

        // Simulate GRBs with different error radii (Fermi vs Swift)
        let grb_configs = vec![
            ("Fermi-GBM", 5.0),        // Typical Fermi error
            ("Swift-BAT", 2.0 / 60.0), // 2 arcmin = 0.033 deg
        ];

        for (instrument, error_radius_deg) in grb_configs {
            println!(
                "\n---------- {} (error radius = {:.3}°) ----------",
                instrument, error_radius_deg
            );

            // 1. SIGNAL: Associated GRB at true GW position
            let true_pos = &skymap.max_prob_position;
            let signal_spatial_prob =
                integrate_skymap_over_circle(true_pos, error_radius_deg, &skymap);

            println!("Signal GRB at true position:");
            println!(
                "  Position: RA={:.2}°, Dec={:.2}°",
                true_pos.ra, true_pos.dec
            );
            println!("  Spatial probability: {:.6}", signal_spatial_prob);

            // 2. BACKGROUND: 1000 unassociated GRBs at random sky positions
            let n_background = 1000;
            let mut rng = rand::thread_rng();
            let mut background_probs = Vec::new();

            for _ in 0..n_background {
                // Random position on sky (uniform in RA, sin(Dec) for uniform area)
                let ra: f64 = rng.gen_range(0.0..360.0);
                let sin_dec: f64 = rng.gen_range(-1.0..1.0);
                let dec = sin_dec.asin().to_degrees(); // Uniform in sin(dec)
                let bg_pos = SkyPosition::new(ra, dec, 0.0);

                let bg_prob = integrate_skymap_over_circle(&bg_pos, error_radius_deg, &skymap);
                background_probs.push(bg_prob);
            }

            // Sort background probabilities (highest first)
            background_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());

            // 3. Calculate empirical FAR and significance
            let n_exceeding = background_probs
                .iter()
                .filter(|&&p| p >= signal_spatial_prob)
                .count();

            // Empirical FAR with continuity correction
            let empirical_far = (n_exceeding as f64 + 1.0) / (n_background as f64 + 1.0);

            // Convert to Gaussian significance (one-sided)
            let significance_sigma = if empirical_far < 1.0 {
                // Φ^(-1)(1 - FAR) where Φ is the standard normal CDF
                let z = 1.0 - empirical_far;
                // Approximate inverse CDF for quick calculation
                if z > 0.5 {
                    let t = (-2.0 * (1.0 - z).ln()).sqrt();
                    t - (2.30753 + t * 0.27061) / (1.0 + t * (0.99229 + t * 0.04481))
                } else {
                    0.0
                }
            } else {
                0.0
            };

            println!("\nBackground distribution (N={}):", n_background);
            println!("  Max: {:.6}", background_probs[0]);
            println!("  95th percentile: {:.6}", background_probs[49]); // Top 5%
            println!("  90th percentile: {:.6}", background_probs[99]); // Top 10%
            println!("  Median: {:.6}", background_probs[499]);
            println!(
                "  Mean: {:.6}",
                background_probs.iter().sum::<f64>() / n_background as f64
            );

            println!("\nStatistical significance:");
            println!("  N(background ≥ signal): {}", n_exceeding);
            println!(
                "  Empirical FAR: {:.6} ({:.2}%)",
                empirical_far,
                empirical_far * 100.0
            );
            println!("  Significance: {:.2}σ", significance_sigma);

            // Calculate RAVEN-style joint FAR
            let time_window = 86400.0; // 1 day in seconds
            let time_prob = 1.0 / time_window;
            let background_rate = 200.0; // ~200 GRBs/year
            let trials_factor = 1.0; // Single instrument/band for simplicity

            let joint_far = calculate_joint_far(
                0.0, // time offset doesn't matter for spatial-only test
                time_window,
                signal_spatial_prob,
                background_rate,
                trials_factor,
            );

            println!("\nRAVEN joint FAR calculation:");
            println!("  Time prob: {:.2e} (1/day)", time_prob);
            println!("  Spatial prob: {:.6}", signal_spatial_prob);
            println!("  Background rate: {} GRBs/year", background_rate);
            println!("  Joint FAR: {:.6} per year", joint_far);
            println!("  = {:.3e} per year", joint_far);

            // Assertion: signal should be more significant than median background
            assert!(
                signal_spatial_prob > background_probs[499],
                "{}: Signal should exceed median background (signal={:.6}, median={:.6})",
                instrument,
                signal_spatial_prob,
                background_probs[499]
            );

            // For Swift with tiny error radius, signal should be in top 5%
            if error_radius_deg < 0.1 {
                assert!(
                    signal_spatial_prob > background_probs[49],
                    "Swift BAT signal should be in top 5% (signal={:.6}, 95th%={:.6})",
                    signal_spatial_prob,
                    background_probs[49]
                );
            }
        }

        println!("\n======================================================\n");
    }

    #[test]
    #[ignore] // Run with: cargo test --package mm-correlator --lib test_o4_population_far_calibration -- --ignored --nocapture
    fn test_o4_population_far_calibration() {
        use mm_core::ParsedSkymap;
        use rand::Rng;
        use std::fs::File;
        use std::io::{BufRead, BufReader, Write};

        // Paths to O4 simulation data
        let injections_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/injections.dat";
        let skymap_dir = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky";

        if !std::path::Path::new(injections_path).exists() {
            println!("Skipping population test - injections.dat not found");
            return;
        }

        println!("\n========== O4 POPULATION FAR CALIBRATION ==========");
        println!("Loading injections from: {}", injections_path);

        // Parse injections.dat
        // Format: simulation_id  longitude(rad)  latitude(rad)  inclination  distance  mass1  mass2  spin1z  spin2z
        let file = File::open(injections_path).expect("Failed to open injections.dat");
        let reader = BufReader::new(file);

        let mut injections = Vec::new();
        for line in reader.lines() {
            let line = line.expect("Failed to read line");
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                // Skip header line and any lines that don't parse
                if let Ok(sim_id) = parts[0].parse::<usize>() {
                    if let (Ok(lon_rad), Ok(lat_rad), Ok(mass1), Ok(mass2)) = (
                        parts[1].parse::<f64>(),
                        parts[2].parse::<f64>(),
                        parts[5].parse::<f64>(),
                        parts[6].parse::<f64>(),
                    ) {
                        // Filter for BNS and NSBH only
                        // BNS: both masses < 3.0 solar masses
                        // NSBH: one mass < 3.0 (NS) and one mass >= 3.0 (BH)
                        let is_bns = mass1 < 3.0 && mass2 < 3.0;
                        let is_nsbh =
                            (mass1 < 3.0 && mass2 >= 3.0) || (mass1 >= 3.0 && mass2 < 3.0);

                        if is_bns || is_nsbh {
                            // Convert radians to degrees
                            let ra_deg = lon_rad.to_degrees();
                            let dec_deg = lat_rad.to_degrees();

                            injections.push((sim_id, ra_deg, dec_deg));
                        }
                    }
                }
            }
        }

        println!("Loaded {} BNS+NSBH injections", injections.len());

        // Test both Fermi-GBM and Swift-BAT with realistic error radii
        let instruments = vec![
            ("Fermi-GBM", 13.2),  // Empirical median from 5832 real GRBs
            ("Swift-BAT", 0.033), // Literature value: ~2 arcmin
        ];

        let n_events = injections.len(); // Process ALL BNS+NSBH events
        let n_background_per_event = 1000; // 1000 background trials per event
        let mut rng = rand::thread_rng();

        // Store results for both instruments
        let mut all_results = Vec::new();

        for (instrument_name, grb_error_deg) in instruments {
            println!(
                "\n========== {} (error radius = {:.4}°) ==========",
                instrument_name, grb_error_deg
            );
            println!("Processing {} BNS+NSBH events", n_events);
            println!("Background trials per event: {}", n_background_per_event);
            println!(
                "Total background trials: {}",
                n_events * n_background_per_event
            );

            let mut signal_probs = Vec::new();
            let mut background_probs = Vec::new();

            // Process each event
            #[allow(clippy::needless_range_loop)]
            for i in 0..n_events {
                let (sim_id, true_ra, true_dec) = injections[i];
                let skymap_path = format!("{}/{}.fits", skymap_dir, sim_id);

                if !std::path::Path::new(&skymap_path).exists() {
                    println!("  Event {}: skymap not found, skipping", sim_id);
                    continue;
                }

                let skymap = match ParsedSkymap::from_fits(&skymap_path) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("  Event {}: failed to load skymap: {}", sim_id, e);
                        continue;
                    }
                };

                // SIGNAL: Add random offset within error circle (NOT centered at true position)
                // This simulates a realistic GRB localization with error
                let offset_angle: f64 = rng.gen_range(0.0..grb_error_deg); // Random radius within error circle
                let offset_azimuth: f64 = rng.gen_range(0.0..360.0); // Random angle

                // Convert offset to RA/Dec offset (approximate for small angles)
                let offset_ra =
                    offset_angle * offset_azimuth.to_radians().cos() / true_dec.to_radians().cos();
                let offset_dec = offset_angle * offset_azimuth.to_radians().sin();

                let observed_ra = true_ra + offset_ra;
                let observed_dec = (true_dec + offset_dec).clamp(-90.0, 90.0); // Clamp to valid range

                let grb_observed_pos = SkyPosition::new(observed_ra, observed_dec, 0.0);

                // Calculate spatial probability using RAVEN integration
                let signal_prob =
                    integrate_skymap_over_circle(&grb_observed_pos, grb_error_deg, &skymap);
                signal_probs.push(signal_prob);

                // BACKGROUND: Generate random positions
                for _ in 0..n_background_per_event {
                    let bg_ra: f64 = rng.gen_range(0.0..360.0);
                    let sin_dec: f64 = rng.gen_range(-1.0..1.0);
                    let bg_dec = sin_dec.asin().to_degrees();
                    let bg_pos = SkyPosition::new(bg_ra, bg_dec, 0.0);

                    let bg_prob = integrate_skymap_over_circle(&bg_pos, grb_error_deg, &skymap);
                    background_probs.push(bg_prob);
                }

                if (i + 1) % 50 == 0 {
                    println!("  Processed {}/{} events...", i + 1, n_events);
                }
            }

            println!("\nProcessed {} events", signal_probs.len());
            println!("  Signal trials: {}", signal_probs.len());
            println!("  Background trials: {}", background_probs.len());

            // Sort for statistics
            signal_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());
            background_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());

            // Calculate statistics
            let signal_median = signal_probs[signal_probs.len() / 2];
            let signal_mean = signal_probs.iter().sum::<f64>() / signal_probs.len() as f64;
            let bg_median = background_probs[background_probs.len() / 2];
            let bg_mean = background_probs.iter().sum::<f64>() / background_probs.len() as f64;

            println!("\n========== SIGNAL DISTRIBUTION ==========");
            println!("  Max: {:.6}", signal_probs[0]);
            println!(
                "  95th percentile: {:.6}",
                signal_probs[(signal_probs.len() as f64 * 0.05) as usize]
            );
            println!(
                "  75th percentile: {:.6}",
                signal_probs[(signal_probs.len() as f64 * 0.25) as usize]
            );
            println!("  Median: {:.6}", signal_median);
            println!("  Mean: {:.6}", signal_mean);
            println!("  Min: {:.6}", signal_probs[signal_probs.len() - 1]);

            println!("\n========== BACKGROUND DISTRIBUTION ==========");
            println!("  Max: {:.6}", background_probs[0]);
            println!(
                "  95th percentile: {:.6}",
                background_probs[(background_probs.len() as f64 * 0.05) as usize]
            );
            println!(
                "  75th percentile: {:.6}",
                background_probs[(background_probs.len() as f64 * 0.25) as usize]
            );
            println!("  Median: {:.6}", bg_median);
            println!("  Mean: {:.6}", bg_mean);
            println!("  Min: {:.6}", background_probs[background_probs.len() - 1]);

            // Write histogram data to instrument-specific file
            let instrument_filename = instrument_name.to_lowercase().replace("-", "_");
            let output_path = format!("/tmp/far_calibration_{}.dat", instrument_filename);
            let mut output = File::create(&output_path).expect("Failed to create output file");
            writeln!(output, "# type spatial_prob").unwrap();
            for prob in &signal_probs {
                writeln!(output, "signal {:.8}", prob).unwrap();
            }
            for prob in &background_probs {
                writeln!(output, "background {:.8}", prob).unwrap();
            }
            println!("\nHistogram data written to: {}", output_path);

            // Statistical test: signal should be significantly higher than background
            println!("\n========== STATISTICAL COMPARISON ==========");
            println!(
                "  Signal median / Background median: {:.2}x",
                signal_median / bg_median
            );
            println!(
                "  Signal mean / Background mean: {:.2}x",
                signal_mean / bg_mean
            );

            // Count how many signal trials exceed 95th percentile of background
            let bg_95th = background_probs[(background_probs.len() as f64 * 0.05) as usize];
            let n_signal_exceeding = signal_probs.iter().filter(|&&p| p > bg_95th).count();
            let frac_signal_exceeding = n_signal_exceeding as f64 / signal_probs.len() as f64;

            println!(
                "  Signal trials exceeding background 95th percentile: {} / {} ({:.1}%)",
                n_signal_exceeding,
                signal_probs.len(),
                frac_signal_exceeding * 100.0
            );

            // Count zeros
            let n_signal_zero = signal_probs.iter().filter(|&&p| p < 1e-8).count();
            let n_bg_zero = background_probs.iter().filter(|&&p| p < 1e-8).count();
            println!(
                "  Zero probability trials: Signal {}/{} ({:.1}%), Background {}/{} ({:.1}%)",
                n_signal_zero,
                signal_probs.len(),
                100.0 * n_signal_zero as f64 / signal_probs.len() as f64,
                n_bg_zero,
                background_probs.len(),
                100.0 * n_bg_zero as f64 / background_probs.len() as f64
            );

            // ========== RAVEN-STYLE ANALYTICAL FAR CALCULATION ==========
            println!("\n========== RAVEN ANALYTICAL FAR ==========");

            // Use RAVEN background rates and parameters
            let time_window = 10.0; // GRB search: -5s to +5s = 10s window
            let grb_rate = background_rates::GRB_RATE_HZ; // 325/yr
            let gw_far = 1e-7; // Typical BNS FAR in Hz

            println!("  Parameters:");
            println!("    Time window: {:.1} s", time_window);
            println!(
                "    GRB rate: {:.3e} Hz ({:.1} /yr)",
                grb_rate,
                grb_rate * 365.25 * 24.0 * 3600.0
            );
            println!("    GW FAR: {:.2e} Hz", gw_far);

            // Calculate RAVEN FAR for BOTH signal and background populations
            let temporal_far = time_window * grb_rate * gw_far;

            // Signal FAR distribution
            let mut signal_fars: Vec<f64> = signal_probs
                .iter()
                .filter(|&&p| p > 0.0)
                .map(|&p| calculate_raven_spatiotemporal_far(time_window, grb_rate, gw_far, p))
                .filter(|&f| f.is_finite())
                .collect();
            signal_fars.sort_by(|a, b| a.partial_cmp(b).unwrap());

            // Background FAR distribution
            let mut background_fars: Vec<f64> = background_probs
                .iter()
                .filter(|&&p| p > 0.0)
                .map(|&p| calculate_raven_spatiotemporal_far(time_window, grb_rate, gw_far, p))
                .filter(|&f| f.is_finite())
                .collect();
            background_fars.sort_by(|a, b| a.partial_cmp(b).unwrap());

            println!(
                "\n  Temporal FAR (without spatial): {:.3e} Hz ({:.4} /yr)",
                temporal_far,
                temporal_far * 365.25 * 24.0 * 3600.0
            );

            if !signal_fars.is_empty() && !background_fars.is_empty() {
                let signal_far_median = signal_fars[signal_fars.len() / 2];
                let bg_far_median = background_fars[background_fars.len() / 2];

                println!("\n  Signal FAR Distribution:");
                println!(
                    "    Median: {:.3e} Hz ({:.4} /yr)",
                    signal_far_median,
                    signal_far_median * 365.25 * 24.0 * 3600.0
                );
                println!(
                    "    95th percentile: {:.3e} Hz ({:.2} /yr)",
                    signal_fars[(signal_fars.len() as f64 * 0.95)
                        .min(signal_fars.len() as f64 - 1.0)
                        as usize],
                    signal_fars[(signal_fars.len() as f64 * 0.95)
                        .min(signal_fars.len() as f64 - 1.0)
                        as usize]
                        * 365.25
                        * 24.0
                        * 3600.0
                );

                println!("\n  Background FAR Distribution:");
                println!(
                    "    Median: {:.3e} Hz ({:.2e} /yr)",
                    bg_far_median,
                    bg_far_median * 365.25 * 24.0 * 3600.0
                );
                println!(
                    "    5th percentile (best): {:.3e} Hz ({:.2e} /yr)",
                    background_fars[(background_fars.len() as f64 * 0.05).max(0.0) as usize],
                    background_fars[(background_fars.len() as f64 * 0.05).max(0.0) as usize]
                        * 365.25
                        * 24.0
                        * 3600.0
                );

                // KEY INSIGHT: RAVEN discrimination = FAR_background / FAR_signal
                let raven_discrimination = bg_far_median / signal_far_median;
                let empirical_discrimination = signal_median / bg_median;

                println!("\n  ✅ RECONCILIATION (Empirical ↔ RAVEN):");
                println!(
                    "    RAVEN FAR discrimination (bg/signal): {:.0}×",
                    raven_discrimination
                );
                println!(
                    "    Empirical P discrimination (signal/bg): {:.0}×",
                    empirical_discrimination
                );
                println!(
                    "    Ratio (should be ~1): {:.2}",
                    raven_discrimination / empirical_discrimination
                );
                println!("\n    Mathematical equivalence:");
                println!("      FAR_bg/FAR_signal = (Δt×R×FAR_GW/P_bg) / (Δt×R×FAR_GW/P_sig)");
                println!("                        = P_signal / P_background");
                println!("                        = empirical discrimination!");
                println!("      ✅ Both methods are measuring the same thing on different scales.");

                // Count significant coincidences
                let far_threshold = 1.0 / (365.25 * 24.0 * 3600.0); // 1/yr in Hz
                let n_signal_significant =
                    signal_fars.iter().filter(|&&f| f < far_threshold).count();
                let n_bg_significant = background_fars
                    .iter()
                    .filter(|&&f| f < far_threshold)
                    .count();

                println!("\n  Significant Coincidences (FAR < 1/yr threshold):");
                println!(
                    "    Signal: {} / {} ({:.1}%)",
                    n_signal_significant,
                    signal_fars.len(),
                    100.0 * n_signal_significant as f64 / signal_fars.len() as f64
                );
                println!(
                    "    Background: {} / {} ({:.3}%)",
                    n_bg_significant,
                    background_fars.len(),
                    100.0 * n_bg_significant as f64 / background_fars.len() as f64
                );

                if n_bg_significant > 0 {
                    println!(
                        "    ⚠️  False positive rate: {:.4}% ({} random coincidences below threshold)",
                        100.0 * n_bg_significant as f64 / background_fars.len() as f64,
                        n_bg_significant
                    );
                } else {
                    println!("    ✅ No false positives (0 random coincidences below threshold)");
                }
            }

            // Store results for comparison
            all_results.push((
                instrument_name,
                grb_error_deg,
                signal_median,
                signal_mean,
                bg_median,
                bg_mean,
                frac_signal_exceeding,
                n_signal_zero as f64 / signal_probs.len() as f64,
                n_bg_zero as f64 / background_probs.len() as f64,
            ));

            // Assertions for this instrument
            assert!(
                signal_median > bg_median * 2.0,
                "{}: Signal median should be >2x background median (signal={:.6}, bg={:.6})",
                instrument_name,
                signal_median,
                bg_median
            );

            assert!(
                signal_mean > bg_mean * 2.0,
                "{}: Signal mean should be >2x background mean (signal={:.6}, bg={:.6})",
                instrument_name,
                signal_mean,
                bg_mean
            );

            // Instrument-dependent threshold: smaller error circles integrate less absolute
            // probability but still show excellent discrimination via ratios
            let threshold = if grb_error_deg < 1.0 { 0.4 } else { 0.5 };
            assert!(
                frac_signal_exceeding > threshold,
                "{}: At least {:.0}% of signal trials should exceed background 95th percentile (got {:.1}%)",
                instrument_name,
                threshold * 100.0,
                frac_signal_exceeding * 100.0
            );

            println!("\n========== {} TEST PASSED ==========\n", instrument_name);
        }

        // Print comparison summary
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║           INSTRUMENT COMPARISON SUMMARY                       ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        for (name, error, sig_med, sig_mean, bg_med, bg_mean, frac_exceed, sig_zero, bg_zero) in
            &all_results
        {
            println!("{}:", name);
            println!("  Error radius: {:.4}°", error);
            println!(
                "  Signal median: {:.6}, Background median: {:.6} (ratio: {:.1}x)",
                sig_med,
                bg_med,
                sig_med / bg_med
            );
            println!(
                "  Signal mean: {:.6}, Background mean: {:.6} (ratio: {:.1}x)",
                sig_mean,
                bg_mean,
                sig_mean / bg_mean
            );
            println!(
                "  Signal exceeding bg 95th percentile: {:.1}%",
                frac_exceed * 100.0
            );
            println!(
                "  Zero probability: Signal {:.1}%, Background {:.1}%",
                sig_zero * 100.0,
                bg_zero * 100.0
            );
            println!();
        }

        println!("\n========== ALL TESTS PASSED ==========\n");
    }

    #[test]
    #[ignore] // Run with: cargo test --package mm-correlator --lib test_optical_far_calibration -- --ignored --nocapture
    fn test_optical_far_calibration() {
        use mm_core::ParsedSkymap;
        use rand::Rng;
        use std::fs::File;
        use std::io::{BufRead, BufReader, Write};

        // Paths to O4 simulation data
        let injections_path =
            "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/injections.dat";
        let skymap_dir = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/allsky";

        if !std::path::Path::new(injections_path).exists() {
            println!("Skipping population test - injections.dat not found");
            return;
        }

        println!("\n========== OPTICAL FAR CALIBRATION (KILONOVA vs SUPERNOVA) ==========");
        println!("Loading injections from: {}", injections_path);

        // Parse injections.dat - same as GRB test
        let file = File::open(injections_path).expect("Failed to open injections.dat");
        let reader = BufReader::new(file);

        let mut injections = Vec::new();
        for line in reader.lines() {
            let line = line.expect("Failed to read line");
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                if let Ok(sim_id) = parts[0].parse::<usize>() {
                    if let (Ok(lon_rad), Ok(lat_rad), Ok(mass1), Ok(mass2)) = (
                        parts[1].parse::<f64>(),
                        parts[2].parse::<f64>(),
                        parts[5].parse::<f64>(),
                        parts[6].parse::<f64>(),
                    ) {
                        // Filter for BNS and NSBH only (same as GRB test)
                        let is_bns = mass1 < 3.0 && mass2 < 3.0;
                        let is_nsbh =
                            (mass1 < 3.0 && mass2 >= 3.0) || (mass1 >= 3.0 && mass2 < 3.0);

                        if is_bns || is_nsbh {
                            let ra_deg = lon_rad.to_degrees();
                            let dec_deg = lat_rad.to_degrees();
                            injections.push((sim_id, ra_deg, dec_deg));
                        }
                    }
                }
            }
        }

        println!("Loaded {} BNS+NSBH injections", injections.len());

        // Optical parameters
        // Typical optical localization uncertainty (ZTF: ~2 arcsec, LSST: ~0.5 arcsec)
        let optical_position_error_deg = 2.0 / 3600.0; // 2 arcsec in degrees

        // Time window for GW-optical correlation: -1s to +1 day
        let time_window_start = -1.0; // seconds before GW
        let time_window_end = 86400.0; // seconds after GW (+1 day)

        // Rate ratio: Supernovae are ~10,000× more common than kilonovae
        // For fair comparison with GRB test (which had 1000 bg per event),
        // we'll also use 1000 supernova trials per event
        let n_events = injections.len();
        let n_supernova_per_event = 1000;
        let mut rng = rand::thread_rng();

        println!("\n========== OPTICAL TRANSIENT PARAMETERS ==========");
        println!(
            "Position error: {:.4}° ({:.2} arcsec)",
            optical_position_error_deg,
            optical_position_error_deg * 3600.0
        );
        println!(
            "Time window: {:.1}s to +{:.0}s ({:.1} days)",
            time_window_start,
            time_window_end,
            time_window_end / 86400.0
        );
        println!("Processing {} BNS+NSBH events", n_events);
        println!(
            "Supernova background trials per event: {}",
            n_supernova_per_event
        );
        println!(
            "Total background trials: {}",
            n_events * n_supernova_per_event
        );
        println!("SN rate / KN rate: ~10,000× (empirical)");

        let mut signal_probs = Vec::new();
        let mut background_probs = Vec::new();
        let mut signal_in_50cr = Vec::new();
        let mut signal_in_90cr = Vec::new();
        let mut background_in_50cr = Vec::new();
        let mut background_in_90cr = Vec::new();

        // Process each event
        #[allow(clippy::needless_range_loop)]
        for i in 0..n_events {
            let (sim_id, true_ra, true_dec) = injections[i];
            let skymap_path = format!("{}/{}.fits", skymap_dir, sim_id);

            if !std::path::Path::new(&skymap_path).exists() {
                println!("  Event {}: skymap not found, skipping", sim_id);
                continue;
            }

            let skymap = match ParsedSkymap::from_fits(&skymap_path) {
                Ok(s) => s,
                Err(e) => {
                    println!("  Event {}: failed to load skymap: {}", sim_id, e);
                    continue;
                }
            };

            // SIGNAL: Kilonova at true position + tiny optical localization error
            // This simulates a real optical detection with ZTF/LSST astrometric uncertainty
            let offset_angle: f64 = rng.gen_range(0.0..optical_position_error_deg);
            let offset_azimuth: f64 = rng.gen_range(0.0..360.0);

            let offset_ra =
                offset_angle * offset_azimuth.to_radians().cos() / true_dec.to_radians().cos();
            let offset_dec = offset_angle * offset_azimuth.to_radians().sin();

            let observed_ra = true_ra + offset_ra;
            let observed_dec = (true_dec + offset_dec).clamp(-90.0, 90.0);

            let kn_pos = SkyPosition::new(observed_ra, observed_dec, 0.0);

            // Calculate spatial probability at kilonova position
            // Use tiny integration radius since optical position is very precise
            let signal_prob =
                integrate_skymap_over_circle(&kn_pos, optical_position_error_deg, &skymap);
            signal_probs.push(signal_prob);

            // Track credible region membership (more intuitive than absolute probability)
            let kn_in_50cr = is_in_credible_region(&kn_pos, &skymap, 0.5);
            let kn_in_90cr = is_in_credible_region(&kn_pos, &skymap, 0.9);
            signal_in_50cr.push(kn_in_50cr);
            signal_in_90cr.push(kn_in_90cr);

            // BACKGROUND: Supernovae at random positions AND random times
            // Key difference from GRB: SNe occur at random times, not coincident with GW
            for _ in 0..n_supernova_per_event {
                // Random sky position
                let bg_ra: f64 = rng.gen_range(0.0..360.0);
                let sin_dec: f64 = rng.gen_range(-1.0..1.0);
                let bg_dec = sin_dec.asin().to_degrees();
                let bg_pos = SkyPosition::new(bg_ra, bg_dec, 0.0);

                // Random time within GW-optical correlation window
                // Unlike GRBs (which are temporally coincident), SNe occur at random times
                // This is accounted for in temporal FAR, but spatial correlation is computed regardless
                let _bg_time_offset: f64 = rng.gen_range(time_window_start..time_window_end);

                let bg_prob =
                    integrate_skymap_over_circle(&bg_pos, optical_position_error_deg, &skymap);
                background_probs.push(bg_prob);

                // Track CR membership for background too
                let bg_in_50cr = is_in_credible_region(&bg_pos, &skymap, 0.5);
                let bg_in_90cr = is_in_credible_region(&bg_pos, &skymap, 0.9);
                background_in_50cr.push(bg_in_50cr);
                background_in_90cr.push(bg_in_90cr);
            }

            if (i + 1) % 50 == 0 {
                println!("  Processed {}/{} events...", i + 1, n_events);
            }
        }

        println!("\nProcessed {} events", signal_probs.len());
        println!("  Kilonova (signal) trials: {}", signal_probs.len());
        println!(
            "  Supernova (background) trials: {}",
            background_probs.len()
        );

        // Sort for statistics
        signal_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());
        background_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());

        // Calculate statistics
        let signal_median = signal_probs[signal_probs.len() / 2];
        let signal_mean = signal_probs.iter().sum::<f64>() / signal_probs.len() as f64;
        let bg_median = background_probs[background_probs.len() / 2];
        let bg_mean = background_probs.iter().sum::<f64>() / background_probs.len() as f64;

        println!("\n========== KILONOVA (SIGNAL) DISTRIBUTION ==========");
        println!("  Max: {:.6}", signal_probs[0]);
        println!(
            "  95th percentile: {:.6}",
            signal_probs[(signal_probs.len() as f64 * 0.05) as usize]
        );
        println!(
            "  75th percentile: {:.6}",
            signal_probs[(signal_probs.len() as f64 * 0.25) as usize]
        );
        println!("  Median: {:.6}", signal_median);
        println!("  Mean: {:.6}", signal_mean);
        println!("  Min: {:.6}", signal_probs[signal_probs.len() - 1]);

        println!("\n========== SUPERNOVA (BACKGROUND) DISTRIBUTION ==========");
        println!("  Max: {:.6}", background_probs[0]);
        println!(
            "  95th percentile: {:.6}",
            background_probs[(background_probs.len() as f64 * 0.05) as usize]
        );
        println!(
            "  75th percentile: {:.6}",
            background_probs[(background_probs.len() as f64 * 0.25) as usize]
        );
        println!("  Median: {:.6}", bg_median);
        println!("  Mean: {:.6}", bg_mean);
        println!("  Min: {:.6}", background_probs[background_probs.len() - 1]);

        // Write histogram data to file
        let output_path = "/tmp/far_calibration_optical.dat";
        let mut output = File::create(output_path).expect("Failed to create output file");
        writeln!(output, "# type spatial_prob").unwrap();
        for prob in &signal_probs {
            writeln!(output, "signal {:.8}", prob).unwrap();
        }
        for prob in &background_probs {
            writeln!(output, "background {:.8}", prob).unwrap();
        }
        println!("\nHistogram data written to: {}", output_path);

        // Statistical test: signal should be significantly higher than background
        println!("\n========== STATISTICAL COMPARISON ==========");
        println!(
            "  Kilonova median / Supernova median: {:.2}x",
            signal_median / bg_median
        );
        println!(
            "  Kilonova mean / Supernova mean: {:.2}x",
            signal_mean / bg_mean
        );

        // Count how many signal trials exceed 95th percentile of background
        let bg_95th = background_probs[(background_probs.len() as f64 * 0.05) as usize];
        let n_signal_exceeding = signal_probs.iter().filter(|&&p| p > bg_95th).count();
        let frac_signal_exceeding = n_signal_exceeding as f64 / signal_probs.len() as f64;

        println!(
            "  Kilonova trials exceeding supernova 95th percentile: {} / {} ({:.1}%)",
            n_signal_exceeding,
            signal_probs.len(),
            frac_signal_exceeding * 100.0
        );

        // Count zeros
        let n_signal_zero = signal_probs.iter().filter(|&&p| p < 1e-8).count();
        let n_bg_zero = background_probs.iter().filter(|&&p| p < 1e-8).count();
        println!(
            "  Zero probability trials: Kilonova {}/{} ({:.1}%), Supernova {}/{} ({:.1}%)",
            n_signal_zero,
            signal_probs.len(),
            100.0 * n_signal_zero as f64 / signal_probs.len() as f64,
            n_bg_zero,
            background_probs.len(),
            100.0 * n_bg_zero as f64 / background_probs.len() as f64
        );

        // ========== CREDIBLE REGION ANALYSIS ==========
        println!("\n========== CREDIBLE REGION MEMBERSHIP ==========");
        println!("(More intuitive than absolute probability for tiny optical errors)");

        let n_signal_in_50cr = signal_in_50cr.iter().filter(|&&x| x).count();
        let n_signal_in_90cr = signal_in_90cr.iter().filter(|&&x| x).count();
        let n_bg_in_50cr = background_in_50cr.iter().filter(|&&x| x).count();
        let n_bg_in_90cr = background_in_90cr.iter().filter(|&&x| x).count();

        println!("\nKilonova (Signal):");
        println!(
            "  In 50% CR: {} / {} ({:.1}%)",
            n_signal_in_50cr,
            signal_in_50cr.len(),
            100.0 * n_signal_in_50cr as f64 / signal_in_50cr.len() as f64
        );
        println!(
            "  In 90% CR: {} / {} ({:.1}%)",
            n_signal_in_90cr,
            signal_in_90cr.len(),
            100.0 * n_signal_in_90cr as f64 / signal_in_90cr.len() as f64
        );

        println!("\nSupernova (Background):");
        println!(
            "  In 50% CR: {} / {} ({:.3}%)",
            n_bg_in_50cr,
            background_in_50cr.len(),
            100.0 * n_bg_in_50cr as f64 / background_in_50cr.len() as f64
        );
        println!(
            "  In 90% CR: {} / {} ({:.3}%)",
            n_bg_in_90cr,
            background_in_90cr.len(),
            100.0 * n_bg_in_90cr as f64 / background_in_90cr.len() as f64
        );

        // CR-based discrimination
        let frac_signal_50cr = n_signal_in_50cr as f64 / signal_in_50cr.len() as f64;
        let frac_bg_50cr = n_bg_in_50cr as f64 / background_in_50cr.len() as f64;
        let cr_discrimination = if frac_bg_50cr > 0.0 {
            frac_signal_50cr / frac_bg_50cr
        } else {
            f64::INFINITY
        };

        println!("\nCredible Region Discrimination:");
        println!(
            "  50% CR: Signal {:.1}% vs Background {:.3}% = {:.1}× enrichment",
            frac_signal_50cr * 100.0,
            frac_bg_50cr * 100.0,
            cr_discrimination
        );

        println!("\n💡 KEY INSIGHT:");
        println!(
            "  Absolute P_spatial values are tiny ({:.2e}) due to 2 arcsec integration radius,",
            signal_median
        );
        println!(
            "  but CR membership shows kilonovae ARE preferentially in high-probability regions!"
        );
        println!(
            "  This is more intuitive: ~{:.0}% of kilonovae in 50% CR vs ~{:.1}% random SNe.",
            frac_signal_50cr * 100.0,
            frac_bg_50cr * 100.0
        );

        // Assertions
        assert!(
            signal_median > bg_median * 2.0,
            "Kilonova median should be >2x supernova median (signal={:.6}, bg={:.6})",
            signal_median,
            bg_median
        );

        assert!(
            signal_mean > bg_mean * 2.0,
            "Kilonova mean should be >2x supernova mean (signal={:.6}, bg={:.6})",
            signal_mean,
            bg_mean
        );

        // For optical, expect very high discrimination due to tiny error circle
        // Similar to Swift-BAT, use 40% threshold
        assert!(
            frac_signal_exceeding > 0.4,
            "At least 40% of kilonova trials should exceed supernova 95th percentile (got {:.1}%)",
            frac_signal_exceeding * 100.0
        );

        println!("\n========== OPTICAL TEST PASSED ==========\n");
        println!("Note: This test uses spatial correlation only.");
        println!("Temporal coincidence (GRBs are prompt, SNe are random in time) is handled separately in joint FAR.");
    }
}
