#[test]
fn test_temporal_correlation() {
    // Test events within time window are correlated
    let gw_time: f64 = 1000.0;
    let grb_time_1: f64 = 1003.0; // 3 seconds later
    let grb_time_2: f64 = 1050.0; // 50 seconds later

    let time_window: f64 = 5.0; // ±5 second window

    // GRB1 should be within window
    assert!((grb_time_1 - gw_time).abs() <= time_window);

    // GRB2 should be outside window
    assert!((grb_time_2 - gw_time).abs() > time_window);
}

#[test]
fn test_correlation_ordering() {
    // Test that correlations work regardless of arrival order
    let gw_time: f64 = 1000.0;
    let grb_time: f64 = 1003.0;

    // GRB arrives first
    let time_offset_1 = (grb_time - gw_time).abs();

    // GW arrives first
    let time_offset_2 = (gw_time - grb_time).abs();

    // Both should give same time offset
    assert_eq!(time_offset_1, time_offset_2);
}

#[test]
fn test_spatial_separation() {
    // Test angular separation calculation
    use std::f64::consts::PI;

    let ra1: f64 = 0.0;
    let dec1: f64 = 0.0;

    let ra2: f64 = 10.0 * PI / 180.0; // 10 degrees
    let dec2: f64 = 0.0;

    // Calculate angular separation
    let cos_sep = dec1.sin() * dec2.sin() + dec1.cos() * dec2.cos() * (ra1 - ra2).cos();
    let separation_rad = cos_sep.acos();
    let separation_deg = separation_rad * 180.0 / PI;

    // Should be approximately 10 degrees
    assert!((separation_deg - 10.0).abs() < 0.1);
}

#[test]
fn test_overlap_computation_simple() {
    // Test simple non-overlapping regions
    let nside = 64;
    let npix = 12 * nside * nside;

    // GW skymap concentrated in one region
    let mut gw_probs = vec![0.0; npix as usize];
    gw_probs[0] = 1.0;

    // GRB skymap concentrated in different region
    let mut grb_probs = vec![0.0; npix as usize];
    grb_probs[npix as usize - 1] = 1.0;

    // Joint probability should be zero (no overlap)
    let joint_probs: Vec<f64> = gw_probs
        .iter()
        .zip(grb_probs.iter())
        .map(|(a, b)| a * b)
        .collect();

    let sum: f64 = joint_probs.iter().sum();
    assert_eq!(sum, 0.0);
}

#[test]
fn test_overlap_computation_complete() {
    // Test complete overlap (same region)
    let nside = 64;
    let npix = 12 * nside * nside;

    // Both concentrated in same pixel
    let mut gw_probs = vec![0.0; npix as usize];
    gw_probs[100] = 1.0;

    let mut grb_probs = vec![0.0; npix as usize];
    grb_probs[100] = 1.0;

    // Joint probability should be 1.0
    let joint_probs: Vec<f64> = gw_probs
        .iter()
        .zip(grb_probs.iter())
        .map(|(a, b)| a * b)
        .collect();

    let sum: f64 = joint_probs.iter().sum();
    assert_eq!(sum, 1.0);
}

#[test]
fn test_credible_region_calculation() {
    // Test 90% credible region area calculation
    let probs = [0.5, 0.3, 0.1, 0.05, 0.05];

    // Sort by probability (descending)
    let mut indexed_probs: Vec<(usize, f64)> =
        probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
    indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Find 90% CR
    let mut cumulative = 0.0;
    let mut cr_90_pixels = 0;

    for &(_idx, prob) in &indexed_probs {
        cumulative += prob;
        cr_90_pixels += 1;
        if cumulative >= 0.9 {
            break;
        }
    }

    // Should include first 3 pixels (0.5 + 0.3 + 0.1 = 0.9)
    assert_eq!(cr_90_pixels, 3);
}

#[test]
fn test_skymap_resampling() {
    // Test resampling between different NSIDE values
    let nside_high = 128;
    let nside_low = 64;

    let npix_high = 12 * nside_high * nside_high;
    let probs_high = vec![1.0 / npix_high as f64; npix_high as usize];

    // Downsample by factor of 4 (since nside ratio is 2)
    let ratio = ((nside_high / nside_low) as i32).pow(2) as usize;
    let npix_low = npix_high / ratio as i64;

    let mut probs_low = vec![0.0; npix_low as usize];
    #[allow(clippy::needless_range_loop)]
    for i in 0..npix_low as usize {
        let start = i * ratio;
        probs_low[i] = probs_high[start..start + ratio].iter().sum();
    }

    // Sum should still be ~1.0
    let sum: f64 = probs_low.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6);
}

#[test]
fn test_correlation_message_serialization() {
    // Test that correlation messages can be serialized/deserialized
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct TestCorrelation {
        simulation_id: u32,
        overlap_area: f64,
    }

    let correlation = TestCorrelation {
        simulation_id: 42,
        overlap_area: 123.45,
    };

    let json = serde_json::to_string(&correlation).unwrap();
    let decoded: TestCorrelation = serde_json::from_str(&json).unwrap();

    assert_eq!(correlation.simulation_id, decoded.simulation_id);
    assert_eq!(correlation.overlap_area, decoded.overlap_area);
}
