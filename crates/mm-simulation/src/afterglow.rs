//! Afterglow emission from structured jets
//!
//! Models optical/X-ray afterglow emission using structured jet models.
//! Unlike the prompt GRB (which requires on-axis viewing), afterglows can be
//! visible from off-axis viewing angles due to:
//! 1. Angular structure of the jet (Gaussian or power-law profile)
//! 2. Deceleration and sideways expansion making emission visible at later times
//!
//! ## References
//! - Lamb & Kobayashi 2017: Gaussian structured jets
//! - Resmi & Zhang 2016: Off-axis afterglow emission
//! - Ryan et al. 2020: GRB170817A afterglow modeling

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Structured jet model type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum JetStructure {
    /// Top-hat jet (classic model, sharp cutoff)
    TopHat,

    /// Gaussian jet (smooth falloff)
    /// E(θ) = E_core * exp(-θ²/(2*θ_core²))
    Gaussian,

    /// Power-law jet
    /// E(θ) = E_core * (θ/θ_core)^(-k) for θ > θ_core
    PowerLaw { index: f64 },
}

/// Afterglow emission properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterglowProperties {
    /// Whether afterglow is detectable
    pub detectable: bool,

    /// Viewing angle (radians)
    pub theta_view: f64,

    /// Core jet angle (radians)
    pub theta_core: f64,

    /// Jet structure type
    pub jet_structure: JetStructure,

    /// Isotropic equivalent energy in jet core (erg)
    pub e_iso_core: f64,

    /// Effective isotropic energy at viewing angle (erg)
    pub e_iso_eff: f64,

    /// Initial Lorentz factor at viewing angle
    pub gamma_0_eff: f64,

    /// Peak time for optical afterglow (days)
    pub t_peak_optical: Option<f64>,

    /// Peak flux at optical (normalized, arbitrary units)
    pub flux_peak_optical: Option<f64>,

    /// Peak apparent magnitude (AB mag in R-band)
    pub peak_magnitude: Option<f64>,

    /// Distance to source (Mpc) - needed for magnitude calculation
    pub distance_mpc: f64,

    /// Afterglow visibility criterion
    /// (different from GRB - can be visible off-axis)
    pub visibility_fraction: f64,
}

/// Configuration for afterglow simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterglowConfig {
    /// Jet structure model
    pub jet_structure: JetStructure,

    /// Circumburst density (cm^-3)
    pub circumburst_density: f64,

    /// Electron energy fraction (ε_e)
    pub epsilon_e: f64,

    /// Magnetic field energy fraction (ε_B)
    pub epsilon_b: f64,

    /// Electron power-law index (p)
    pub electron_index: f64,

    /// Limiting magnitude for detection (AB mag in R-band)
    /// ZTF: ~21 mag, LSST: ~24.5 mag, DECam: ~23.5 mag
    pub limiting_magnitude: f64,
}

impl Default for AfterglowConfig {
    fn default() -> Self {
        Self {
            jet_structure: JetStructure::Gaussian,
            circumburst_density: 1e-3, // Typical for BNS mergers (low density)
            epsilon_e: 0.1,
            epsilon_b: 0.01,
            electron_index: 2.5,
            limiting_magnitude: 21.0, // ZTF-like sensitivity
        }
    }
}

impl AfterglowConfig {
    /// ZTF survey sensitivity (limiting mag ~21)
    pub fn ztf_survey() -> Self {
        Self {
            limiting_magnitude: 21.0,
            ..Default::default()
        }
    }

    /// LSST survey sensitivity (limiting mag ~24.5)
    pub fn lsst_survey() -> Self {
        Self {
            limiting_magnitude: 24.5,
            ..Default::default()
        }
    }

    /// DECam survey sensitivity (limiting mag ~23.5)
    pub fn decam_survey() -> Self {
        Self {
            limiting_magnitude: 23.5,
            ..Default::default()
        }
    }

    /// GW170817-like afterglow configuration
    pub fn gw170817_like() -> Self {
        Self {
            jet_structure: JetStructure::Gaussian,
            circumburst_density: 4e-4, // Low density environment
            epsilon_e: 0.15,
            epsilon_b: 0.003,
            electron_index: 2.16,
            limiting_magnitude: 21.0,
        }
    }

    /// Standard power-law jet
    pub fn power_law_jet(index: f64) -> Self {
        Self {
            jet_structure: JetStructure::PowerLaw { index },
            ..Default::default()
        }
    }
}

/// Simulate afterglow emission from a structured jet
///
/// # Arguments
///
/// * `theta_view` - Viewing angle (radians)
/// * `theta_core` - Core jet opening angle (radians)
/// * `e_iso_core` - Isotropic equivalent energy in core (erg)
/// * `distance_mpc` - Distance to source (Mpc)
/// * `config` - Afterglow configuration
///
/// # Returns
///
/// Afterglow properties including detectability and timing
pub fn simulate_afterglow(
    theta_view: f64,
    theta_core: f64,
    e_iso_core: f64,
    distance_mpc: f64,
    config: &AfterglowConfig,
) -> AfterglowProperties {
    // 1. Calculate effective energy at viewing angle
    let e_iso_eff = effective_energy(theta_view, theta_core, e_iso_core, &config.jet_structure);

    // 2. Calculate effective Lorentz factor
    let gamma_0_core = 100.0; // Typical initial Lorentz factor
    let gamma_0_eff =
        effective_lorentz_factor(theta_view, theta_core, gamma_0_core, &config.jet_structure);

    // 3. Calculate afterglow light curve properties
    let (t_peak, flux_peak) = if e_iso_eff > 0.0 {
        calculate_afterglow_peak(e_iso_eff, gamma_0_eff, theta_view, config)
    } else {
        (None, None)
    };

    // 4. Convert flux to apparent magnitude
    // Reference: ON-AXIS SGRB at 100 Mpc peaks at ~16 mag (M_opt ~ -19)
    // Off-axis (like GW170817A) are much fainter (~17-25 mag)
    let peak_mag =
        flux_peak.map(|flux| flux_to_magnitude(flux, distance_mpc, config.circumburst_density));

    // 5. Determine detectability by comparing to limiting magnitude
    // Fainter magnitudes = higher numbers, so detectable if mag < limiting_mag
    let detectable = peak_mag.map_or(false, |mag| mag < config.limiting_magnitude);

    // 6. Calculate visibility fraction (what fraction of viewing angles are detectable)
    let visibility_fraction = calculate_visibility_fraction(theta_core, &config.jet_structure);

    AfterglowProperties {
        detectable,
        theta_view,
        theta_core,
        jet_structure: config.jet_structure,
        e_iso_core,
        e_iso_eff,
        gamma_0_eff,
        t_peak_optical: t_peak,
        flux_peak_optical: flux_peak,
        peak_magnitude: peak_mag,
        distance_mpc,
        visibility_fraction,
    }
}

/// Calculate effective isotropic energy at viewing angle
fn effective_energy(
    theta_view: f64,
    theta_core: f64,
    e_iso_core: f64,
    structure: &JetStructure,
) -> f64 {
    match structure {
        JetStructure::TopHat => {
            if theta_view <= theta_core {
                e_iso_core
            } else {
                0.0 // No emission outside jet core
            }
        }

        JetStructure::Gaussian => {
            // E(θ) = E_core * exp(-θ²/(2*θ_core²))
            let ratio = theta_view / theta_core;
            e_iso_core * (-0.5 * ratio * ratio).exp()
        }

        JetStructure::PowerLaw { index } => {
            if theta_view <= theta_core {
                e_iso_core
            } else {
                // E(θ) = E_core * (θ/θ_core)^(-k)
                let ratio = theta_view / theta_core;
                e_iso_core * ratio.powf(-index)
            }
        }
    }
}

/// Calculate effective Lorentz factor at viewing angle
fn effective_lorentz_factor(
    theta_view: f64,
    theta_core: f64,
    gamma_0_core: f64,
    structure: &JetStructure,
) -> f64 {
    match structure {
        JetStructure::TopHat => {
            if theta_view <= theta_core {
                gamma_0_core
            } else {
                1.0 // No relativistic motion outside jet
            }
        }

        JetStructure::Gaussian => {
            // Γ(θ) ∝ E(θ)^(1/2) for similar baryon loading
            let energy_ratio = effective_energy(theta_view, theta_core, 1.0, structure);
            gamma_0_core * energy_ratio.sqrt()
        }

        JetStructure::PowerLaw { index } => {
            if theta_view <= theta_core {
                gamma_0_core
            } else {
                let ratio = theta_view / theta_core;
                gamma_0_core * ratio.powf(-index / 2.0)
            }
        }
    }
}

/// Calculate afterglow peak time and flux
///
/// Returns (t_peak_days, normalized_flux)
fn calculate_afterglow_peak(
    e_iso_eff: f64,
    gamma_0_eff: f64,
    theta_view: f64,
    config: &AfterglowConfig,
) -> (Option<f64>, Option<f64>) {
    // Deceleration time (when Γ ~ 1/θ_view)
    // t_dec ~ E_iso / (n * m_p * c^3 * Γ_0^8) * (1 + z)

    let mp_c3 = 1.5e33; // m_p * c^3 in CGS
    let e_iso_52 = e_iso_eff / 1e52; // Energy in units of 10^52 erg
    let n = config.circumburst_density;

    // Deceleration time (days)
    // Simplified scaling: t_dec ~ E_iso^(1/3) * n^(-1/3) * Γ_0^(-8/3)
    // Normalization adjusted to give realistic peak times of ~1-10 days for typical GRBs
    let t_dec_days =
        10.0 * e_iso_52.powf(1.0 / 3.0) * n.powf(-1.0 / 3.0) * gamma_0_eff.powf(-8.0 / 3.0);

    // For off-axis viewing, peak time is delayed
    // t_peak ~ t_dec / (1 - θ_view/θ_jet)
    let theta_eff = theta_view.max(0.1); // Avoid singularity
    let delay_factor = 1.0 + (theta_eff * gamma_0_eff).powi(2);
    let t_peak = t_dec_days * delay_factor;

    // Peak flux (normalized to on-axis peak)
    // F_peak ∝ E_iso * n^(1/2) / d^2
    // For off-axis, flux is reduced by beaming and viewing angle effects
    let beaming_factor = if theta_view < 1.0 / gamma_0_eff {
        1.0
    } else {
        (1.0 / (gamma_0_eff * theta_view)).powi(2)
    };

    let flux_norm = e_iso_52 * n.sqrt() * beaming_factor;

    if t_peak > 0.0 && flux_norm > 0.0 {
        (Some(t_peak), Some(flux_norm))
    } else {
        (None, None)
    }
}

/// Convert normalized flux to apparent magnitude
///
/// Uses a reference afterglow calibration:
/// - **On-axis** SGRB at 100 Mpc: M_opt ~ -19 → m ~ 16 mag
/// - **Off-axis** (like GW170817A at 40 Mpc): m ~ 17-25 mag depending on viewing angle
///
/// On-axis SGRBs have absolute magnitudes M_opt,peak ~ -18 to -21 (typical -19)
/// At distance d: m = M + 5*log10(d/10pc) = -19 + 5*log10(d_Mpc * 1e6)
///
/// # Arguments
///
/// * `flux_norm` - Normalized flux (e_iso_52 * sqrt(n) * beaming_factor)
/// * `distance_mpc` - Distance to source (Mpc)
/// * `circumburst_density` - Circumburst density (cm^-3)
///
/// # Returns
///
/// Apparent AB magnitude (R-band)
fn flux_to_magnitude(flux_norm: f64, distance_mpc: f64, circumburst_density: f64) -> f64 {
    // Reference: On-axis SGRB at 100 Mpc with E_iso = 1e52 erg, n = 1e-3
    // Absolute magnitude M ~ -19 → apparent magnitude m ~ 16.0 at 100 Mpc
    let flux_ref = 1.0 * (1e-3_f64).sqrt() * 1.0; // On-axis, no beaming suppression
    let m_ref = 16.0; // Brighter than previous 19 mag (on-axis vs off-axis!)
    let d_ref = 100.0;

    // Magnitude scales as:
    // - Fainter by 2.5*log10(flux_ref/flux) due to reduced flux
    // - Fainter by 5*log10(d/d_ref) due to distance (inverse square law)
    // - Correction for density (sqrt(n) dependence in flux_norm already included)

    let mag = m_ref + 2.5 * (flux_ref / flux_norm).log10() + 5.0 * (distance_mpc / d_ref).log10();

    mag
}

/// Calculate what fraction of viewing angles produce detectable afterglows
fn calculate_visibility_fraction(theta_core: f64, structure: &JetStructure) -> f64 {
    match structure {
        JetStructure::TopHat => {
            // Only visible within jet cone
            let solid_angle = 2.0 * PI * (1.0 - theta_core.cos());
            solid_angle / (4.0 * PI)
        }

        JetStructure::Gaussian => {
            // Visible out to ~3*θ_core (flux drops to ~1% of peak)
            let theta_max = 3.0 * theta_core;
            let theta_max_capped = theta_max.min(PI / 2.0);
            let solid_angle = 2.0 * PI * (1.0 - theta_max_capped.cos());
            solid_angle / (4.0 * PI)
        }

        JetStructure::PowerLaw { index } => {
            // Visible out to where power-law drops to 1% of core
            // (θ_max/θ_core)^(-k) = 0.01 → θ_max = θ_core * 100^(1/k)
            let theta_max = theta_core * 100.0_f64.powf(1.0 / index);
            let theta_max_capped = theta_max.min(PI / 2.0);
            let solid_angle = 2.0 * PI * (1.0 - theta_max_capped.cos());
            solid_angle / (4.0 * PI)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_jet_energy_profile() {
        let theta_core = 0.1; // ~5.7 degrees
        let e_iso_core = 1e52;

        // On-axis
        let e_on_axis = effective_energy(0.0, theta_core, e_iso_core, &JetStructure::Gaussian);
        assert!((e_on_axis - e_iso_core).abs() < 1e10);

        // At core angle, energy should be ~60% of core
        let e_at_core =
            effective_energy(theta_core, theta_core, e_iso_core, &JetStructure::Gaussian);
        let expected_at_core = e_iso_core * (-0.5_f64).exp();
        assert!((e_at_core - expected_at_core).abs() / expected_at_core < 0.01);

        // Far off-axis (3*θ_core), energy should be ~1% of core
        let e_off_axis = effective_energy(
            3.0 * theta_core,
            theta_core,
            e_iso_core,
            &JetStructure::Gaussian,
        );
        assert!(e_off_axis < 0.02 * e_iso_core);

        println!("Gaussian jet energy profile:");
        println!("  On-axis: {:.2e} erg", e_on_axis);
        println!(
            "  At θ_core: {:.2e} erg ({:.1}%)",
            e_at_core,
            e_at_core / e_iso_core * 100.0
        );
        println!(
            "  At 3*θ_core: {:.2e} erg ({:.1}%)",
            e_off_axis,
            e_off_axis / e_iso_core * 100.0
        );
    }

    #[test]
    fn test_afterglow_detectability() {
        let theta_core = 0.17; // ~10 degrees (typical for short GRBs)
        let e_iso_core = 1e52;
        let distance_mpc = 100.0;
        let config = AfterglowConfig::default();

        // On-axis observer
        let ag_on_axis = simulate_afterglow(0.0, theta_core, e_iso_core, distance_mpc, &config);
        assert!(ag_on_axis.detectable);
        assert!(ag_on_axis.t_peak_optical.is_some());
        println!(
            "On-axis magnitude: {:.2}",
            ag_on_axis.peak_magnitude.unwrap()
        );

        // Slightly off-axis (within 2*θ_core)
        let ag_off_axis = simulate_afterglow(
            2.0 * theta_core,
            theta_core,
            e_iso_core,
            distance_mpc,
            &config,
        );
        // Should still be detectable for Gaussian jet

        // Far off-axis (5*θ_core)
        let ag_far = simulate_afterglow(
            5.0 * theta_core,
            theta_core,
            e_iso_core,
            distance_mpc,
            &config,
        );
        // May not be detectable (flux too low)

        println!(
            "\nAfterglow detectability (limiting mag = {}):",
            config.limiting_magnitude
        );
        println!(
            "  On-axis: detectable={}, mag={:.2}, t_peak={:.2} days",
            ag_on_axis.detectable,
            ag_on_axis.peak_magnitude.unwrap_or(99.0),
            ag_on_axis.t_peak_optical.unwrap_or(0.0)
        );
        println!(
            "  Off-axis (2*θ_core): detectable={}, mag={:.2}, t_peak={:.2} days",
            ag_off_axis.detectable,
            ag_off_axis.peak_magnitude.unwrap_or(99.0),
            ag_off_axis.t_peak_optical.unwrap_or(0.0)
        );
        println!(
            "  Far off-axis (5*θ_core): detectable={}, mag={:.2}",
            ag_far.detectable,
            ag_far.peak_magnitude.unwrap_or(99.0)
        );
    }

    #[test]
    fn test_visibility_fraction() {
        let theta_core = 0.17; // ~10 degrees

        // Top-hat: only visible within jet cone (~0.76%)
        let vis_tophat = calculate_visibility_fraction(theta_core, &JetStructure::TopHat);
        assert!(vis_tophat < 0.01);
        assert!(vis_tophat > 0.005);

        // Gaussian: visible out to ~3*θ_core (~6-7%)
        let vis_gaussian = calculate_visibility_fraction(theta_core, &JetStructure::Gaussian);
        assert!(vis_gaussian > vis_tophat);
        assert!(vis_gaussian < 0.10);

        println!("\nVisibility fractions:");
        println!("  Top-hat (θ_core=10°): {:.2}%", vis_tophat * 100.0);
        println!("  Gaussian (θ_core=10°): {:.2}%", vis_gaussian * 100.0);
    }

    #[test]
    fn test_gw170817_afterglow() {
        let theta_core = 0.087; // ~5 degrees (GW170817 jet core)
        let theta_view = 0.35; // ~20 degrees (GW170817 viewing angle)
        let e_iso_core = 2e52; // GW170817 jet energy
        let distance_mpc = 40.0; // GW170817 distance
        let config = AfterglowConfig::gw170817_like();

        let ag = simulate_afterglow(theta_view, theta_core, e_iso_core, distance_mpc, &config);

        println!("\nGW170817-like afterglow:");
        println!("  Distance: {:.0} Mpc", distance_mpc);
        println!(
            "  Viewing angle: {:.1}° (off-axis)",
            theta_view.to_degrees()
        );
        println!("  Core angle: {:.1}°", theta_core.to_degrees());
        println!("  E_iso effective: {:.2e} erg", ag.e_iso_eff);
        println!("  Detectable: {}", ag.detectable);

        if let Some(mag) = ag.peak_magnitude {
            println!(
                "  Peak magnitude: {:.2} (limit: {})",
                mag, config.limiting_magnitude
            );
        }
        if let Some(t_peak) = ag.t_peak_optical {
            println!("  Peak time: {:.1} days", t_peak);
        }
        if let Some(flux) = ag.flux_peak_optical {
            println!("  Peak flux (normalized): {:.2e}", flux);
        }

        // GW170817 was far off-axis (~20° vs ~5° core), so optical afterglow was very faint
        // At 4-sigma off-axis for Gaussian jet, flux ~ exp(-8) ~ 0.03% of on-axis
        // This is below typical optical detection thresholds
        // (Radio afterglow was detected, but that's not modeled here)

        // Just verify properties are computed
        assert!(ag.t_peak_optical.is_some());
        assert!(ag.e_iso_eff > 0.0);
        assert_eq!(ag.jet_structure, JetStructure::Gaussian);
    }
}
