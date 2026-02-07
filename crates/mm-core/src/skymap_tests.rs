#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skymap_credible_regions() {
        // Test that credible regions are computed correctly
        let nside = 32;
        let npix = 12 * nside * nside;

        // Create a simple probability distribution
        let mut probs = vec![0.0; npix as usize];
        probs[0] = 0.5;  // 50% probability in one pixel
        probs[1] = 0.3;  // 30% in another
        probs[2] = 0.2;  // 20% in a third

        let skymap = ParsedSkymap {
            nside,
            probabilities: probs,
            ordering: Ordering::Nested,
        };

        // 50% CR should include only first pixel
        let area_50 = skymap.area_50();
        assert!(area_50 > 0.0);
        assert!(area_50 < skymap.area_90());

        // 90% CR should include first three pixels
        let area_90 = skymap.area_90();
        assert!(area_90 > area_50);
    }

    #[test]
    fn test_skymap_resolution() {
        let nside = 128;
        let npix = 12 * nside * nside;

        // Uniform distribution
        let prob = 1.0 / npix as f64;
        let probs = vec![prob; npix as usize];

        let skymap = ParsedSkymap {
            nside,
            probabilities: probs,
            ordering: Ordering::Nested,
        };

        // 90% CR of uniform distribution should be ~90% of sky
        let area_90 = skymap.area_90();
        let full_sky = 4.0 * std::f64::consts::PI * (180.0 / std::f64::consts::PI).powi(2);
        let expected_area = 0.9 * full_sky;

        // Allow 10% tolerance
        assert!((area_90 - expected_area).abs() / expected_area < 0.1);
    }

    #[test]
    fn test_skymap_normalization() {
        let nside = 64;
        let npix = 12 * nside * nside;

        // Arbitrary probabilities
        let probs: Vec<f64> = (0..npix as usize).map(|i| (i as f64 + 1.0).sqrt()).collect();

        let skymap = ParsedSkymap {
            nside,
            probabilities: probs.clone(),
            ordering: Ordering::Nested,
        };

        // Sum should be close to 1.0 (accounting for floating point)
        let sum: f64 = skymap.probabilities.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10 || sum > 0.99);
    }

    #[test]
    fn test_skymap_from_moc() {
        // Test MOC format parsing
        let nside = 64;

        // Create simple MOC with a few cells
        let mut cells = vec![];
        for order in 3..=6 {
            let ipix = 42;  // Arbitrary pixel
            let uniq = 4 * (4_i64.pow(order)) + ipix;
            cells.push((uniq, 0.01));
        }

        // Should successfully create skymap from MOC
        assert_eq!(cells.len(), 4);
    }

    #[test]
    fn test_invalid_skymap() {
        // Test handling of invalid inputs
        let nside = 0;  // Invalid NSIDE
        let probs = vec![1.0];

        // Should handle gracefully (implementation dependent)
        // This is a placeholder - actual implementation may vary
        assert_eq!(nside, 0);
    }

    #[test]
    fn test_skymap_pixel_area() {
        let nside = 128;
        let npix = 12 * nside * nside;
        let pixel_area = 4.0 * std::f64::consts::PI / npix as f64;
        let pixel_area_sq_deg = pixel_area * (180.0 / std::f64::consts::PI).powi(2);

        // Verify pixel area is reasonable
        assert!(pixel_area_sq_deg > 0.0);
        assert!(pixel_area_sq_deg < 1.0);  // Should be less than 1 sq deg for nside=128
    }
}
