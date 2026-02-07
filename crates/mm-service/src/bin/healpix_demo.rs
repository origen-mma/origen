use anyhow::Result;
use mm_core::{CredibleRegion, ParsedSkymap, SkyPosition, SkymapOrdering};
use mm_correlator::spatial::{
    calculate_skymap_offset, calculate_spatial_probability_from_skymap,
    calculate_spatial_significance, is_in_credible_region,
};
use tracing::info;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("=== HEALPix Skymap Parsing & Spatial Correlation Demo ===\n");

    // Create a mock parsed skymap to demonstrate the API
    // In production, this would come from ParsedSkymap::from_fits("path/to/skymap.fits")
    let mock_skymap = create_mock_skymap();

    info!("📊 Mock Skymap Information:");
    info!("  NSIDE: {}", mock_skymap.nside);
    info!("  Ordering: {:?}", mock_skymap.ordering);
    info!("  Total pixels: {}", mock_skymap.probabilities.len());
    info!(
        "  Max probability position: (RA={:.2}°, Dec={:.2}°)",
        mock_skymap.max_prob_position.ra, mock_skymap.max_prob_position.dec
    );
    info!("  Total sky area: {:.2} sq deg", mock_skymap.total_area);
    info!("  50% CR area: {:.2} sq deg", mock_skymap.area_50());
    info!("  90% CR area: {:.2} sq deg\n", mock_skymap.area_90());

    // Test optical transient positions at various locations
    let test_positions = vec![
        ("Center (max prob)", SkyPosition::new(180.0, 45.0, 2.0)),
        ("Close (0.5° offset)", SkyPosition::new(180.5, 45.0, 2.0)),
        ("Medium (2° offset)", SkyPosition::new(182.0, 45.0, 2.0)),
        ("Far (5° offset)", SkyPosition::new(185.0, 45.0, 2.0)),
        ("Very far (10° offset)", SkyPosition::new(190.0, 45.0, 2.0)),
    ];

    info!("🔬 Testing Spatial Correlation at Various Positions:\n");

    for (label, position) in &test_positions {
        info!(
            "Position: {} (RA={:.2}°, Dec={:.2}°)",
            label, position.ra, position.dec
        );

        // Query probability at this position
        let prob = calculate_spatial_probability_from_skymap(position, &mock_skymap);
        info!("  Probability: {:.6}", prob);

        // Check credible region membership
        let in_50cr = is_in_credible_region(position, &mock_skymap, 0.5);
        let in_90cr = is_in_credible_region(position, &mock_skymap, 0.9);
        info!("  In 50% CR: {}", if in_50cr { "✅ Yes" } else { "❌ No" });
        info!("  In 90% CR: {}", if in_90cr { "✅ Yes" } else { "❌ No" });

        // Calculate spatial significance (boosted if in CR)
        let significance = calculate_spatial_significance(position, &mock_skymap);
        info!("  Spatial significance: {:.6}", significance);

        // Get detailed offset information
        let offset = calculate_skymap_offset(position, &mock_skymap);
        info!("  Angular separation: {:.2}°", offset.angular_separation);
        info!("  Raw probability: {:.6}\n", offset.probability);
    }

    info!("✅ Enhanced Spatial Correlation Features:");
    info!("  ✓ Skymap-based probability queries");
    info!("  ✓ Credible region membership checks");
    info!("  ✓ Spatial significance scoring");
    info!("  ✓ Detailed offset calculations");

    info!("\n📝 Next Steps:");
    info!("  1. Download real Fermi/LIGO FITS skymaps");
    info!("  2. Parse with ParsedSkymap::from_fits(path)");
    info!("  3. Use in correlator for accurate spatial matching");

    Ok(())
}

/// Create a mock skymap for demonstration
/// In production, this comes from ParsedSkymap::from_fits("path/to/skymap.fits")
fn create_mock_skymap() -> ParsedSkymap {
    let nside = 128;
    let npix = 12 * nside * nside;

    // Create Gaussian-like probability distribution centered at (RA=180, Dec=45)
    let center_ra = 180.0_f64;
    let center_dec = 45.0_f64;
    let sigma = 2.0; // 2 degree width

    let mut probabilities = Vec::with_capacity(npix as usize);

    for i in 0..npix {
        // Simple approximation of pixel position
        let theta = (i as f64 / npix as f64) * std::f64::consts::PI;
        let phi = ((i % (4 * nside)) as f64 / (4.0 * nside as f64)) * 2.0 * std::f64::consts::PI;

        let dec = 90.0 - theta.to_degrees();
        let ra = phi.to_degrees();

        // Calculate angular separation from center
        let delta_ra = (ra - center_ra).abs();
        let delta_dec = (dec - center_dec).abs();
        let separation = (delta_ra * delta_ra + delta_dec * delta_dec).sqrt();

        // Gaussian probability
        let prob = (-0.5 * (separation / sigma).powi(2)).exp();

        probabilities.push(prob);
    }

    // Normalize
    let sum: f64 = probabilities.iter().sum();
    for p in &mut probabilities {
        *p /= sum;
    }

    // Sort indices by probability for credible regions
    let mut indexed_probs: Vec<(usize, f64)> = probabilities
        .iter()
        .enumerate()
        .map(|(i, &p)| (i, p))
        .collect();
    indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Calculate credible regions
    let mut cumulative = 0.0;
    let mut pixels_50 = Vec::new();
    let mut pixels_90 = Vec::new();

    for &(idx, prob) in &indexed_probs {
        cumulative += prob;
        if cumulative <= 0.5 {
            pixels_50.push(idx);
        }
        if cumulative <= 0.9 {
            pixels_90.push(idx);
        }
    }

    let pixel_area = 4.0 * std::f64::consts::PI / (npix as f64);
    let area_50 = (pixels_50.len() as f64) * pixel_area * (180.0 / std::f64::consts::PI).powi(2);
    let area_90 = (pixels_90.len() as f64) * pixel_area * (180.0 / std::f64::consts::PI).powi(2);

    let credible_regions = vec![
        CredibleRegion {
            level: 0.5,
            area: area_50,
            pixel_indices: pixels_50,
        },
        CredibleRegion {
            level: 0.9,
            area: area_90,
            pixel_indices: pixels_90,
        },
    ];

    ParsedSkymap {
        probabilities,
        nside,
        ordering: SkymapOrdering::Nested,
        credible_regions,
        max_prob_position: SkyPosition::new(center_ra, center_dec, 1.0),
        total_area: 4.0 * std::f64::consts::PI * (180.0 / std::f64::consts::PI).powi(2),
    }
}
