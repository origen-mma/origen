//! Kilonova light curve synthesis and GW population model for injection campaigns.
//!
//! Generates realistic kilonova light curves from ejecta properties using the
//! MetzgerKN forward model, sampled at survey-specific cadences with photometric
//! noise. Also provides a GW event population model for drawing merger events
//! from astrophysically-motivated distributions.

use crate::ejecta_properties::BinaryParams;
use crate::grb_simulation::GwEventParams;
use mm_core::svi_models::metzger_kn_eval_batch;
use mm_core::{GWEvent, GpsTime, LightCurve, MockSkymap, Photometry, SkyPosition};
use rand::Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Survey model
// ---------------------------------------------------------------------------

/// Survey observation model for light curve sampling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyModel {
    pub name: String,
    /// Median revisit cadence (days)
    pub cadence_days: f64,
    /// Available photometric bands
    pub bands: Vec<String>,
    /// 5-sigma point-source limiting magnitude (AB)
    pub limiting_mag: f64,
    /// Systematic photometric noise floor (mag)
    pub mag_noise_floor: f64,
    /// Fraction of sky covered per night
    pub sky_fraction: f64,
    /// Astrometric uncertainty (arcsec)
    pub position_uncertainty: f64,
}

impl SurveyModel {
    /// Zwicky Transient Facility (O4-era)
    pub fn ztf() -> Self {
        Self {
            name: "ZTF".to_string(),
            cadence_days: 2.0,
            bands: vec!["g".to_string(), "r".to_string()],
            limiting_mag: 20.5,
            mag_noise_floor: 0.02,
            sky_fraction: 0.47,
            position_uncertainty: 1.0,
        }
    }

    /// Vera C. Rubin Observatory LSST
    pub fn lsst() -> Self {
        Self {
            name: "LSST".to_string(),
            cadence_days: 3.0,
            bands: vec![
                "g".to_string(),
                "r".to_string(),
                "i".to_string(),
                "z".to_string(),
            ],
            limiting_mag: 24.5,
            mag_noise_floor: 0.005,
            sky_fraction: 0.45,
            position_uncertainty: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// GW population model
// ---------------------------------------------------------------------------

/// Gravitational wave population model for BNS/NSBH mergers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GwPopulationModel {
    /// BNS detection horizon (Mpc)
    pub d_horizon_mpc: f64,
    /// NS mass distribution: mean (M_sun)
    pub ns_mass_mean: f64,
    /// NS mass distribution: std dev (M_sun)
    pub ns_mass_sigma: f64,
    /// NS mass minimum (M_sun)
    pub ns_mass_min: f64,
    /// NS mass maximum (M_sun)
    pub ns_mass_max: f64,
    /// NS radius (km) for EOS
    pub ns_radius_km: f64,
    /// TOV maximum NS mass (M_sun)
    pub tov_mass: f64,
}

impl GwPopulationModel {
    /// O4 observing run parameters (~190 Mpc BNS range)
    pub fn o4() -> Self {
        Self {
            d_horizon_mpc: 190.0,
            ns_mass_mean: 1.35,
            ns_mass_sigma: 0.15,
            ns_mass_min: 1.1,
            ns_mass_max: 2.0,
            ns_radius_km: 12.0,
            tov_mass: 2.17,
        }
    }

    /// O5 observing run parameters (~330 Mpc BNS range)
    pub fn o5() -> Self {
        Self {
            d_horizon_mpc: 330.0,
            ..Self::o4()
        }
    }
}

// ---------------------------------------------------------------------------
// Kilonova light curve parameters (bookkeeping)
// ---------------------------------------------------------------------------

/// Properties of a generated kilonova light curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnLightCurveParams {
    /// Peak absolute magnitude (bolometric)
    pub absolute_peak_mag: f64,
    /// Peak apparent magnitude at distance
    pub apparent_peak_mag: f64,
    /// Time of peak brightness after merger (days)
    pub peak_time_days: f64,
    /// Whether any observations are above survey detection limit
    pub detectable: bool,
    /// Number of detectable photometric points
    pub n_detections: usize,
}

// ---------------------------------------------------------------------------
// GW event drawing
// ---------------------------------------------------------------------------

/// Draw a GW event from the population model.
///
/// Returns (BinaryParams, GwEventParams, GWEvent, MockSkymap).
/// The GWEvent has `skymap: None` so the correlator uses point-source fallback.
pub fn draw_gw_event(
    pop: &GwPopulationModel,
    trigger_gps: f64,
    rng: &mut impl Rng,
) -> (BinaryParams, GwEventParams, GWEvent, MockSkymap) {
    // Distance: uniform in comoving volume ∝ d³ → CDF inversion: d = d_max * u^(1/3)
    let u: f64 = rng.gen();
    let distance = pop.d_horizon_mpc * u.cbrt();
    let distance = distance.max(1.0); // avoid d=0

    // Inclination: uniform in cos(i)
    let cos_i: f64 = rng.gen_range(-1.0..1.0);
    let inclination = cos_i.acos();

    // Redshift (Hubble flow approximation, valid for d < 500 Mpc)
    let z = distance * 70.0 / 3e5; // H0 = 70 km/s/Mpc

    // NS masses from truncated Gaussian
    let mass_dist = Normal::new(pop.ns_mass_mean, pop.ns_mass_sigma).unwrap();
    let mut m1 = mass_dist
        .sample(rng)
        .clamp(pop.ns_mass_min, pop.ns_mass_max);
    let mut m2 = mass_dist
        .sample(rng)
        .clamp(pop.ns_mass_min, pop.ns_mass_max);
    if m2 > m1 {
        std::mem::swap(&mut m1, &mut m2);
    }

    // Random sky position
    let ra = rng.gen_range(0.0..360.0);
    let sin_dec: f64 = rng.gen_range(-1.0..1.0);
    let dec = sin_dec.asin().to_degrees();

    let binary_params = BinaryParams {
        mass_1_source: m1,
        mass_2_source: m2,
        radius_1: pop.ns_radius_km,
        radius_2: pop.ns_radius_km,
        chi_1: 0.0,
        chi_2: 0.0,
        tov_mass: pop.tov_mass,
        r_16: pop.ns_radius_km,
        ratio_zeta: 0.1,
        alpha: 0.0,
        ratio_epsilon: 2e-4,
    };

    let gw_params = GwEventParams {
        inclination,
        distance,
        z,
    };

    let position = SkyPosition::new(ra, dec, 0.1); // GW centroid, small uncertainty

    // GW FAR: typical O4 range
    let far = 10f64.powf(rng.gen_range(-4.0..-1.0)); // 1e-4 to 0.1 per year

    let gw_event = GWEvent {
        superevent_id: format!("S{:.0}inj", trigger_gps),
        alert_type: "injection".to_string(),
        gps_time: GpsTime::from_seconds(trigger_gps),
        instruments: vec!["H1".to_string(), "L1".to_string()],
        far,
        position: Some(position.clone()),
        skymap: None,
    };

    // MockSkymap: area scales as distance²
    // GW170817 at 40 Mpc: ~28 sq deg 90% CR → radius_90 ~ 3.0 deg
    let radius_90 = (3.0 * distance / 40.0).min(30.0);
    let radius_50 = radius_90 / 2.0;
    let skymap = MockSkymap::new(ra, dec, radius_50, radius_90);

    (binary_params, gw_params, gw_event, skymap)
}

// ---------------------------------------------------------------------------
// Kilonova light curve generation
// ---------------------------------------------------------------------------

/// GPS seconds → MJD conversion
fn gps_to_mjd(gps: f64) -> f64 {
    let unix = gps + 315964800.0 - 18.0;
    unix / 86400.0 + 40587.0
}

/// AB magnitude → microJansky flux
fn mag_to_ujy(mag: f64) -> f64 {
    10f64.powf((23.9 - mag) / 2.5) * 1e6
}

/// Compute peak absolute magnitude from ejecta properties.
///
/// Empirical scaling calibrated to AT2017gfo:
///   M_peak = -15.8 - 2.5 * log10(M_ej / 0.05) - 1.25 * log10(v_ej / 0.3)
///
/// At GW170817 values (M_ej ≈ 0.05 M_sun, v_ej ≈ 0.3c) this gives M ≈ -15.8,
/// consistent with the observed i-band peak.
fn peak_absolute_mag(mej_solar: f64, vej_c: f64) -> f64 {
    let mej_ref = 0.05; // M_sun
    let vej_ref = 0.3; // units of c
    -15.8 - 2.5 * (mej_solar / mej_ref).log10() - 1.25 * (vej_c / vej_ref).log10()
}

/// Generate a synthetic kilonova light curve from ejecta properties.
///
/// Uses the MetzgerKN forward model to compute the light curve shape,
/// converts to apparent magnitudes at the given distance, samples at
/// survey cadence, and adds realistic photometric noise.
pub fn generate_kilonova_lightcurve(
    ejecta: &crate::ejecta_properties::EjectaProperties,
    gw_params: &GwEventParams,
    survey: &SurveyModel,
    trigger_gps: f64,
    rng: &mut impl Rng,
) -> (LightCurve, KnLightCurveParams) {
    let mej = ejecta.mej_total.max(1e-6);
    let vej = ejecta.vej_dyn.max(0.01); // units of c

    // Peak magnitude
    let abs_peak = peak_absolute_mag(mej, vej);
    let dist_mod = 5.0 * gw_params.distance.log10() + 25.0; // distance modulus
    let app_peak = abs_peak + dist_mod;

    // MetzgerKN parameters: [log10(M_ej/M_sun), log10(v_ej in cm/s), log10(κ), t0]
    let c_cgs = 2.998e10;
    let log10_mej = mej.log10();
    let log10_vej = (vej * c_cgs).log10();
    let log10_kappa = 1.0; // κ = 10 cm²/g → log10(10) = 1.0
    let t0_days = 0.0;
    let params = [log10_mej, log10_vej, log10_kappa, t0_days];

    // Generate observation schedule: start at random phase offset, every cadence_days
    let phase_offset: f64 = rng.gen_range(0.0..survey.cadence_days);
    let max_days = 14.0; // observe for 2 weeks
    let mut obs_phases = Vec::new();
    let mut t = phase_offset;
    while t <= max_days {
        obs_phases.push(t);
        t += survey.cadence_days;
    }

    if obs_phases.is_empty() {
        let kn_params = KnLightCurveParams {
            absolute_peak_mag: abs_peak,
            apparent_peak_mag: app_peak,
            peak_time_days: 1.0,
            detectable: false,
            n_detections: 0,
        };
        return (
            LightCurve::new(format!("KN_inj_{:.0}", trigger_gps)),
            kn_params,
        );
    }

    // Evaluate MetzgerKN forward model (returns normalized 0-1 flux)
    let norm_flux = metzger_kn_eval_batch(&params, &obs_phases);

    // Find peak normalized flux for the model shape (to find peak time)
    let peak_norm = norm_flux.iter().cloned().fold(0.0f64, f64::max);
    let peak_time_days = obs_phases
        .iter()
        .zip(norm_flux.iter())
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(t, _)| *t)
        .unwrap_or(1.0);

    // Build light curve
    let mut lc = LightCurve::new(format!("KN_inj_{:.0}", trigger_gps));
    let mut n_detections = 0;
    let n_bands = survey.bands.len();

    for (i, (&phase, &fnorm)) in obs_phases.iter().zip(norm_flux.iter()).enumerate() {
        if fnorm <= 0.0 || peak_norm <= 0.0 {
            continue;
        }

        // Apparent magnitude at this epoch
        // fnorm is already normalized to peak within the model, so:
        // m(t) = app_peak - 2.5 * log10(fnorm / peak_norm)
        let mag = app_peak - 2.5 * (fnorm / peak_norm).log10();

        // Photometric SNR: SNR = 5 * 10^((lim - m) / 2.5)
        let snr = 5.0 * 10f64.powf((survey.limiting_mag - mag) / 2.5);
        if snr < 3.0 {
            continue; // below detection threshold
        }

        // Photometric noise
        let sigma_mag = (survey.mag_noise_floor.powi(2) + (1.0857 / snr).powi(2)).sqrt();
        let noise_dist = Normal::new(0.0, sigma_mag).unwrap();
        let mag_obs = mag + noise_dist.sample(rng);

        if mag_obs > survey.limiting_mag {
            continue; // noised below limit
        }

        // Convert to flux
        let flux = mag_to_ujy(mag_obs);
        let flux_err = flux / snr;

        // Pick band (cycle through bands)
        let band = &survey.bands[i % n_bands];

        // Convert GPS time to MJD
        let obs_gps = trigger_gps + phase * 86400.0;
        let mjd = gps_to_mjd(obs_gps);

        lc.add_measurement(Photometry::new(mjd, flux, flux_err, band.clone()));
        n_detections += 1;
    }

    let detectable = n_detections >= 2; // need ≥2 points for correlator

    let kn_params = KnLightCurveParams {
        absolute_peak_mag: abs_peak,
        apparent_peak_mag: app_peak,
        peak_time_days,
        detectable,
        n_detections,
    };

    (lc, kn_params)
}

/// Convert a background optical transient to a LightCurve sampled at survey cadence.
pub fn background_to_lightcurve(
    bg: &crate::background_optical::BackgroundOpticalTransient,
    survey: &SurveyModel,
    window_start_gps: f64,
    window_end_gps: f64,
    rng: &mut impl Rng,
) -> LightCurve {
    let mut lc = LightCurve::new(bg.transient_id.clone());
    let n_bands = survey.bands.len();

    // Sample at cadence within the window
    let phase_offset: f64 = rng.gen_range(0.0..survey.cadence_days);
    let mut t_gps = window_start_gps + phase_offset * 86400.0;
    let mut i = 0;

    while t_gps <= window_end_gps {
        let mag = bg.magnitude_at_time(t_gps);

        if mag < survey.limiting_mag {
            let snr = 5.0 * 10f64.powf((survey.limiting_mag - mag) / 2.5);
            if snr >= 3.0 {
                let sigma_mag = (survey.mag_noise_floor.powi(2) + (1.0857 / snr).powi(2)).sqrt();
                let noise_dist = Normal::new(0.0, sigma_mag).unwrap();
                let mag_obs = mag + noise_dist.sample(rng);

                if mag_obs < survey.limiting_mag {
                    let flux = mag_to_ujy(mag_obs);
                    let flux_err = flux / snr;
                    let band = &survey.bands[i % n_bands];
                    let mjd = gps_to_mjd(t_gps);
                    lc.add_measurement(Photometry::new(mjd, flux, flux_err, band.clone()));
                }
            }
        }

        t_gps += survey.cadence_days * 86400.0;
        i += 1;
    }

    lc
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ejecta_properties::BinaryType;
    use rand::SeedableRng;

    #[test]
    fn test_peak_mag_gw170817() {
        // AT2017gfo: M_ej ~ 0.05 M_sun, v_ej ~ 0.3c → M_peak ~ -15.8
        let mag = peak_absolute_mag(0.05, 0.3);
        assert!((mag - (-15.8)).abs() < 0.01, "Expected ~-15.8, got {}", mag);
    }

    #[test]
    fn test_peak_mag_scaling() {
        // More ejecta → brighter
        let mag_low = peak_absolute_mag(0.01, 0.2);
        let mag_high = peak_absolute_mag(0.1, 0.2);
        assert!(mag_high < mag_low, "More ejecta should be brighter");

        // Faster ejecta → brighter
        let mag_slow = peak_absolute_mag(0.05, 0.1);
        let mag_fast = peak_absolute_mag(0.05, 0.4);
        assert!(mag_fast < mag_slow, "Faster ejecta should be brighter");
    }

    #[test]
    fn test_gw170817_like_kn() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let ejecta = crate::ejecta_properties::EjectaProperties {
            mej_dyn: 0.01,
            mej_wind: 0.04,
            mej_total: 0.05,
            vej_dyn: 0.3,
            vej_wind: 0.1,
            mdisk: 0.1,
            ejet_grb: None,
            binary_type: BinaryType::BNS,
        };
        let gw_params = GwEventParams {
            inclination: 0.44,
            distance: 40.0,
            z: 0.0093,
        };
        let survey = SurveyModel::ztf();
        let trigger_gps = 1187008882.0; // GW170817

        let (lc, params) =
            generate_kilonova_lightcurve(&ejecta, &gw_params, &survey, trigger_gps, &mut rng);

        // At 40 Mpc, dist mod = 5*log10(40)+25 = 33.0
        // App peak ~ -15.8 + 33.0 = 17.2
        assert!(
            params.apparent_peak_mag < 20.0,
            "Should be bright at 40 Mpc, got {}",
            params.apparent_peak_mag
        );
        assert!(params.detectable, "Should be detectable by ZTF at 40 Mpc");
        assert!(
            lc.measurements.len() >= 2,
            "Should have multiple detections, got {}",
            lc.measurements.len()
        );
    }

    #[test]
    fn test_distance_scaling() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        let ejecta = crate::ejecta_properties::EjectaProperties {
            mej_dyn: 0.01,
            mej_wind: 0.04,
            mej_total: 0.05,
            vej_dyn: 0.3,
            vej_wind: 0.1,
            mdisk: 0.1,
            ejet_grb: None,
            binary_type: BinaryType::BNS,
        };
        let survey = SurveyModel::ztf();

        let near = GwEventParams {
            inclination: 0.3,
            distance: 40.0,
            z: 0.009,
        };
        let far = GwEventParams {
            inclination: 0.3,
            distance: 200.0,
            z: 0.047,
        };

        let (_, p_near) = generate_kilonova_lightcurve(&ejecta, &near, &survey, 1e9, &mut rng);
        let (_, p_far) = generate_kilonova_lightcurve(&ejecta, &far, &survey, 1e9, &mut rng);

        // 200 Mpc should be ~3.5 mag fainter than 40 Mpc
        let expected_diff = 5.0 * (200.0_f64 / 40.0).log10();
        let actual_diff = p_far.apparent_peak_mag - p_near.apparent_peak_mag;
        assert!(
            (actual_diff - expected_diff).abs() < 0.1,
            "Distance scaling wrong: expected {:.1} mag diff, got {:.1}",
            expected_diff,
            actual_diff
        );
    }

    #[test]
    fn test_population_draw() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);
        let pop = GwPopulationModel::o4();
        let n = 1000;

        let mut distances = Vec::new();
        let mut masses = Vec::new();

        for i in 0..n {
            let gps = 1e9 + i as f64 * 1000.0;
            let (bp, _gw_params, _event, _skymap) = draw_gw_event(&pop, gps, &mut rng);
            distances.push(_gw_params.distance);
            masses.push(bp.mass_1_source);
            masses.push(bp.mass_2_source);

            // Basic sanity
            assert!(bp.mass_1_source >= bp.mass_2_source);
            assert!(bp.mass_1_source <= pop.ns_mass_max);
            assert!(bp.mass_2_source >= pop.ns_mass_min);
        }

        // Distance should follow d² distribution → median at d_horizon * 0.5^(1/3) ≈ 0.794
        distances.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_d = distances[n / 2];
        let expected_median = pop.d_horizon_mpc * 0.5_f64.cbrt();
        assert!(
            (median_d - expected_median).abs() / expected_median < 0.1,
            "Median distance {:.1} should be ~{:.1}",
            median_d,
            expected_median
        );

        // Mass mean should be close to 1.35
        let mean_mass: f64 = masses.iter().sum::<f64>() / masses.len() as f64;
        assert!(
            (mean_mass - 1.35).abs() < 0.05,
            "Mean mass {:.3} should be ~1.35",
            mean_mass
        );
    }

    #[test]
    fn test_survey_presets() {
        let ztf = SurveyModel::ztf();
        assert_eq!(ztf.bands.len(), 2);
        assert!((ztf.cadence_days - 2.0).abs() < 0.01);
        assert!((ztf.limiting_mag - 20.5).abs() < 0.01);

        let lsst = SurveyModel::lsst();
        assert_eq!(lsst.bands.len(), 4);
        assert!(lsst.limiting_mag > ztf.limiting_mag);
    }

    #[test]
    fn test_mag_to_flux_roundtrip() {
        // mag_to_ujy must round-trip with Photometry::magnitude()
        // Photometry::magnitude() uses (flux / 1e6).log10(), so flux values
        // are 1e6× larger than physical μJy.
        for &mag in &[16.0, 18.0, 20.0, 22.0, 24.0] {
            let flux = mag_to_ujy(mag);
            let phot = Photometry::new(60000.0, flux, 1.0, "g".to_string());
            let recovered = phot.magnitude().unwrap();
            assert!(
                (recovered - mag).abs() < 1e-10,
                "Round-trip failed: input mag={}, recovered={}",
                mag,
                recovered
            );
        }
    }
}
