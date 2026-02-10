//! GRB counterpart simulation for gravitational wave events
//!
//! This module simulates gamma-ray burst (GRB) counterparts for binary neutron star (BNS)
//! mergers detected by gravitational wave observatories. The simulation is based on
//! observational constraints from short GRBs.
//!
//! ## Physical Model
//!
//! - **Jet opening angle**: Normal distribution ~10° ± 2° (Fong et al. 2015)
//! - **Isotropic energy (E_iso)**: Log-normal ~10^51.5 ± 0.5 erg
//! - **Duration (T90)**: Log-normal ~0.3-2 seconds (short GRBs)
//! - **Peak energy (E_peak)**: Log-normal ~200 keV
//! - **Visibility criterion**: GRB visible only if inclination ≤ jet opening angle
//!
//! ## References
//!
//! - Fong et al. 2015: "The Afterglow and Early-Type Host Galaxy of the Short GRB 150101B at z = 0.1343"
//! - Beniamini & Nakar 2019: "Observational constraints on the structure of gamma-ray burst jets"

use rand::Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Configuration for GRB simulation parameters
///
/// Default values are based on observational constraints from short GRBs
/// associated with binary neutron star mergers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrbSimulationConfig {
    /// Mean jet opening angle (degrees)
    pub jet_angle_mean: f64,

    /// Standard deviation of jet opening angle (degrees)
    pub jet_angle_std: f64,

    /// Mean of log10(E_iso) distribution (erg)
    pub eiso_log_mean: f64,

    /// Standard deviation of log10(E_iso) distribution
    pub eiso_log_std: f64,

    /// Mean of log10(T90) distribution (seconds)
    pub t90_log_mean: f64,

    /// Standard deviation of log10(T90) distribution
    pub t90_log_std: f64,

    /// Mean of log10(E_peak) distribution (keV)
    pub epeak_log_mean: f64,

    /// Standard deviation of log10(E_peak) distribution
    pub epeak_log_std: f64,
}

impl Default for GrbSimulationConfig {
    fn default() -> Self {
        Self {
            // Fong+2015: jet opening angle ~10° ± 2°
            jet_angle_mean: 10.0,
            jet_angle_std: 2.0,

            // Short GRB population: E_iso ~ 10^51.5 erg
            eiso_log_mean: 51.5,
            eiso_log_std: 0.5,

            // Short GRBs: T90 ~ 0.3-2 seconds
            t90_log_mean: 0.3,
            t90_log_std: 0.3,

            // Typical E_peak ~ 200 keV
            epeak_log_mean: 2.3, // log10(200) ≈ 2.3
            epeak_log_std: 0.3,
        }
    }
}

/// Simulated GRB counterpart properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedGrb {
    /// Whether the GRB is visible from Earth (within jet opening angle)
    pub visible: bool,

    /// Jet half-opening angle (degrees)
    pub theta_jet_deg: f64,

    /// Isotropic equivalent energy (erg)
    /// Only valid if visible = true
    pub e_iso: Option<f64>,

    /// Source-frame duration T90 (seconds)
    /// Only valid if visible = true
    pub t90: Option<f64>,

    /// Source-frame peak energy (keV)
    /// Only valid if visible = true
    pub e_peak: Option<f64>,

    /// Bolometric fluence (erg/cm²)
    /// Only valid if visible = true
    pub fluence: Option<f64>,

    /// Observed (redshifted) T90 (seconds)
    /// T90_obs = T90 * (1 + z)
    /// Only valid if visible = true
    pub t90_obs: Option<f64>,

    /// Observed (redshifted) E_peak (keV)
    /// E_peak_obs = E_peak / (1 + z)
    /// Only valid if visible = true
    pub e_peak_obs: Option<f64>,
}

/// GW event parameters needed for GRB simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GwEventParams {
    /// Inclination angle (radians)
    pub inclination: f64,

    /// Luminosity distance (Mpc)
    pub distance: f64,

    /// Cosmological redshift
    pub z: f64,
}

impl GwEventParams {
    /// Create from mm-core GWEvent
    pub fn from_gw_event(_event: &mm_core::events::GWEvent) -> Self {
        // Note: mm-core GWEvent might not have these fields yet
        // This is a placeholder - adjust based on actual GWEvent structure
        Self {
            inclination: 0.0, // TODO: Extract from event
            distance: 0.0,    // TODO: Extract from event
            z: 0.0,           // TODO: Extract from event
        }
    }
}

/// Simulate a GRB counterpart for a gravitational wave event
///
/// # Arguments
///
/// * `gw_params` - GW event parameters (inclination, distance, redshift)
/// * `config` - GRB simulation configuration
/// * `rng` - Random number generator
///
/// # Returns
///
/// `SimulatedGrb` with visibility and properties. If not visible, only
/// `visible` and `theta_jet_deg` are populated.
///
/// # Example
///
/// ```
/// use mm_simulation::grb_simulation::{simulate_grb_counterpart, GwEventParams, GrbSimulationConfig};
/// use rand::thread_rng;
///
/// let gw_params = GwEventParams {
///     inclination: 0.2,  // ~11.5 degrees (likely visible)
///     distance: 40.0,    // 40 Mpc (GW170817-like)
///     z: 0.01,
/// };
///
/// let config = GrbSimulationConfig::default();
/// let mut rng = thread_rng();
///
/// let grb = simulate_grb_counterpart(&gw_params, &config, &mut rng);
///
/// if grb.visible {
///     println!("GRB detected! Fluence: {:.2e} erg/cm²", grb.fluence.unwrap());
/// } else {
///     println!("GRB not visible (jet angle: {:.1}°)", grb.theta_jet_deg);
/// }
/// ```
pub fn simulate_grb_counterpart(
    gw_params: &GwEventParams,
    config: &GrbSimulationConfig,
    rng: &mut impl Rng,
) -> SimulatedGrb {
    // Sample jet opening angle (degrees)
    let jet_angle_dist = Normal::new(config.jet_angle_mean, config.jet_angle_std)
        .expect("Invalid jet angle distribution parameters");
    let theta_jet_deg = jet_angle_dist.sample(rng);

    // Convert inclination to degrees
    let inclination_deg = gw_params.inclination.to_degrees();

    // Check visibility criterion: GRB visible if viewing angle within jet cone
    let visible = inclination_deg <= theta_jet_deg;

    if !visible {
        return SimulatedGrb {
            visible: false,
            theta_jet_deg,
            e_iso: None,
            t90: None,
            e_peak: None,
            fluence: None,
            t90_obs: None,
            e_peak_obs: None,
        };
    }

    // Sample intrinsic GRB properties (log-normal distributions)

    // Isotropic energy: E_iso [erg]
    let eiso_dist = Normal::new(config.eiso_log_mean, config.eiso_log_std)
        .expect("Invalid E_iso distribution parameters");
    let e_iso = 10_f64.powf(eiso_dist.sample(rng));

    // Duration: T90 [seconds]
    let t90_dist = Normal::new(config.t90_log_mean, config.t90_log_std)
        .expect("Invalid T90 distribution parameters");
    let t90 = 10_f64.powf(t90_dist.sample(rng));

    // Peak energy: E_peak [keV]
    let epeak_dist = Normal::new(config.epeak_log_mean, config.epeak_log_std)
        .expect("Invalid E_peak distribution parameters");
    let e_peak = 10_f64.powf(epeak_dist.sample(rng));

    // Compute fluence [erg/cm²]
    // Fluence = E_iso / (4π d²)
    const MPC_TO_CM: f64 = 3.086e24; // 1 Mpc in cm
    let distance_cm = gw_params.distance * MPC_TO_CM;
    let fluence = e_iso / (4.0 * PI * distance_cm.powi(2));

    // Apply cosmological redshift corrections
    // Observed T90 is stretched by (1 + z)
    let t90_obs = t90 * (1.0 + gw_params.z);

    // Observed E_peak is reduced by (1 + z)
    let e_peak_obs = e_peak / (1.0 + gw_params.z);

    SimulatedGrb {
        visible: true,
        theta_jet_deg,
        e_iso: Some(e_iso),
        t90: Some(t90),
        e_peak: Some(e_peak),
        fluence: Some(fluence),
        t90_obs: Some(t90_obs),
        e_peak_obs: Some(e_peak_obs),
    }
}

/// Simulate GRB counterparts for a batch of GW events
///
/// This is more efficient than calling `simulate_grb_counterpart` individually
/// as it reuses the RNG and distributions.
///
/// # Arguments
///
/// * `gw_events` - Slice of GW event parameters
/// * `config` - GRB simulation configuration
/// * `rng` - Random number generator
///
/// # Returns
///
/// Vector of simulated GRBs, one per input GW event
pub fn simulate_grb_batch(
    gw_events: &[GwEventParams],
    config: &GrbSimulationConfig,
    rng: &mut impl Rng,
) -> Vec<SimulatedGrb> {
    gw_events
        .iter()
        .map(|gw_params| simulate_grb_counterpart(gw_params, config, rng))
        .collect()
}

/// Statistics for a batch of simulated GRBs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrbSimulationStats {
    /// Total number of GW events simulated
    pub total_events: usize,

    /// Number of visible GRBs
    pub visible_grbs: usize,

    /// Visibility fraction (visible / total)
    pub visibility_fraction: f64,

    /// Mean jet opening angle (degrees)
    pub mean_jet_angle: f64,

    /// For visible GRBs: mean fluence (erg/cm²)
    pub mean_fluence: Option<f64>,

    /// For visible GRBs: mean T90_obs (seconds)
    pub mean_t90_obs: Option<f64>,

    /// For visible GRBs: mean E_peak_obs (keV)
    pub mean_epeak_obs: Option<f64>,
}

/// Compute statistics for a batch of simulated GRBs
pub fn compute_simulation_stats(grbs: &[SimulatedGrb]) -> GrbSimulationStats {
    let total_events = grbs.len();
    let visible_grbs: Vec<_> = grbs.iter().filter(|g| g.visible).collect();
    let n_visible = visible_grbs.len();

    let visibility_fraction = if total_events > 0 {
        n_visible as f64 / total_events as f64
    } else {
        0.0
    };

    let mean_jet_angle = grbs.iter().map(|g| g.theta_jet_deg).sum::<f64>() / total_events as f64;

    let mean_fluence = if n_visible > 0 {
        Some(visible_grbs.iter().filter_map(|g| g.fluence).sum::<f64>() / n_visible as f64)
    } else {
        None
    };

    let mean_t90_obs = if n_visible > 0 {
        Some(visible_grbs.iter().filter_map(|g| g.t90_obs).sum::<f64>() / n_visible as f64)
    } else {
        None
    };

    let mean_epeak_obs = if n_visible > 0 {
        Some(
            visible_grbs
                .iter()
                .filter_map(|g| g.e_peak_obs)
                .sum::<f64>()
                / n_visible as f64,
        )
    } else {
        None
    };

    GrbSimulationStats {
        total_events,
        visible_grbs: n_visible,
        visibility_fraction,
        mean_jet_angle,
        mean_fluence,
        mean_t90_obs,
        mean_epeak_obs,
    }
}

/// Complete multi-messenger event simulation
///
/// This structure contains all properties of a simulated multi-messenger event
/// from gravitational waves to optical/gamma-ray counterparts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMessengerEvent {
    /// Binary parameters (masses, spins, radii)
    pub binary_params: crate::ejecta_properties::BinaryParams,

    /// Gravitational wave parameters (inclination, distance, redshift)
    pub gw_params: GwEventParams,

    /// Ejecta properties for kilonova emission
    pub ejecta: crate::ejecta_properties::EjectaProperties,

    /// GRB counterpart (if visible)
    pub grb: SimulatedGrb,

    /// Afterglow emission properties (optical/X-ray)
    pub afterglow: crate::afterglow::AfterglowProperties,

    /// Binary type classification
    pub binary_type: crate::ejecta_properties::BinaryType,
}

impl MultiMessengerEvent {
    /// Check if this event has a visible GRB counterpart
    pub fn has_grb(&self) -> bool {
        self.grb.visible
    }

    /// Check if this event produces optical ejecta (kilonova)
    pub fn has_kilonova(&self) -> bool {
        self.ejecta.mej_total > 0.0
    }

    /// Get total ejecta mass (solar masses)
    pub fn kilonova_mass(&self) -> f64 {
        self.ejecta.mej_total
    }

    /// Check if this event has a detectable afterglow
    /// (can be true even if GRB is not visible, for structured jets)
    pub fn has_afterglow(&self) -> bool {
        self.afterglow.detectable
    }

    /// Get GRB properties (if visible)
    pub fn grb_properties(&self) -> Option<GrbProperties> {
        if self.grb.visible {
            Some(GrbProperties {
                t90_obs: self.grb.t90_obs?,
                fluence: self.grb.fluence?,
                e_peak_obs: self.grb.e_peak_obs?,
                theta_jet: self.grb.theta_jet_deg,
            })
        } else {
            None
        }
    }
}

/// Extracted GRB properties for easy access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrbProperties {
    /// Observed T90 duration (seconds, redshifted)
    pub t90_obs: f64,

    /// Observed fluence (erg/cm²)
    pub fluence: f64,

    /// Observed peak energy (keV, blueshifted)
    pub e_peak_obs: f64,

    /// Jet opening angle (degrees)
    pub theta_jet: f64,
}

/// Simulate a complete multi-messenger event from binary parameters
///
/// This function integrates all simulation components:
/// 1. Binary classification (BNS/NSBH/BBH)
/// 2. Ejecta property calculation
/// 3. GRB counterpart simulation
///
/// # Arguments
///
/// * `binary_params` - Physical parameters of the binary (masses, spins, radii)
/// * `gw_params` - Gravitational wave parameters (inclination, distance, redshift)
/// * `grb_config` - GRB simulation configuration
/// * `rng` - Random number generator
///
/// # Returns
///
/// Complete multi-messenger event with all properties
///
/// # Example
///
/// ```
/// use mm_simulation::{
///     simulate_multimessenger_event, BinaryParams, GwEventParams, GrbSimulationConfig,
/// };
/// use rand::thread_rng;
///
/// let mut rng = thread_rng();
///
/// // GW170817-like binary
/// let binary_params = BinaryParams {
///     mass_1_source: 1.46,
///     mass_2_source: 1.27,
///     radius_1: 11.9,
///     radius_2: 11.9,
///     chi_1: 0.0,
///     chi_2: 0.0,
///     tov_mass: 2.17,
///     r_16: 11.9,
///     ratio_zeta: 0.2,
///     alpha: 1.0,
///     ratio_epsilon: 0.1,
/// };
///
/// let gw_params = GwEventParams {
///     inclination: 0.44,  // ~25°
///     distance: 40.0,     // Mpc
///     z: 0.01,
/// };
///
/// let event = simulate_multimessenger_event(
///     &binary_params,
///     &gw_params,
///     &GrbSimulationConfig::default(),
///     &mut rng,
/// );
///
/// println!("Binary type: {:?}", event.binary_type);
/// println!("Kilonova ejecta: {:.4} M_sun", event.kilonova_mass());
///
/// if event.has_grb() {
///     let grb = event.grb_properties().unwrap();
///     println!("GRB detected! T90 = {:.2} s", grb.t90_obs);
/// }
/// ```
pub fn simulate_multimessenger_event(
    binary_params: &crate::ejecta_properties::BinaryParams,
    gw_params: &GwEventParams,
    grb_config: &GrbSimulationConfig,
    rng: &mut impl Rng,
) -> MultiMessengerEvent {
    // 1. Compute ejecta properties from binary parameters
    let ejecta = crate::ejecta_properties::compute_ejecta_properties(binary_params);

    // 2. Simulate GRB counterpart
    let grb = simulate_grb_counterpart(gw_params, grb_config, rng);

    // 3. Simulate afterglow emission (optical/X-ray)
    // Use jet energy from ejecta properties or GRB
    let e_iso_core = if let Some(ejet) = ejecta.ejet_grb {
        ejet
    } else {
        grb.e_iso.unwrap_or(1e52) // Default if no jet energy
    };

    let afterglow = crate::afterglow::simulate_afterglow(
        gw_params.inclination,
        grb.theta_jet_deg.to_radians(),
        e_iso_core,
        gw_params.distance,
        &crate::afterglow::AfterglowConfig::default(),
    );

    // 4. Package everything together
    MultiMessengerEvent {
        binary_params: binary_params.clone(),
        gw_params: gw_params.clone(),
        ejecta: ejecta.clone(),
        grb,
        afterglow,
        binary_type: ejecta.binary_type,
    }
}

/// Simulate a batch of multi-messenger events
///
/// This is a convenience function for simulating many events efficiently.
///
/// # Arguments
///
/// * `binary_params` - Vector of binary parameters
/// * `gw_params` - Vector of GW parameters (must match length of binary_params)
/// * `grb_config` - GRB simulation configuration
/// * `rng` - Random number generator
///
/// # Returns
///
/// Vector of simulated multi-messenger events
pub fn simulate_multimessenger_batch(
    binary_params: &[crate::ejecta_properties::BinaryParams],
    gw_params: &[GwEventParams],
    grb_config: &GrbSimulationConfig,
    rng: &mut impl Rng,
) -> Vec<MultiMessengerEvent> {
    assert_eq!(
        binary_params.len(),
        gw_params.len(),
        "binary_params and gw_params must have the same length"
    );

    binary_params
        .iter()
        .zip(gw_params.iter())
        .map(|(bp, gwp)| simulate_multimessenger_event(bp, gwp, grb_config, rng))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_grb_simulation_gw170817_like() {
        // GW170817: i ~ 25°, d ~ 40 Mpc, z ~ 0.01
        let gw_params = GwEventParams {
            inclination: 0.44, // ~25 degrees
            distance: 40.0,
            z: 0.01,
        };

        let config = GrbSimulationConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let grb = simulate_grb_counterpart(&gw_params, &config, &mut rng);

        // With jet angle ~10° and inclination ~25°, most should be invisible
        // But with some probability (if jet angle > 25°), it could be visible
        println!("GRB visible: {}", grb.visible);
        println!("Jet angle: {:.1}°", grb.theta_jet_deg);

        if grb.visible {
            assert!(grb.fluence.is_some());
            assert!(grb.t90_obs.is_some());
            assert!(grb.e_peak_obs.is_some());

            // Check that redshift corrections are applied
            let t90 = grb.t90.unwrap();
            let t90_obs = grb.t90_obs.unwrap();
            assert!((t90_obs - t90 * 1.01).abs() < 1e-10);

            let e_peak = grb.e_peak.unwrap();
            let e_peak_obs = grb.e_peak_obs.unwrap();
            assert!((e_peak_obs - e_peak / 1.01).abs() < 1e-6);
        }
    }

    #[test]
    fn test_grb_simulation_batch() {
        let config = GrbSimulationConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);

        // Generate 1000 GW events with random inclinations
        let gw_events: Vec<_> = (0..1000)
            .map(|_| {
                let inclination = rng.gen::<f64>() * PI; // 0 to π
                GwEventParams {
                    inclination,
                    distance: 100.0 + rng.gen::<f64>() * 900.0, // 100-1000 Mpc
                    z: 0.02 + rng.gen::<f64>() * 0.18,          // z = 0.02-0.2
                }
            })
            .collect();

        let grbs = simulate_grb_batch(&gw_events, &config, &mut rng);

        assert_eq!(grbs.len(), 1000);

        let stats = compute_simulation_stats(&grbs);
        println!("\nGRB Simulation Statistics:");
        println!("  Total events: {}", stats.total_events);
        println!("  Visible GRBs: {}", stats.visible_grbs);
        println!(
            "  Visibility fraction: {:.1}%",
            stats.visibility_fraction * 100.0
        );
        println!("  Mean jet angle: {:.1}°", stats.mean_jet_angle);

        if let Some(mean_fluence) = stats.mean_fluence {
            println!("  Mean fluence: {:.2e} erg/cm²", mean_fluence);
        }

        // Visibility should be ~1-5% for isotropic inclination distribution
        // (solid angle of 10° cone is small compared to full sky)
        // Allow up to 6% for statistical fluctuations with small sample size
        assert!(stats.visibility_fraction > 0.001);
        assert!(stats.visibility_fraction < 0.06);

        // Mean jet angle should be close to configured mean
        assert!((stats.mean_jet_angle - config.jet_angle_mean).abs() < 1.0);
    }

    #[test]
    fn test_visibility_criterion() {
        let config = GrbSimulationConfig {
            jet_angle_mean: 10.0,
            jet_angle_std: 0.01, // Very narrow distribution
            ..Default::default()
        };

        let mut rng = rand::rngs::StdRng::seed_from_u64(456);

        // Face-on (inclination = 0°): should be visible
        let gw_face_on = GwEventParams {
            inclination: 0.0,
            distance: 100.0,
            z: 0.01,
        };

        let grb_face_on = simulate_grb_counterpart(&gw_face_on, &config, &mut rng);
        assert!(grb_face_on.visible);

        // Edge-on (inclination = 90°): should NOT be visible
        let gw_edge_on = GwEventParams {
            inclination: PI / 2.0,
            distance: 100.0,
            z: 0.01,
        };

        let grb_edge_on = simulate_grb_counterpart(&gw_edge_on, &config, &mut rng);
        assert!(!grb_edge_on.visible);
        assert!(grb_edge_on.fluence.is_none());
    }

    #[test]
    fn test_multimessenger_event_gw170817() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(789);

        // GW170817-like binary
        let binary_params = crate::ejecta_properties::BinaryParams {
            mass_1_source: 1.46,
            mass_2_source: 1.27,
            radius_1: 11.9,
            radius_2: 11.9,
            chi_1: 0.0,
            chi_2: 0.0,
            tov_mass: 2.17,
            r_16: 11.9,
            ratio_zeta: 0.2,
            alpha: 0.0,
            ratio_epsilon: 0.1,
        };

        let gw_params = GwEventParams {
            inclination: 0.44, // ~25°
            distance: 40.0,
            z: 0.01,
        };

        let event = simulate_multimessenger_event(
            &binary_params,
            &gw_params,
            &GrbSimulationConfig::default(),
            &mut rng,
        );

        println!("\n=== Multi-Messenger Event Simulation ===");
        println!("Binary type: {:?}", event.binary_type);
        println!(
            "Kilonova ejecta: {:.4} M_sun (dyn: {:.4}, wind: {:.4})",
            event.kilonova_mass(),
            event.ejecta.mej_dyn,
            event.ejecta.mej_wind
        );
        println!("Ejecta velocity: {:.3}c", event.ejecta.vej_dyn);
        println!("Disk mass: {:.4} M_sun", event.ejecta.mdisk);

        // Check ejecta properties
        assert_eq!(event.binary_type, crate::ejecta_properties::BinaryType::BNS);
        assert!(event.has_kilonova());
        assert!(event.kilonova_mass() > 0.0);
        assert!(event.ejecta.mdisk > 0.0);

        // GRB may or may not be visible depending on random jet angle
        if event.has_grb() {
            let grb = event.grb_properties().unwrap();
            println!("GRB visible! T90 = {:.2} s", grb.t90_obs);
            println!("GRB fluence: {:.2e} erg/cm²", grb.fluence);
            println!("Jet angle: {:.1}°", grb.theta_jet);

            assert!(grb.t90_obs > 0.0);
            assert!(grb.fluence > 0.0);
        } else {
            println!("GRB not visible (viewing angle > jet angle)");
        }
    }

    #[test]
    fn test_multimessenger_batch() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(101112);

        // Generate 100 BNS events with varying parameters
        let n_events = 100;

        let binary_params: Vec<_> = (0..n_events)
            .map(|_| crate::ejecta_properties::BinaryParams {
                mass_1_source: 1.2 + rng.gen::<f64>() * 0.6, // 1.2-1.8 M_sun
                mass_2_source: 1.2 + rng.gen::<f64>() * 0.6,
                radius_1: 11.0 + rng.gen::<f64>() * 2.0, // 11-13 km
                radius_2: 11.0 + rng.gen::<f64>() * 2.0,
                chi_1: rng.gen::<f64>() * 0.1, // Small spins
                chi_2: rng.gen::<f64>() * 0.1,
                tov_mass: 2.17,
                r_16: 11.9,
                ratio_zeta: 0.2,
                alpha: 0.0, // No additive correction
                ratio_epsilon: 0.1,
            })
            .collect();

        let gw_params: Vec<_> = (0..n_events)
            .map(|_| GwEventParams {
                inclination: rng.gen::<f64>() * PI,        // Random inclination
                distance: 50.0 + rng.gen::<f64>() * 150.0, // 50-200 Mpc
                z: 0.01 + rng.gen::<f64>() * 0.04,         // z = 0.01-0.05
            })
            .collect();

        let events = simulate_multimessenger_batch(
            &binary_params,
            &gw_params,
            &GrbSimulationConfig::default(),
            &mut rng,
        );

        assert_eq!(events.len(), n_events);

        // Compute statistics
        let n_with_grb = events.iter().filter(|e| e.has_grb()).count();
        let mean_ejecta = events.iter().map(|e| e.kilonova_mass()).sum::<f64>() / n_events as f64;

        println!("\n=== Multi-Messenger Batch Statistics ===");
        println!("Total events: {}", n_events);
        println!("Events with visible GRB: {}", n_with_grb);
        println!(
            "GRB detection fraction: {:.1}%",
            n_with_grb as f64 / n_events as f64 * 100.0
        );
        println!("Mean ejecta mass: {:.4} M_sun", mean_ejecta);

        // All events should have kilonova ejecta
        assert!(events.iter().all(|e| e.has_kilonova()));

        // Mean ejecta should be reasonable (0.001-0.02 M_sun typical)
        assert!(mean_ejecta > 0.0005);
        assert!(mean_ejecta < 0.05);

        // GRB detection fraction should be ~1-5%
        let grb_fraction = n_with_grb as f64 / n_events as f64;
        assert!(grb_fraction > 0.0);
        assert!(grb_fraction < 0.15); // Allow some margin for small sample
    }

    #[test]
    fn test_multimessenger_nsbh() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(131415);

        // NSBH system: 5 M_sun BH + 1.4 M_sun NS
        let binary_params = crate::ejecta_properties::BinaryParams {
            mass_1_source: 5.0, // BH
            mass_2_source: 1.4, // NS
            radius_1: 0.0,      // BH has no radius
            radius_2: 12.0,     // NS radius
            chi_1: 0.5,         // Spinning BH
            chi_2: 0.0,
            tov_mass: 2.17,
            r_16: 12.0,
            ratio_zeta: 0.2,
            alpha: 0.0,
            ratio_epsilon: 0.1,
        };

        let gw_params = GwEventParams {
            inclination: 0.3, // ~17°
            distance: 100.0,
            z: 0.02,
        };

        let event = simulate_multimessenger_event(
            &binary_params,
            &gw_params,
            &GrbSimulationConfig::default(),
            &mut rng,
        );

        println!("\n=== NSBH Event ===");
        println!("Binary type: {:?}", event.binary_type);
        println!("Kilonova ejecta: {:.4} M_sun", event.kilonova_mass());

        assert_eq!(
            event.binary_type,
            crate::ejecta_properties::BinaryType::NSBH
        );

        // NSBH should produce ejecta (if NS is tidally disrupted)
        // Amount depends on mass ratio and BH spin
        if event.has_kilonova() {
            println!("NS tidally disrupted, producing ejecta");
            assert!(event.kilonova_mass() > 0.0);
        } else {
            println!("NS directly plunged into BH (no disruption)");
        }
    }
}
