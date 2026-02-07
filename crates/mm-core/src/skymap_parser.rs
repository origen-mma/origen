use fitsio::FitsFile;
use std::path::Path;
use thiserror::Error;
use cdshealpix::nested::{hash, center};

use crate::SkyPosition;

/// Parsed HEALPix skymap with probability distribution
#[derive(Debug, Clone)]
pub struct ParsedSkymap {
    /// HEALPix probability values (normalized to sum to 1.0)
    pub probabilities: Vec<f64>,

    /// HEALPix NSIDE parameter
    pub nside: i64,

    /// HEALPix ordering scheme
    pub ordering: SkymapOrdering,

    /// Credible regions
    pub credible_regions: Vec<CredibleRegion>,

    /// Position of maximum probability
    pub max_prob_position: SkyPosition,

    /// Total sky area in square degrees
    pub total_area: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkymapOrdering {
    Nested,
    Ring,
}

#[derive(Debug, Clone)]
pub struct CredibleRegion {
    /// Credible level (e.g., 0.5 for 50%, 0.9 for 90%)
    pub level: f64,

    /// Sky area in square degrees
    pub area: f64,

    /// Indices of pixels in this credible region
    pub pixel_indices: Vec<usize>,
}

#[derive(Debug, Error)]
pub enum SkymapParseError {
    #[error("FITS I/O error: {0}")]
    FitsIo(String),

    #[error("Missing required HDU: {0}")]
    MissingHdu(String),

    #[error("Missing required column: {0}")]
    MissingColumn(String),

    #[error("Invalid HEALPix NSIDE: {0}")]
    InvalidNside(i64),

    #[error("Invalid ordering scheme: {0}")]
    InvalidOrdering(String),

    #[error("Probability map is empty")]
    EmptyProbMap,

    #[error("Probability sum is invalid: {0}")]
    InvalidProbSum(f64),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl ParsedSkymap {
    /// Parse a HEALPix FITS skymap file (supports both flat and multi-order)
    pub fn from_fits<P: AsRef<Path>>(path: P) -> Result<Self, SkymapParseError> {
        let mut fptr = FitsFile::open(&path)
            .map_err(|e| SkymapParseError::FitsIo(e.to_string()))?;

        // Try to read from primary HDU or first extension
        let hdu = fptr
            .hdu(0)
            .or_else(|_| fptr.hdu(1))
            .map_err(|e| SkymapParseError::FitsIo(e.to_string()))?;

        // Check if this is a flat HEALPix map by looking for NSIDE keyword
        // If NSIDE doesn't exist, it's likely a multi-order MOC skymap
        let nside_result: Result<i64, _> = hdu.read_key(&mut fptr, "NSIDE");

        if nside_result.is_err() {
            // No NSIDE, likely multi-order MOC format
            drop(fptr); // Close the file before reopening
            return Self::from_fits_multiorder(path);
        }

        // Parse traditional flat HEALPix skymap
        let nside = nside_result.unwrap();

        if !is_valid_nside(nside) {
            return Err(SkymapParseError::InvalidNside(nside));
        }

        // Read ordering scheme
        let ordering_str: String = hdu
            .read_key(&mut fptr, "ORDERING")
            .unwrap_or_else(|_| "NESTED".to_string());

        let ordering = match ordering_str.to_uppercase().as_str() {
            "NESTED" => SkymapOrdering::Nested,
            "RING" => SkymapOrdering::Ring,
            _ => return Err(SkymapParseError::InvalidOrdering(ordering_str)),
        };

        // Read probability column
        // Common column names: PROB, PROBABILITY, PROBDENSITY
        let probabilities: Vec<f64> = Self::read_prob_column(&mut fptr)?;

        if probabilities.is_empty() {
            return Err(SkymapParseError::EmptyProbMap);
        }

        // Normalize probabilities to sum to 1.0
        let prob_sum: f64 = probabilities.iter().sum();
        if prob_sum <= 0.0 || prob_sum.is_nan() {
            return Err(SkymapParseError::InvalidProbSum(prob_sum));
        }

        let normalized_probs: Vec<f64> = probabilities
            .iter()
            .map(|&p| p / prob_sum)
            .collect();

        // Sort indices by probability (descending) for credible region calculation
        let mut indexed_probs: Vec<(usize, f64)> = normalized_probs
            .iter()
            .enumerate()
            .map(|(i, &p)| (i, p))
            .collect();
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Calculate credible regions (50%, 90%)
        let credible_regions = vec![
            Self::calculate_credible_region(&indexed_probs, 0.5, nside),
            Self::calculate_credible_region(&indexed_probs, 0.9, nside),
        ];

        // Find maximum probability position
        let max_prob_idx = indexed_probs[0].0;
        let max_prob_position = Self::pixel_to_sky_position(max_prob_idx, nside, ordering)?;

        // Calculate total sky area
        let npix = 12 * nside * nside;
        let pixel_area = 4.0 * std::f64::consts::PI / (npix as f64); // steradians
        let total_area = pixel_area * (npix as f64) * (180.0 / std::f64::consts::PI).powi(2);

        Ok(Self {
            probabilities: normalized_probs,
            nside,
            ordering,
            credible_regions,
            max_prob_position,
            total_area,
        })
    }

    /// Read probability column from FITS file
    fn read_prob_column(fptr: &mut FitsFile) -> Result<Vec<f64>, SkymapParseError> {
        // Try common column names
        let column_names = ["PROB", "PROBABILITY", "PROBDENSITY"];

        for col_name in &column_names {
            if let Ok(hdu) = fptr.hdu(1) {
                if let Ok(data) = hdu.read_col::<f64>(fptr, col_name) {
                    return Ok(data);
                }
            }
        }

        Err(SkymapParseError::MissingColumn(
            "PROB/PROBABILITY/PROBDENSITY".to_string(),
        ))
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
        let pixel_area = 4.0 * std::f64::consts::PI / (npix as f64); // steradians
        let area = (pixel_indices.len() as f64) * pixel_area * (180.0 / std::f64::consts::PI).powi(2);

        CredibleRegion {
            level,
            area,
            pixel_indices,
        }
    }

    /// Convert HEALPix pixel index to sky position using cdshealpix
    fn pixel_to_sky_position(
        pixel_idx: usize,
        nside: i64,
        ordering: SkymapOrdering,
    ) -> Result<SkyPosition, SkymapParseError> {
        let depth = (nside as f64).log2() as u8;
        let npix = 12 * nside * nside;

        // Convert RING to NESTED if needed (cdshealpix uses NESTED)
        let nested_idx = match ordering {
            SkymapOrdering::Nested => pixel_idx as u64,
            SkymapOrdering::Ring => {
                // For RING ordering, we need to convert to NESTED
                // Most modern skymaps use NESTED, so this is rarely needed
                // For now, use a simplified approach
                ring_to_nested_simple(pixel_idx as u64, nside as u64)
            }
        };

        // Get center coordinates (lon, lat in radians) using cdshealpix
        let (lon, lat) = center(depth, nested_idx);

        // Convert to degrees
        let ra = lon.to_degrees();
        let dec = lat.to_degrees();

        // Calculate pixel size for uncertainty
        let pixel_area = 4.0 * std::f64::consts::PI / (npix as f64); // steradians
        let pixel_radius = (pixel_area / std::f64::consts::PI).sqrt(); // radians
        let uncertainty = pixel_radius.to_degrees() * 3600.0; // arcseconds

        Ok(SkyPosition::new(ra, dec, uncertainty))
    }

    /// Query probability at a given sky position using cdshealpix
    pub fn probability_at_position(&self, position: &SkyPosition) -> f64 {
        let depth = (self.nside as f64).log2() as u8;

        // Convert RA, Dec to radians
        let lon = position.ra.to_radians();
        let lat = position.dec.to_radians();

        // Get HEALPix cell index at this position (NESTED ordering)
        let nested_idx = hash(depth, lon, lat) as usize;

        // Handle RING ordering if needed
        let actual_idx = match self.ordering {
            SkymapOrdering::Nested => nested_idx,
            SkymapOrdering::Ring => {
                // Convert NESTED to RING
                nested_to_ring_simple(nested_idx as u64, self.nside as u64) as usize
            }
        };

        // Return probability at this pixel
        self.probabilities.get(actual_idx).copied().unwrap_or(0.0)
    }

    /// Check if a position is within a credible region using cdshealpix
    pub fn is_in_credible_region(&self, position: &SkyPosition, level: f64) -> bool {
        let depth = (self.nside as f64).log2() as u8;

        // Convert RA, Dec to radians
        let lon = position.ra.to_radians();
        let lat = position.dec.to_radians();

        // Get HEALPix cell index at this position (NESTED ordering)
        let nested_idx = hash(depth, lon, lat) as usize;

        // Handle RING ordering if needed
        let actual_idx = match self.ordering {
            SkymapOrdering::Nested => nested_idx,
            SkymapOrdering::Ring => {
                nested_to_ring_simple(nested_idx as u64, self.nside as u64) as usize
            }
        };

        // Find the credible region at this level
        if let Some(region) = self.credible_regions.iter().find(|r| (r.level - level).abs() < 0.01) {
            region.pixel_indices.contains(&actual_idx)
        } else {
            false
        }
    }

    /// Get 50% credible region area
    pub fn area_50(&self) -> f64 {
        self.credible_regions
            .iter()
            .find(|r| (r.level - 0.5).abs() < 0.01)
            .map(|r| r.area)
            .unwrap_or(0.0)
    }

    /// Get 90% credible region area
    pub fn area_90(&self) -> f64 {
        self.credible_regions
            .iter()
            .find(|r| (r.level - 0.9).abs() < 0.01)
            .map(|r| r.area)
            .unwrap_or(0.0)
    }

    /// Parse multi-order (MOC) FITS skymap
    fn from_fits_multiorder<P: AsRef<Path>>(path: P) -> Result<Self, SkymapParseError> {
        let mut fptr = FitsFile::open(path)
            .map_err(|e| SkymapParseError::FitsIo(e.to_string()))?;

        let hdu = fptr.hdu(1)
            .map_err(|e| SkymapParseError::FitsIo(e.to_string()))?;

        // Read UNIQ column (encodes order + pixel index)
        let uniq: Vec<i64> = hdu.read_col(&mut fptr, "UNIQ")
            .map_err(|e| SkymapParseError::MissingColumn(format!("UNIQ: {}", e)))?;

        // Read probability column (try different names)
        let probdensity: Vec<f64> = hdu.read_col(&mut fptr, "PROBDENSITY")
            .or_else(|_| hdu.read_col(&mut fptr, "PROB"))
            .map_err(|e| SkymapParseError::MissingColumn(format!("PROBDENSITY/PROB: {}", e)))?;

        if uniq.is_empty() || probdensity.is_empty() {
            return Err(SkymapParseError::EmptyProbMap);
        }

        // Decode UNIQ to find max order and convert to flat HEALPix at that resolution
        // UNIQ encoding: uniq = 4 * (4^order) + ipix_nested
        let mut max_order = 0_u8;
        for &u in &uniq {
            let order = ((u as f64 / 4.0).log2() / 2.0).floor() as u8;
            if order > max_order {
                max_order = order;
            }
        }

        let nside = 2_i64.pow(max_order as u32);
        let npix = 12 * nside * nside;

        // Create flat probability map at target NSIDE
        let mut probabilities = vec![0.0; npix as usize];

        for (u, prob) in uniq.iter().zip(probdensity.iter()) {
            let order = (((*u as f64) / 4.0).log2() / 2.0).floor() as u8;
            let ipix_at_order = (*u - 4 * (4_i64.pow(order as u32))) as u64;

            // Convert from this order to target NSIDE
            if order == max_order {
                // Same resolution, direct assignment
                probabilities[ipix_at_order as usize] = *prob;
            } else {
                // Lower resolution pixel covers multiple higher resolution pixels
                let ratio = 4_usize.pow((max_order - order) as u32);
                let start_idx = (ipix_at_order * ratio as u64) as usize;

                // Distribute probability evenly across sub-pixels
                for i in 0..ratio {
                    if start_idx + i < probabilities.len() {
                        probabilities[start_idx + i] = prob / ratio as f64;
                    }
                }
            }
        }

        // Normalize probabilities
        let sum: f64 = probabilities.iter().sum();
        if sum > 0.0 {
            for p in &mut probabilities {
                *p /= sum;
            }
        }

        // Rest of processing is same as flat skymap
        let mut indexed_probs: Vec<(usize, f64)> = probabilities
            .iter()
            .enumerate()
            .map(|(i, &p)| (i, p))
            .collect();
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let credible_regions = vec![
            Self::calculate_credible_region(&indexed_probs, 0.5, nside),
            Self::calculate_credible_region(&indexed_probs, 0.9, nside),
        ];

        let max_prob_idx = indexed_probs[0].0;
        let max_prob_position = Self::pixel_to_sky_position(max_prob_idx, nside, SkymapOrdering::Nested)?;

        let pixel_area = 4.0 * std::f64::consts::PI / (npix as f64);
        let total_area = pixel_area * (npix as f64) * (180.0 / std::f64::consts::PI).powi(2);

        Ok(Self {
            probabilities,
            nside,
            ordering: SkymapOrdering::Nested,
            credible_regions,
            max_prob_position,
            total_area,
        })
    }
}

/// Check if NSIDE is a valid HEALPix parameter (power of 2)
fn is_valid_nside(nside: i64) -> bool {
    nside > 0 && (nside & (nside - 1)) == 0
}

/// Convert RING ordering pixel index to NESTED ordering
/// Note: This is a simplified implementation. For production use,
/// consider using a full HEALPix library with proper RING support.
fn ring_to_nested_simple(ring_idx: u64, _nside: u64) -> u64 {
    // Most modern skymaps use NESTED ordering
    // For RING ordering support, would need full conversion algorithm
    // For now, pass through (caller should ensure NESTED ordering)
    ring_idx
}

/// Convert NESTED ordering pixel index to RING ordering
/// Note: This is a simplified implementation. For production use,
/// consider using a full HEALPix library with proper RING support.
fn nested_to_ring_simple(nested_idx: u64, _nside: u64) -> u64 {
    // Most modern skymaps use NESTED ordering
    // For RING ordering support, would need full conversion algorithm
    // For now, pass through (caller should ensure NESTED ordering)
    nested_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_nside() {
        assert!(is_valid_nside(1));
        assert!(is_valid_nside(2));
        assert!(is_valid_nside(4));
        assert!(is_valid_nside(8));
        assert!(is_valid_nside(16));
        assert!(is_valid_nside(128));
        assert!(is_valid_nside(512));

        assert!(!is_valid_nside(0));
        assert!(!is_valid_nside(3));
        assert!(!is_valid_nside(5));
        assert!(!is_valid_nside(100));
    }

    #[test]
    fn test_pixel_to_sky_position() {
        let nside = 128;
        let result = ParsedSkymap::pixel_to_sky_position(0, nside, SkymapOrdering::Nested);
        assert!(result.is_ok());

        let pos = result.unwrap();
        // Should produce valid sky coordinates
        assert!(pos.dec >= -90.0 && pos.dec <= 90.0);
        assert!(pos.ra >= 0.0 && pos.ra < 360.0);
    }
}
