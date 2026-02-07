//! Spherical rotation for HEALPix skymaps
//!
//! Rotates a skymap from one position to another while preserving
//! the probability distribution shape.

use anyhow::Result;
use cdshealpix::nested::{center, hash};
use mm_core::{CredibleRegion, ParsedSkymap, SkyPosition, SkymapOrdering};
use std::f64::consts::PI;

/// Rotate a HEALPix skymap from source position to target position
///
/// This applies a spherical rotation that moves the maximum probability
/// position (or specified source position) to the target position,
/// preserving the shape and structure of the probability distribution.
pub fn rotate_skymap(
    skymap: &ParsedSkymap,
    source_ra: f64,
    source_dec: f64,
    target_ra: f64,
    target_dec: f64,
) -> Result<ParsedSkymap> {
    let depth = (skymap.nside as f64).log2() as u8;
    let npix = skymap.probabilities.len();

    // Calculate rotation matrix using ZYZ Euler angles
    let rotation = calculate_rotation_matrix(source_ra, source_dec, target_ra, target_dec);

    // Create new probability map
    let mut rotated_probs = vec![0.0; npix];

    // For each pixel in the original skymap, find where it maps to after rotation
    for (src_idx, &prob) in skymap.probabilities.iter().enumerate() {
        if prob == 0.0 {
            continue; // Skip zero probability pixels for efficiency
        }

        // Get source pixel center coordinates
        let (src_lon, src_lat) = center(depth, src_idx as u64);
        let src_ra = src_lon.to_degrees();
        let src_dec = src_lat.to_degrees();

        // Convert to Cartesian coordinates
        let src_vec = spherical_to_cartesian(src_ra, src_dec);

        // Apply rotation
        let rotated_vec = apply_rotation(&rotation, &src_vec);

        // Convert back to spherical
        let (rot_ra, rot_dec) = cartesian_to_spherical(&rotated_vec);

        // Find target pixel using cdshealpix hash
        let rot_lon = rot_ra.to_radians();
        let rot_lat = rot_dec.to_radians();
        let tgt_idx = hash(depth, rot_lon, rot_lat) as usize;

        // Accumulate probability (handles overlaps from rotation)
        if tgt_idx < rotated_probs.len() {
            rotated_probs[tgt_idx] += prob;
        }
    }

    // Normalize probabilities
    let sum: f64 = rotated_probs.iter().sum();
    if sum > 0.0 {
        for p in &mut rotated_probs {
            *p /= sum;
        }
    }

    // Recalculate credible regions for rotated skymap
    let mut indexed_probs: Vec<(usize, f64)> = rotated_probs
        .iter()
        .enumerate()
        .map(|(i, &p)| (i, p))
        .collect();
    indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let credible_regions = vec![
        calculate_credible_region(&indexed_probs, 0.5, skymap.nside),
        calculate_credible_region(&indexed_probs, 0.9, skymap.nside),
    ];

    // Find maximum probability position in rotated skymap
    let max_prob_idx = indexed_probs[0].0;
    let (max_lon, max_lat) = center(depth, max_prob_idx as u64);
    let max_prob_position = SkyPosition::new(max_lon.to_degrees(), max_lat.to_degrees(), 1.0);

    Ok(ParsedSkymap {
        probabilities: rotated_probs,
        nside: skymap.nside,
        ordering: SkymapOrdering::Nested,
        credible_regions,
        max_prob_position,
        total_area: skymap.total_area,
    })
}

/// Calculate rotation matrix to move from source to target position
fn calculate_rotation_matrix(
    src_ra: f64,
    src_dec: f64,
    tgt_ra: f64,
    tgt_dec: f64,
) -> [[f64; 3]; 3] {
    // Convert to radians
    let src_ra_rad = src_ra * PI / 180.0;
    let src_dec_rad = src_dec * PI / 180.0;
    let tgt_ra_rad = tgt_ra * PI / 180.0;
    let tgt_dec_rad = tgt_dec * PI / 180.0;

    // Source and target vectors
    let src = spherical_to_cartesian(src_ra, src_dec);
    let tgt = spherical_to_cartesian(tgt_ra, tgt_dec);

    // Calculate rotation axis (cross product)
    let axis = [
        src[1] * tgt[2] - src[2] * tgt[1],
        src[2] * tgt[0] - src[0] * tgt[2],
        src[0] * tgt[1] - src[1] * tgt[0],
    ];

    // Normalize axis
    let axis_len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();

    if axis_len < 1e-10 {
        // Source and target are the same (or opposite), return identity
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }

    let axis_norm = [axis[0] / axis_len, axis[1] / axis_len, axis[2] / axis_len];

    // Calculate rotation angle (dot product)
    let cos_angle = src[0] * tgt[0] + src[1] * tgt[1] + src[2] * tgt[2];
    let angle = cos_angle.acos();

    // Rodrigues' rotation formula
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let one_minus_cos = 1.0 - cos_a;

    let ux = axis_norm[0];
    let uy = axis_norm[1];
    let uz = axis_norm[2];

    [
        [
            cos_a + ux * ux * one_minus_cos,
            ux * uy * one_minus_cos - uz * sin_a,
            ux * uz * one_minus_cos + uy * sin_a,
        ],
        [
            uy * ux * one_minus_cos + uz * sin_a,
            cos_a + uy * uy * one_minus_cos,
            uy * uz * one_minus_cos - ux * sin_a,
        ],
        [
            uz * ux * one_minus_cos - uy * sin_a,
            uz * uy * one_minus_cos + ux * sin_a,
            cos_a + uz * uz * one_minus_cos,
        ],
    ]
}

/// Apply rotation matrix to a 3D vector
fn apply_rotation(matrix: &[[f64; 3]; 3], vec: &[f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * vec[0] + matrix[0][1] * vec[1] + matrix[0][2] * vec[2],
        matrix[1][0] * vec[0] + matrix[1][1] * vec[1] + matrix[1][2] * vec[2],
        matrix[2][0] * vec[0] + matrix[2][1] * vec[1] + matrix[2][2] * vec[2],
    ]
}

/// Convert spherical coordinates (RA, Dec in degrees) to Cartesian unit vector
fn spherical_to_cartesian(ra: f64, dec: f64) -> [f64; 3] {
    let ra_rad = ra * PI / 180.0;
    let dec_rad = dec * PI / 180.0;

    [
        dec_rad.cos() * ra_rad.cos(),
        dec_rad.cos() * ra_rad.sin(),
        dec_rad.sin(),
    ]
}

/// Convert Cartesian unit vector to spherical coordinates (RA, Dec in degrees)
fn cartesian_to_spherical(vec: &[f64; 3]) -> (f64, f64) {
    let dec_rad = vec[2].asin();
    let ra_rad = vec[1].atan2(vec[0]);

    let mut ra = ra_rad * 180.0 / PI;
    let dec = dec_rad * 180.0 / PI;

    // Ensure RA is in [0, 360)
    if ra < 0.0 {
        ra += 360.0;
    }

    (ra, dec)
}

/// Calculate credible region at given level
fn calculate_credible_region(
    indexed_probs: &[(usize, f64)],
    level: f64,
    nside: i64,
) -> CredibleRegion {
    let mut cumulative_prob = 0.0;
    let mut pixel_indices = Vec::new();

    for &(idx, prob) in indexed_probs {
        cumulative_prob += prob;
        pixel_indices.push(idx);

        if cumulative_prob >= level {
            break;
        }
    }

    // Calculate area
    let npix = 12 * nside * nside;
    let pixel_area = 4.0 * PI / (npix as f64); // steradians
    let area = (pixel_indices.len() as f64) * pixel_area * (180.0 / PI).powi(2);

    CredibleRegion {
        level,
        area,
        pixel_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spherical_cartesian_conversion() {
        let ra = 180.0;
        let dec = 45.0;

        let cart = spherical_to_cartesian(ra, dec);
        let (ra2, dec2) = cartesian_to_spherical(&cart);

        assert!((ra - ra2).abs() < 1e-10);
        assert!((dec - dec2).abs() < 1e-10);
    }

    #[test]
    fn test_rotation_identity() {
        // Rotating from a position to itself should be identity
        let matrix = calculate_rotation_matrix(100.0, 30.0, 100.0, 30.0);

        let vec = spherical_to_cartesian(150.0, 20.0);
        let rotated = apply_rotation(&matrix, &vec);

        assert!((vec[0] - rotated[0]).abs() < 1e-10);
        assert!((vec[1] - rotated[1]).abs() < 1e-10);
        assert!((vec[2] - rotated[2]).abs() < 1e-10);
    }
}
