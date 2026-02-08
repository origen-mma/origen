//! Ejecta property calculations for BNS and NSBH mergers
//!
//! Converts gravitational wave binary parameters (masses, spins, tidal deformabilities)
//! to kilonova ejecta properties (ejecta mass, velocity, composition).
//!
//! Based on fits from:
//! - Krüger & Foucart 2020 (BNS dynamical ejecta)
//! - Radice et al. 2018 (BNS ejecta velocity)
//! - Kruger et al. 2020 (BNS disk mass)
//! - Foucart et al. 2018 (NSBH ejecta)
//! - Barbieri et al. 2020, Dietrich et al. 2020 (BNS/NSBH disk fits)

use serde::{Deserialize, Serialize};

/// Physical constants
const GEOM_MSUN_KM: f64 = 1.47662504; // Geometric solar mass in km (G M_sun / c^2)
const MSUN_TO_ERGS: f64 = 1.787e54;    // Solar mass to ergs (M_sun c^2)

/// Binary type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryType {
    /// Both components are neutron stars
    BNS,
    /// Primary is black hole, secondary is neutron star
    NSBH,
    /// Both components are black holes (no ejecta)
    BBH,
}

/// Kilonova ejecta properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EjectaProperties {
    /// Dynamical ejecta mass (solar masses)
    pub mej_dyn: f64,

    /// Wind ejecta mass (solar masses)
    pub mej_wind: f64,

    /// Total ejecta mass (solar masses)
    pub mej_total: f64,

    /// Dynamical ejecta velocity (c)
    pub vej_dyn: f64,

    /// Wind ejecta velocity (c, typically ~0.05-0.15)
    pub vej_wind: f64,

    /// Remnant disk mass (solar masses)
    pub mdisk: f64,

    /// GRB jet energy (erg), if applicable
    pub ejet_grb: Option<f64>,

    /// Binary type
    pub binary_type: BinaryType,
}

/// Binary system parameters for ejecta calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryParams {
    /// Primary mass (source frame, solar masses)
    pub mass_1_source: f64,

    /// Secondary mass (source frame, solar masses)
    pub mass_2_source: f64,

    /// Primary radius (km), 0 if black hole
    pub radius_1: f64,

    /// Secondary radius (km), 0 if black hole
    pub radius_2: f64,

    /// Primary aligned spin component (dimensionless)
    pub chi_1: f64,

    /// Secondary aligned spin component (dimensionless)
    pub chi_2: f64,

    /// TOV maximum mass (solar masses), from EOS
    pub tov_mass: f64,

    /// Radius of 1.6 M_sun NS (km), from EOS
    pub r_16: f64,

    /// Wind to disk ratio (typically 0.05-0.2)
    pub ratio_zeta: f64,

    /// Correction to dynamical ejecta (typically small, ~0)
    pub alpha: f64,

    /// Jet efficiency parameter (E_jet / E_disk, typically 2e-4)
    pub ratio_epsilon: f64,
}

impl BinaryParams {
    /// Classify the binary type based on radii
    pub fn binary_type(&self) -> BinaryType {
        if self.radius_1 > 0.0 && self.radius_2 > 0.0 {
            BinaryType::BNS
        } else if self.radius_1 == 0.0 && self.radius_2 > 0.0 {
            BinaryType::NSBH
        } else {
            BinaryType::BBH
        }
    }

    /// Compute compactness = M * geom_msun_km / R
    pub fn compactness_1(&self) -> f64 {
        if self.radius_1 > 0.0 {
            self.mass_1_source * GEOM_MSUN_KM / self.radius_1
        } else {
            0.5 // Black hole limit
        }
    }

    pub fn compactness_2(&self) -> f64 {
        if self.radius_2 > 0.0 {
            self.mass_2_source * GEOM_MSUN_KM / self.radius_2
        } else {
            0.5
        }
    }
}

/// Compute ejecta properties for a binary merger
pub fn compute_ejecta_properties(params: &BinaryParams) -> EjectaProperties {
    match params.binary_type() {
        BinaryType::BNS => compute_bns_ejecta(params),
        BinaryType::NSBH => compute_nsbh_ejecta(params),
        BinaryType::BBH => EjectaProperties {
            mej_dyn: 0.0,
            mej_wind: 0.0,
            mej_total: 0.0,
            vej_dyn: 0.0,
            vej_wind: 0.0,
            mdisk: 0.0,
            ejet_grb: None,
            binary_type: BinaryType::BBH,
        },
    }
}

/// BNS ejecta calculation (Krüger & Foucart 2020, Radice et al. 2018)
fn compute_bns_ejecta(params: &BinaryParams) -> EjectaProperties {
    let mass_1 = params.mass_1_source;
    let mass_2 = params.mass_2_source;
    let comp_1 = params.compactness_1();
    let comp_2 = params.compactness_2();

    let total_mass = mass_1 + mass_2;
    let mass_ratio = mass_2 / mass_1;

    // Dynamical ejecta (Krüger & Foucart 2020)
    let mdyn = dynamic_mass_fitting_krfo(mass_1, mass_2, comp_1, comp_2);
    let mej_dyn = mdyn + params.alpha;

    // Dynamical ejecta velocity (Radice et al. 2018)
    let vej_dyn = dynamic_vel_fitting_radice2018(mass_1, mass_2, comp_1, comp_2);

    // Remnant disk mass (Kruger et al. 2020)
    let log10_mdisk = log10_disk_mass_fitting_bns(
        total_mass,
        mass_ratio,
        params.tov_mass,
        params.r_16 / GEOM_MSUN_KM,
    );
    let mdisk = 10_f64.powf(log10_mdisk);

    // Wind ejecta from disk
    let mej_wind = params.ratio_zeta * mdisk;
    let vej_wind = 0.1; // Typical wind velocity ~0.1c

    let mej_total = mej_dyn + mej_wind;

    // GRB jet energy (fraction of accretion energy)
    let ejet_grb = if mdisk > 0.0 {
        Some(params.ratio_epsilon * (1.0 - params.ratio_zeta) * mdisk * MSUN_TO_ERGS)
    } else {
        None
    };

    EjectaProperties {
        mej_dyn,
        mej_wind,
        mej_total,
        vej_dyn,
        vej_wind,
        mdisk,
        ejet_grb,
        binary_type: BinaryType::BNS,
    }
}

/// NSBH ejecta calculation (Foucart et al. 2018)
fn compute_nsbh_ejecta(params: &BinaryParams) -> EjectaProperties {
    let mass_bh = params.mass_1_source;
    let mass_ns = params.mass_2_source;
    let comp_ns = params.compactness_2();
    let chi_bh = params.chi_1;

    // Baryon mass of NS
    let baryon_mass_ns = baryon_mass_ns(mass_ns, comp_ns);

    // Check if NS is tidally disrupted
    let risco = chi_bh_to_risco(chi_bh);
    let mass_ratio_inv = mass_bh / mass_ns;

    // Remnant disk mass
    let mdisk_remnant = remnant_disk_mass_fitting_nsbh(
        mass_bh,
        mass_ns,
        comp_ns,
        chi_bh,
        baryon_mass_ns,
        risco,
        mass_ratio_inv,
    );

    // Dynamical ejecta mass
    let mdyn = dynamic_mass_fitting_nsbh(
        mass_bh,
        mass_ns,
        comp_ns,
        chi_bh,
        baryon_mass_ns,
        risco,
        mass_ratio_inv,
    );

    let mdisk = (mdisk_remnant - mdyn).max(0.0);

    let mej_dyn = (mdyn + params.alpha).max(0.0);
    let mej_wind = if mdisk > 0.0 {
        params.ratio_zeta * mdisk
    } else {
        0.0
    };

    let mej_total = mej_dyn + mej_wind;

    // Ejecta velocities
    let vej_dyn = 0.25; // Typical for NSBH
    let vej_wind = 0.1;

    // GRB jet energy
    let ejet_grb = if mdisk > 0.0 {
        Some(params.ratio_epsilon * (1.0 - params.ratio_zeta) * mdisk * MSUN_TO_ERGS)
    } else {
        None
    };

    EjectaProperties {
        mej_dyn,
        mej_wind,
        mej_total,
        vej_dyn,
        vej_wind,
        mdisk,
        ejet_grb,
        binary_type: BinaryType::NSBH,
    }
}

// ========== BNS Fitting Functions ==========

/// Dynamical ejecta mass for BNS (Krüger & Foucart 2020)
/// See https://arxiv.org/pdf/2002.07728.pdf
fn dynamic_mass_fitting_krfo(
    mass_1: f64,
    mass_2: f64,
    comp_1: f64,
    comp_2: f64,
) -> f64 {
    const A: f64 = -9.3335;
    const B: f64 = 114.17;
    const C: f64 = -337.56;
    const N: f64 = 1.5465;

    let mdyn_1 = mass_1 * (A / comp_1 + B * (mass_2 / mass_1).powf(N) + C * comp_1);
    let mdyn_2 = mass_2 * (A / comp_2 + B * (mass_1 / mass_2).powf(N) + C * comp_2);

    let mdyn = (mdyn_1 + mdyn_2) * 1e-3; // Convert to solar masses

    mdyn.max(0.0)
}

/// Dynamical ejecta velocity for BNS (Radice et al. 2018)
/// See https://arxiv.org/pdf/1809.11161 Eq. (22)
fn dynamic_vel_fitting_radice2018(
    mass_1: f64,
    mass_2: f64,
    comp_1: f64,
    comp_2: f64,
) -> f64 {
    const A: f64 = -0.287;
    const B: f64 = 0.494;
    const C: f64 = -3.000;

    let vej = A * mass_1 / mass_2 * (1.0 + C * comp_1)
        + A * mass_2 / mass_1 * (1.0 + C * comp_2)
        + B;

    vej.max(0.0).min(0.8) // Cap at 0.8c for physical realism
}

/// Remnant disk mass for BNS (Kruger et al. 2020)
/// See https://arxiv.org/pdf/2205.08513 Eq. (22)
fn log10_disk_mass_fitting_bns(
    total_mass: f64,
    mass_ratio: f64,
    mtov: f64,
    r_16: f64,
) -> f64 {
    const A0: f64 = -1.725;
    const DELTA_A: f64 = -2.337;
    const B0: f64 = -0.564;
    const DELTA_B: f64 = -0.437;
    const C: f64 = 0.958;
    const D: f64 = 0.057;
    const BETA: f64 = 5.879;
    const Q_TRANS: f64 = 0.886;

    // Threshold mass for prompt collapse
    let k = -3.606 * mtov / r_16 + 2.38;
    let threshold_mass = k * mtov;

    // Mass ratio dependence
    let xi = 0.5 * (BETA * (mass_ratio - Q_TRANS)).tanh();

    let a = A0 + DELTA_A * xi;
    let b = B0 + DELTA_B * xi;

    let log10_mdisk = a * (1.0 + b * ((C - total_mass / threshold_mass) / D).tanh());

    log10_mdisk.max(-3.0) // Minimum disk mass ~ 0.001 M_sun
}

// ========== NSBH Fitting Functions ==========

/// Convert BH spin to ISCO radius
fn chi_bh_to_risco(chi_bh: f64) -> f64 {
    let z1 = 1.0 + (1.0 - chi_bh.powi(2)).powf(1.0 / 3.0)
        * ((1.0 + chi_bh).powf(1.0 / 3.0) + (1.0 - chi_bh).powf(1.0 / 3.0));
    let z2 = (3.0 * chi_bh.powi(2) + z1.powi(2)).sqrt();

    3.0 + z2 - chi_bh.signum() * ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt()
}

/// NS baryon mass (Foucart et al. 2018, Eq. 7)
fn baryon_mass_ns(mass_ns: f64, comp_ns: f64) -> f64 {
    mass_ns * (1.0 + 0.6 * comp_ns / (1.0 - 0.5 * comp_ns))
}

/// Remnant disk mass for NSBH (Foucart et al. 2018, Eq. 4)
fn remnant_disk_mass_fitting_nsbh(
    _mass_bh: f64,
    _mass_ns: f64,
    comp_ns: f64,
    _chi_bh: f64,
    baryon_mass_ns: f64,
    risco: f64,
    mass_ratio_inv: f64,
) -> f64 {
    const A: f64 = 0.40642158;
    const B: f64 = 0.13885773;
    const C: f64 = 0.25512517;
    const D: f64 = 0.761250847;

    let symm_mass_ratio = mass_ratio_inv / (1.0 + mass_ratio_inv).powi(2);

    let mut remnant_mass = A * symm_mass_ratio.powf(-1.0 / 3.0) * (1.0 - 2.0 * comp_ns);
    remnant_mass += -B * risco / symm_mass_ratio * comp_ns + C;
    remnant_mass = remnant_mass.max(0.0);
    remnant_mass = remnant_mass.powf(1.0 + D);
    remnant_mass *= baryon_mass_ns;

    remnant_mass
}

/// Dynamical ejecta mass for NSBH (Foucart et al. 2018, Eq. 9)
fn dynamic_mass_fitting_nsbh(
    _mass_bh: f64,
    _mass_ns: f64,
    comp_ns: f64,
    _chi_bh: f64,
    baryon_mass_ns: f64,
    risco: f64,
    mass_ratio_inv: f64,
) -> f64 {
    const A1: f64 = 7.11595154e-03;
    const A2: f64 = 1.43636803e-03;
    const A4: f64 = -2.76202990e-02;
    const N1: f64 = 8.63604211e-01;
    const N2: f64 = 1.68399507;

    let mdyn = A1 * mass_ratio_inv.powf(N1) * (1.0 - 2.0 * comp_ns) / comp_ns
        + -A2 * mass_ratio_inv.powf(N2) * risco
        + A4;

    let mdyn = mdyn * baryon_mass_ns;

    mdyn.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bns_ejecta_gw170817_like() {
        // GW170817-like parameters
        let params = BinaryParams {
            mass_1_source: 1.46,
            mass_2_source: 1.27,
            radius_1: 12.0,  // km
            radius_2: 11.5,
            chi_1: 0.0,
            chi_2: 0.0,
            tov_mass: 2.1,
            r_16: 12.0,
            ratio_zeta: 0.1,
            alpha: 0.0,
            ratio_epsilon: 2e-4,
        };

        let ejecta = compute_ejecta_properties(&params);

        println!("BNS Ejecta Properties (GW170817-like):");
        println!("  M_ej,dyn: {:.4} M_sun", ejecta.mej_dyn);
        println!("  M_ej,wind: {:.4} M_sun", ejecta.mej_wind);
        println!("  M_ej,total: {:.4} M_sun", ejecta.mej_total);
        println!("  v_ej,dyn: {:.3}c", ejecta.vej_dyn);
        println!("  M_disk: {:.4} M_sun", ejecta.mdisk);

        if let Some(ejet) = ejecta.ejet_grb {
            println!("  E_jet,GRB: {:.2e} erg", ejet);
        }

        // Typical expectations for GW170817
        assert!(ejecta.mej_total > 0.001);
        assert!(ejecta.mej_total < 0.1);
        assert!(ejecta.vej_dyn > 0.1);
        assert!(ejecta.vej_dyn < 0.5);
        assert_eq!(ejecta.binary_type, BinaryType::BNS);
    }

    #[test]
    fn test_nsbh_ejecta() {
        // NSBH parameters: 5 M_sun BH + 1.4 M_sun NS
        let params = BinaryParams {
            mass_1_source: 5.0,
            mass_2_source: 1.4,
            radius_1: 0.0,   // BH
            radius_2: 12.0,  // NS
            chi_1: 0.5,      // Moderate BH spin
            chi_2: 0.0,
            tov_mass: 2.1,
            r_16: 12.0,
            ratio_zeta: 0.1,
            alpha: 0.0,
            ratio_epsilon: 2e-4,
        };

        let ejecta = compute_ejecta_properties(&params);

        println!("\nNSBH Ejecta Properties:");
        println!("  M_ej,dyn: {:.4} M_sun", ejecta.mej_dyn);
        println!("  M_ej,wind: {:.4} M_sun", ejecta.mej_wind);
        println!("  M_ej,total: {:.4} M_sun", ejecta.mej_total);
        println!("  M_disk: {:.4} M_sun", ejecta.mdisk);

        assert_eq!(ejecta.binary_type, BinaryType::NSBH);

        // With moderate spin, should have some tidal disruption and ejecta
        // (exact values depend on mass ratio and spin)
    }

    #[test]
    fn test_bbh_no_ejecta() {
        let params = BinaryParams {
            mass_1_source: 30.0,
            mass_2_source: 25.0,
            radius_1: 0.0,
            radius_2: 0.0,
            chi_1: 0.0,
            chi_2: 0.0,
            tov_mass: 2.1,
            r_16: 12.0,
            ratio_zeta: 0.1,
            alpha: 0.0,
            ratio_epsilon: 2e-4,
        };

        let ejecta = compute_ejecta_properties(&params);

        assert_eq!(ejecta.binary_type, BinaryType::BBH);
        assert_eq!(ejecta.mej_total, 0.0);
        assert_eq!(ejecta.mdisk, 0.0);
        assert!(ejecta.ejet_grb.is_none());
    }

    #[test]
    fn test_risco_calculation() {
        // Test ISCO radius for various spins
        assert!((chi_bh_to_risco(0.0) - 6.0).abs() < 0.1); // Non-spinning: r_isco = 6 M
        assert!(chi_bh_to_risco(0.998) < 2.0); // Maximal prograde: r_isco ~ 1 M
        assert!(chi_bh_to_risco(-0.5) > 6.0); // Retrograde: r_isco > 6 M
    }
}
