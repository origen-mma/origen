use crate::LightCurve;

/// Estimate explosion time from light curve
///
/// This uses a simple backwards extrapolation from the rising part of the light curve.
/// For more sophisticated methods, see the light curve fitting code in
/// /Users/mcoughlin/Code/ZTF/lightcurve-fitting/src
pub fn estimate_explosion_time(lc: &LightCurve) -> Option<f64> {
    if lc.measurements.is_empty() {
        return None;
    }

    // Sort by time
    let mut measurements = lc.measurements.clone();
    measurements.sort_by(|a, b| a.mjd.partial_cmp(&b.mjd).unwrap());

    // Find peak (brightest, lowest magnitude or highest flux)
    let peak_idx = measurements
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.flux
                .partial_cmp(&b.flux)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)?;

    // If peak is the first point, we can't extrapolate backwards
    if peak_idx == 0 {
        // Just use first detection time minus a typical rise time
        let first_detection = measurements[0].to_gps_time();
        // Typical fast transient rises in ~1 day
        return Some(first_detection - 86400.0);
    }

    // Get rising portion (before peak)
    let rising = &measurements[0..=peak_idx];

    if rising.len() < 2 {
        // Not enough data, use first detection minus typical rise time
        let first_detection = measurements[0].to_gps_time();
        return Some(first_detection - 86400.0);
    }

    // Fit linear rise: flux vs time
    // Convert to GPS times for fitting
    let times: Vec<f64> = rising.iter().map(|m| m.to_gps_time()).collect();
    let fluxes: Vec<f64> = rising.iter().map(|m| m.flux).collect();

    // Simple linear regression
    let n = times.len() as f64;
    let sum_t: f64 = times.iter().sum();
    let sum_f: f64 = fluxes.iter().sum();
    let sum_tt: f64 = times.iter().map(|t| t * t).sum();
    let sum_tf: f64 = times.iter().zip(fluxes.iter()).map(|(t, f)| t * f).sum();

    let denominator = n * sum_tt - sum_t * sum_t;
    if denominator.abs() < 1e-10 {
        // Can't fit, use first detection minus typical rise time
        let first_detection = measurements[0].to_gps_time();
        return Some(first_detection - 86400.0);
    }

    let slope = (n * sum_tf - sum_t * sum_f) / denominator;
    let intercept = (sum_f - slope * sum_t) / n;

    // Extrapolate back to zero flux (explosion time)
    // flux = slope * time + intercept
    // 0 = slope * t0 + intercept
    // t0 = -intercept / slope

    if slope.abs() < 1e-10 {
        // Slope too small, use first detection
        return Some(measurements[0].to_gps_time());
    }

    let t0 = -intercept / slope;

    // Sanity check: t0 should be before first detection
    let first_detection = measurements[0].to_gps_time();
    if t0 > first_detection {
        // Extrapolation went wrong, use first detection minus rise time
        // Estimate rise time from the data
        let rise_time = times[peak_idx] - times[0];
        return Some(first_detection - rise_time.max(3600.0)); // At least 1 hour
    }

    // Also check it's not too far in the past (more than 30 days)
    if first_detection - t0 > 30.0 * 86400.0 {
        // Too far back, use first detection minus typical rise time
        return Some(first_detection - 86400.0);
    }

    Some(t0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Photometry;

    #[test]
    fn test_explosion_time_rising_lightcurve() {
        let mut lc = LightCurve::new("test".to_string());

        // Create a rising light curve
        // MJD 60000 + days, linear rise from 100 to 1000 flux
        let mjd_start = 60000.0;
        for i in 0..10 {
            let mjd = mjd_start + i as f64;
            let flux = 100.0 + (i as f64) * 100.0;
            lc.add_measurement(Photometry::new(mjd, flux, 10.0, "r".to_string()));
        }

        let t0 = estimate_explosion_time(&lc);
        assert!(t0.is_some());

        let t0 = t0.unwrap();
        let first_detection_gps = lc.measurements[0].to_gps_time();

        // t0 should be before first detection
        assert!(t0 < first_detection_gps);

        // Should be reasonably close (within a few days)
        let diff_days = (first_detection_gps - t0) / 86400.0;
        assert!(
            diff_days < 5.0,
            "t0 is {} days before first detection",
            diff_days
        );
    }
}
