//! Integration tests for the FAR tuning injection campaign.
//!
//! Tests the end-to-end pipeline: population draw → ejecta → kilonova
//! light curve → background generation. Campaign tests that exercise the
//! correlator are in the unit test suite (`far_campaign::tests`).

use mm_simulation::far_campaign::{CampaignConfig, CampaignResults};
use mm_simulation::optical_injection::{
    background_to_lightcurve, draw_gw_event, generate_kilonova_lightcurve, GwPopulationModel,
    SurveyModel,
};
use mm_simulation::{compute_ejecta_properties, BackgroundOpticalConfig};
use rand::SeedableRng;

/// Campaign results should serialize to valid JSON and round-trip cleanly.
#[test]
fn test_campaign_results_serde() {
    let results = CampaignResults {
        n_injections: 10,
        n_detectable: 4,
        n_recovered: 1,
        n_background_tested: 20,
        n_background_false: 2,
        median_injection_distance: 150.0,
        injection_outcomes: vec![],
        background_outcomes: vec![],
        roc_curve: vec![],
        efficiency_vs_distance: vec![(50.0, 0.8), (100.0, 0.5), (200.0, 0.1)],
    };

    let json = serde_json::to_string_pretty(&results).unwrap();
    let roundtrip: CampaignResults = serde_json::from_str(&json).unwrap();

    assert_eq!(roundtrip.n_injections, 10);
    assert_eq!(roundtrip.n_detectable, 4);
    assert_eq!(roundtrip.n_recovered, 1);
    assert_eq!(roundtrip.efficiency_vs_distance.len(), 3);
    assert!((roundtrip.efficiency_vs_distance[0].1 - 0.8).abs() < 1e-10);
}

/// CampaignConfig presets should serialize (except the skipped correlator_config).
#[test]
fn test_campaign_config_serde() {
    let config = CampaignConfig::quick_ztf();
    let json = serde_json::to_string_pretty(&config).unwrap();
    let roundtrip: CampaignConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(roundtrip.n_injections, config.n_injections);
    assert_eq!(roundtrip.seed, config.seed);
    assert!((roundtrip.gw_pop.d_horizon_mpc - 190.0).abs() < 0.1);
}

/// Population draw → ejecta → KN light curve should produce physically
/// reasonable values for all parameter combinations.
#[test]
fn test_injection_outcome_physical_ranges() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(404);
    let pop = GwPopulationModel::o4();
    let survey = SurveyModel::ztf();

    for i in 0..50 {
        let gps = 1e9 + i as f64 * 86400.0;
        let (bp, gw_params, _event, _skymap) = draw_gw_event(&pop, gps, &mut rng);

        // Distance within horizon
        assert!(gw_params.distance > 0.0 && gw_params.distance <= pop.d_horizon_mpc);

        // Inclination in [0, pi]
        assert!(gw_params.inclination >= 0.0 && gw_params.inclination <= std::f64::consts::PI);

        // Ejecta
        let ejecta = compute_ejecta_properties(&bp);
        assert!(ejecta.mej_total > 0.0, "Ejecta mass should be positive");
        assert!(
            ejecta.vej_dyn > 0.0 && ejecta.vej_dyn < 1.0,
            "Ejecta velocity {:.3} should be in (0, c)",
            ejecta.vej_dyn
        );

        // KN light curve
        let (_lc, kn_params) =
            generate_kilonova_lightcurve(&ejecta, &gw_params, &survey, gps, &mut rng);

        // Peak magnitudes should be physical
        assert!(
            kn_params.absolute_peak_mag < 0.0,
            "Absolute mag {:.1} should be negative (bright)",
            kn_params.absolute_peak_mag
        );
        assert!(
            kn_params.apparent_peak_mag > kn_params.absolute_peak_mag,
            "Apparent ({:.1}) > absolute ({:.1}) for d > 10 pc",
            kn_params.apparent_peak_mag,
            kn_params.absolute_peak_mag
        );

        // If detectable, should have >= 2 LC points
        if kn_params.detectable {
            assert!(
                kn_params.n_detections >= 2,
                "Detectable requires >= 2 detections, got {}",
                kn_params.n_detections
            );
        }
    }
}

/// GW population draws should follow the expected d³ volume distribution.
#[test]
fn test_population_distance_distribution() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(505);
    let pop = GwPopulationModel::o4();
    let n = 5000;

    let mut distances: Vec<f64> = (0..n)
        .map(|i| {
            let (_, gw, _, _) = draw_gw_event(&pop, 1e9 + i as f64 * 100.0, &mut rng);
            gw.distance
        })
        .collect();

    distances.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Median of d ∝ d² CDF: d_median = d_max * 0.5^(1/3) ≈ 0.794 * d_max
    let median = distances[n / 2];
    let expected_median = pop.d_horizon_mpc * 0.5_f64.cbrt();
    let frac_error = (median - expected_median).abs() / expected_median;
    assert!(
        frac_error < 0.05,
        "Median distance {:.1} should be ~{:.1} Mpc (error {:.1}%)",
        median,
        expected_median,
        frac_error * 100.0
    );

    // 90th percentile: d_90 = d_max * 0.9^(1/3) ≈ 0.965 * d_max
    let p90 = distances[(n as f64 * 0.9) as usize];
    let expected_p90 = pop.d_horizon_mpc * 0.9_f64.cbrt();
    let frac_error_90 = (p90 - expected_p90).abs() / expected_p90;
    assert!(
        frac_error_90 < 0.05,
        "90th percentile {:.1} should be ~{:.1} Mpc",
        p90,
        expected_p90
    );
}

/// A nearby GW170817-like event should be detectable and have a bright KN.
#[test]
fn test_nearby_kn_always_detectable() {
    let survey = SurveyModel::ztf();

    // GW170817-like ejecta at 40 Mpc
    let ejecta = mm_simulation::EjectaProperties {
        mej_dyn: 0.01,
        mej_wind: 0.04,
        mej_total: 0.05,
        vej_dyn: 0.3,
        vej_wind: 0.1,
        mdisk: 0.1,
        ejet_grb: None,
        binary_type: mm_simulation::BinaryType::BNS,
    };
    let gw_params = mm_simulation::grb_simulation::GwEventParams {
        inclination: 0.3,
        distance: 40.0,
        z: 0.009,
    };

    // Run 20 trials with different random cadence offsets
    let mut n_detectable = 0;
    for seed_offset in 0..20 {
        let mut trial_rng = rand::rngs::StdRng::seed_from_u64(606 + seed_offset);
        let (_lc, params) = generate_kilonova_lightcurve(
            &ejecta,
            &gw_params,
            &survey,
            1187008882.0,
            &mut trial_rng,
        );
        if params.detectable {
            n_detectable += 1;
        }
        // At 40 Mpc the peak should be ~17 mag, well above ZTF limit of 20.5
        assert!(
            params.apparent_peak_mag < 19.0,
            "At 40 Mpc, peak should be < 19 mag, got {:.1}",
            params.apparent_peak_mag
        );
    }

    assert!(
        n_detectable >= 18,
        "GW170817-like KN at 40 Mpc should be detectable in >=90% of trials, got {}/20",
        n_detectable
    );
}

/// LSST survey should detect KNe at larger distances than ZTF.
#[test]
fn test_lsst_deeper_than_ztf() {
    let ztf = SurveyModel::ztf();
    let lsst = SurveyModel::lsst();

    let ejecta = mm_simulation::EjectaProperties {
        mej_dyn: 0.005,
        mej_wind: 0.02,
        mej_total: 0.025,
        vej_dyn: 0.25,
        vej_wind: 0.1,
        mdisk: 0.05,
        ejet_grb: None,
        binary_type: mm_simulation::BinaryType::BNS,
    };

    // At 200 Mpc: ZTF should barely detect, LSST should easily detect
    let gw_far = mm_simulation::grb_simulation::GwEventParams {
        inclination: 0.3,
        distance: 200.0,
        z: 0.047,
    };

    let mut ztf_detections = 0;
    let mut lsst_detections = 0;
    for seed in 0..30 {
        let mut r = rand::rngs::StdRng::seed_from_u64(707 + seed);
        let (_, p_ztf) = generate_kilonova_lightcurve(&ejecta, &gw_far, &ztf, 1e9, &mut r);
        let mut r2 = rand::rngs::StdRng::seed_from_u64(707 + seed);
        let (_, p_lsst) = generate_kilonova_lightcurve(&ejecta, &gw_far, &lsst, 1e9, &mut r2);
        if p_ztf.detectable {
            ztf_detections += 1;
        }
        if p_lsst.detectable {
            lsst_detections += 1;
        }
    }

    assert!(
        lsst_detections > ztf_detections,
        "LSST should detect more KNe at 200 Mpc: LSST={}/30, ZTF={}/30",
        lsst_detections,
        ztf_detections
    );
}

/// Background optical transients should produce valid light curves at survey cadence.
#[test]
fn test_background_lightcurve_generation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(808);
    let survey = SurveyModel::ztf();
    let bg_config = BackgroundOpticalConfig::ztf();

    let t_start = 1.4e9;
    let t_end = t_start + 7.0 * 86400.0; // 7 days

    let transients =
        mm_simulation::generate_background_optical(&bg_config, t_start, t_end, &mut rng);

    assert!(
        !transients.is_empty(),
        "Should generate background transients"
    );

    // Convert a subset to light curves and check they're valid
    let mut n_with_points = 0;
    for bg in transients.iter().take(50) {
        let lc = background_to_lightcurve(bg, &survey, t_start, t_end, &mut rng);
        for m in &lc.measurements {
            assert!(m.flux > 0.0, "Flux must be positive");
            assert!(m.flux_err > 0.0, "Flux error must be positive");
            assert!(m.mjd > 0.0, "MJD must be positive");
        }
        if !lc.measurements.is_empty() {
            n_with_points += 1;
        }
    }

    assert!(
        n_with_points > 0,
        "At least some background transients should produce detectable LC points"
    );
}
