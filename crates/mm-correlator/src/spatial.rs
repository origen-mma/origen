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

/// Check if a position is within a credible region of a skymap
pub fn is_in_credible_region(position: &SkyPosition, skymap: &ParsedSkymap, level: f64) -> bool {
    skymap.is_in_credible_region(position, level)
}

/// Calculate spatial significance using skymap
/// Returns a score from 0-1 based on:
/// - Probability at position
/// - Whether it's within 50% or 90% credible region
pub fn calculate_spatial_significance(position: &SkyPosition, skymap: &ParsedSkymap) -> f64 {
    let prob = skymap.probability_at_position(position);

    // Boost significance if within credible regions
    let in_50cr = skymap.is_in_credible_region(position, 0.5);
    let in_90cr = skymap.is_in_credible_region(position, 0.9);

    let base_score = prob;

    if in_50cr {
        // Within 50% CR: very significant
        (base_score * 2.0).min(1.0)
    } else if in_90cr {
        // Within 90% CR: significant
        (base_score * 1.5).min(1.0)
    } else {
        // Outside 90% CR: use raw probability
        base_score
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
}
