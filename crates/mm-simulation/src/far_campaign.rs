//! Injection campaign for FAR tuning.
//!
//! Runs a Monte Carlo campaign: for each trial, draws a GW event from the
//! population model, generates a kilonova signal light curve plus background
//! optical transients at realistic rates, feeds everything through the
//! `SupereventCorrelator`, and records which signals are recovered and which
//! backgrounds are falsely associated. The results are used to build ROC
//! curves for choosing optimal FAR thresholds.

use crate::background_optical::{generate_background_optical, BackgroundOpticalConfig};
use crate::ejecta_properties::compute_ejecta_properties;
use crate::optical_injection::{
    background_to_lightcurve, draw_gw_event, generate_kilonova_lightcurve, GwPopulationModel,
    SurveyModel,
};
use mm_core::{Event, SkyPosition};
use mm_correlator::{CorrelatorConfig, SupereventCorrelator};
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Campaign configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignConfig {
    /// Number of signal injections to perform
    pub n_injections: usize,
    /// Observing window per injection (days)
    pub observing_window_days: f64,
    /// Survey model (ZTF or LSST)
    pub survey: SurveyModel,
    /// GW population model
    pub gw_pop: GwPopulationModel,
    /// Background optical transient config
    pub background_config: BackgroundOpticalConfig,
    /// Correlator configuration (use without_lc_filter for speed)
    #[serde(skip)]
    pub correlator_config: CorrelatorConfig,
    /// Random seed for reproducibility
    pub seed: u64,
    /// FAR thresholds to evaluate for ROC curve
    pub far_thresholds: Vec<f64>,
}

impl CampaignConfig {
    /// Quick ZTF campaign (100 injections, good for testing)
    pub fn quick_ztf() -> Self {
        Self {
            n_injections: 100,
            observing_window_days: 14.0,
            survey: SurveyModel::ztf(),
            gw_pop: GwPopulationModel::o4(),
            background_config: BackgroundOpticalConfig::ztf(),
            correlator_config: CorrelatorConfig::without_lc_filter(),
            seed: 42,
            far_thresholds: log_spaced_thresholds(1e-6, 10.0, 15),
        }
    }

    /// Full ZTF campaign (1000 injections)
    pub fn full_ztf() -> Self {
        Self {
            n_injections: 1000,
            ..Self::quick_ztf()
        }
    }

    /// LSST campaign
    pub fn lsst() -> Self {
        Self {
            n_injections: 1000,
            observing_window_days: 14.0,
            survey: SurveyModel::lsst(),
            gw_pop: GwPopulationModel::o5(),
            background_config: BackgroundOpticalConfig::lsst(),
            correlator_config: CorrelatorConfig::without_lc_filter(),
            seed: 42,
            far_thresholds: log_spaced_thresholds(1e-6, 10.0, 15),
        }
    }
}

fn log_spaced_thresholds(min: f64, max: f64, n: usize) -> Vec<f64> {
    let log_min = min.log10();
    let log_max = max.log10();
    (0..n)
        .map(|i| 10f64.powf(log_min + (log_max - log_min) * i as f64 / (n - 1) as f64))
        .collect()
}

// ---------------------------------------------------------------------------
// Outcome structs
// ---------------------------------------------------------------------------

/// Outcome of a single signal injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionOutcome {
    pub injection_id: usize,
    pub distance_mpc: f64,
    pub inclination_rad: f64,
    pub mej_total: f64,
    pub vej_dyn: f64,
    pub absolute_peak_mag: f64,
    pub apparent_peak_mag: f64,
    pub n_lc_detections: usize,
    /// Whether the KN was bright enough for >=2 survey detections
    pub detectable: bool,
    /// Whether the correlator matched the signal to the GW event
    pub recovered: bool,
    /// Joint FAR assigned by the correlator (if recovered)
    pub joint_far: Option<f64>,
}

/// Outcome of a single background transient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundOutcome {
    pub injection_id: usize,
    pub transient_id: String,
    /// Whether the correlator falsely associated this with a GW event
    pub falsely_associated: bool,
    /// Joint FAR assigned if falsely associated
    pub joint_far: Option<f64>,
}

/// A single point on the ROC curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocPoint {
    pub far_threshold: f64,
    /// Fraction of detectable injections recovered
    pub efficiency: f64,
    /// Fraction of background transients falsely associated
    pub false_positive_rate: f64,
    pub n_signal_recovered: usize,
    pub n_background_false: usize,
}

/// Aggregate campaign results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignResults {
    pub n_injections: usize,
    pub n_detectable: usize,
    pub n_recovered: usize,
    pub n_background_tested: usize,
    pub n_background_false: usize,
    pub median_injection_distance: f64,
    pub injection_outcomes: Vec<InjectionOutcome>,
    pub background_outcomes: Vec<BackgroundOutcome>,
    pub roc_curve: Vec<RocPoint>,
    /// Distance bins: (d_max, efficiency) for detection efficiency vs distance
    pub efficiency_vs_distance: Vec<(f64, f64)>,
}

// ---------------------------------------------------------------------------
// Campaign runner
// ---------------------------------------------------------------------------

/// Run the full injection campaign.
///
/// For each injection:
/// 1. Draws a GW event from the population
/// 2. Computes BNS ejecta properties
/// 3. Generates a kilonova light curve with survey noise
/// 4. Generates background optical transients at realistic rates
/// 5. Feeds GW event + all light curves through the correlator
/// 6. Records whether signal is recovered and at what FAR
pub fn run_injection_campaign(config: &CampaignConfig) -> CampaignResults {
    let mut rng = rand::rngs::StdRng::seed_from_u64(config.seed);

    let mut injection_outcomes = Vec::with_capacity(config.n_injections);
    let mut background_outcomes = Vec::new();

    // Base GPS time for the campaign
    let base_gps = 1.4e9; // ~2024

    for i in 0..config.n_injections {
        let trigger_gps = base_gps + i as f64 * config.observing_window_days * 86400.0;

        // 1. Draw GW event
        let (binary_params, gw_params, gw_event, mock_skymap) =
            draw_gw_event(&config.gw_pop, trigger_gps, &mut rng);

        // 2. Compute ejecta
        let ejecta = compute_ejecta_properties(&binary_params);

        // 3. Generate signal KN light curve
        let (signal_lc, kn_params) = generate_kilonova_lightcurve(
            &ejecta,
            &gw_params,
            &config.survey,
            trigger_gps,
            &mut rng,
        );

        // 4. Generate background transients for this window
        let window_end = trigger_gps + config.observing_window_days * 86400.0;
        let bg_transients = generate_background_optical(
            &config.background_config,
            trigger_gps,
            window_end,
            &mut rng,
        );

        if (i + 1) % 100 == 0 || i == 0 {
            info!(
                "Injection {}/{}: d={:.0} Mpc, m_peak={:.1}, detectable={}, bg={}",
                i + 1,
                config.n_injections,
                gw_params.distance,
                kn_params.apparent_peak_mag,
                kn_params.detectable,
                bg_transients.len(),
            );
        }

        // 5. Set up correlator with adjusted spatial threshold for this event
        let mut corr_config = config.correlator_config.clone();
        // Spatial threshold must cover the skymap — use 2× radius_90
        corr_config.spatial_threshold = (mock_skymap.radius_90 * 2.0).max(5.0);

        let mut correlator = SupereventCorrelator::new(corr_config);

        // Feed GW event
        let gw_position = gw_event.position.clone();
        if let Err(e) = correlator.process_gcn_event(Event::GravitationalWave(gw_event)) {
            warn!("Injection {}: failed to process GW event: {}", i, e);
            continue;
        }

        // 6. Feed signal KN light curve (position sampled from skymap)
        let signal_recovered;
        let signal_far;
        if kn_params.detectable && signal_lc.measurements.len() >= 2 {
            let signal_pos = mock_skymap.sample_position(&mut rng);
            match correlator.process_optical_lightcurve(&signal_lc, &signal_pos) {
                Ok(matched_ids) => {
                    if matched_ids.is_empty() {
                        signal_recovered = false;
                        signal_far = None;
                    } else {
                        signal_recovered = true;
                        // Get the best (lowest) FAR across matched superevents
                        signal_far = matched_ids
                            .iter()
                            .filter_map(|id| {
                                correlator.get_superevent(id).and_then(|se| {
                                    se.optical_candidates
                                        .iter()
                                        .filter_map(|c| c.joint_far)
                                        .fold(None, |acc: Option<f64>, f| {
                                            Some(acc.map_or(f, |a: f64| a.min(f)))
                                        })
                                })
                            })
                            .fold(None, |acc: Option<f64>, f| {
                                Some(acc.map_or(f, |a: f64| a.min(f)))
                            });
                    }
                }
                Err(e) => {
                    debug!("Injection {}: correlator error for signal: {}", i, e);
                    signal_recovered = false;
                    signal_far = None;
                }
            }
        } else {
            signal_recovered = false;
            signal_far = None;
        }

        injection_outcomes.push(InjectionOutcome {
            injection_id: i,
            distance_mpc: gw_params.distance,
            inclination_rad: gw_params.inclination,
            mej_total: ejecta.mej_total,
            vej_dyn: ejecta.vej_dyn,
            absolute_peak_mag: kn_params.absolute_peak_mag,
            apparent_peak_mag: kn_params.apparent_peak_mag,
            n_lc_detections: kn_params.n_detections,
            detectable: kn_params.detectable,
            recovered: signal_recovered,
            joint_far: signal_far,
        });

        // 7. Feed background transients
        // Only test a subset near the GW position to keep computation tractable
        let gw_pos = gw_position.unwrap_or_else(|| SkyPosition::new(180.0, 0.0, 1.0));
        let search_radius = mock_skymap.radius_90 * 3.0; // generous search radius

        for bg in &bg_transients {
            let bg_pos = SkyPosition::new(bg.ra, bg.dec, 1.0);
            let sep = gw_pos.angular_separation(&bg_pos);

            // Skip transients far from GW event (will never be spatially coincident)
            if sep > search_radius {
                continue;
            }

            let bg_lc =
                background_to_lightcurve(bg, &config.survey, trigger_gps, window_end, &mut rng);

            if bg_lc.measurements.len() < 2 {
                continue;
            }

            let (bg_associated, bg_far) =
                match correlator.process_optical_lightcurve(&bg_lc, &bg_pos) {
                    Ok(matched_ids) => {
                        if matched_ids.is_empty() {
                            (false, None)
                        } else {
                            let best_far = matched_ids
                                .iter()
                                .filter_map(|id| {
                                    correlator.get_superevent(id).and_then(|se| {
                                        se.optical_candidates
                                            .iter()
                                            .filter(|c| c.object_id == bg.transient_id)
                                            .filter_map(|c| c.joint_far)
                                            .fold(None, |acc: Option<f64>, f| {
                                                Some(acc.map_or(f, |a: f64| a.min(f)))
                                            })
                                    })
                                })
                                .fold(None, |acc: Option<f64>, f| {
                                    Some(acc.map_or(f, |a: f64| a.min(f)))
                                });
                            (true, best_far)
                        }
                    }
                    Err(_) => (false, None),
                };

            background_outcomes.push(BackgroundOutcome {
                injection_id: i,
                transient_id: bg.transient_id.clone(),
                falsely_associated: bg_associated,
                joint_far: bg_far,
            });
        }
    }

    // Compute aggregate statistics
    let n_detectable = injection_outcomes.iter().filter(|o| o.detectable).count();
    let n_recovered = injection_outcomes.iter().filter(|o| o.recovered).count();
    let n_background_tested = background_outcomes.len();
    let n_background_false = background_outcomes
        .iter()
        .filter(|o| o.falsely_associated)
        .count();

    let mut distances: Vec<f64> = injection_outcomes.iter().map(|o| o.distance_mpc).collect();
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_distance = if distances.is_empty() {
        0.0
    } else {
        distances[distances.len() / 2]
    };

    info!("=== Campaign Summary ===");
    info!("Injections: {}", config.n_injections);
    info!(
        "Detectable: {} ({:.1}%)",
        n_detectable,
        100.0 * n_detectable as f64 / config.n_injections as f64
    );
    info!(
        "Recovered: {} ({:.1}% of detectable)",
        n_recovered,
        if n_detectable > 0 {
            100.0 * n_recovered as f64 / n_detectable as f64
        } else {
            0.0
        }
    );
    info!(
        "Background tested: {}, falsely associated: {} ({:.2}%)",
        n_background_tested,
        n_background_false,
        if n_background_tested > 0 {
            100.0 * n_background_false as f64 / n_background_tested as f64
        } else {
            0.0
        }
    );
    info!("Median injection distance: {:.1} Mpc", median_distance);

    // Compute ROC curve
    let roc_curve = compute_roc(
        &injection_outcomes,
        &background_outcomes,
        &config.far_thresholds,
    );

    // Compute efficiency vs distance
    let dist_bins = vec![50.0, 100.0, 150.0, 200.0, 300.0];
    let efficiency_vs_distance = compute_efficiency_vs_distance(&injection_outcomes, &dist_bins);

    CampaignResults {
        n_injections: config.n_injections,
        n_detectable,
        n_recovered,
        n_background_tested,
        n_background_false,
        median_injection_distance: median_distance,
        injection_outcomes,
        background_outcomes,
        roc_curve,
        efficiency_vs_distance,
    }
}

// ---------------------------------------------------------------------------
// ROC and analysis
// ---------------------------------------------------------------------------

/// Compute the ROC curve by sweeping FAR thresholds.
fn compute_roc(
    injections: &[InjectionOutcome],
    backgrounds: &[BackgroundOutcome],
    thresholds: &[f64],
) -> Vec<RocPoint> {
    let n_detectable = injections.iter().filter(|o| o.detectable).count();
    let n_bg = backgrounds.len();

    thresholds
        .iter()
        .map(|&thresh| {
            let n_signal = injections
                .iter()
                .filter(|o| o.detectable && o.joint_far.is_some_and(|f| f <= thresh))
                .count();

            let n_false = backgrounds
                .iter()
                .filter(|o| o.joint_far.is_some_and(|f| f <= thresh))
                .count();

            RocPoint {
                far_threshold: thresh,
                efficiency: if n_detectable > 0 {
                    n_signal as f64 / n_detectable as f64
                } else {
                    0.0
                },
                false_positive_rate: if n_bg > 0 {
                    n_false as f64 / n_bg as f64
                } else {
                    0.0
                },
                n_signal_recovered: n_signal,
                n_background_false: n_false,
            }
        })
        .collect()
}

/// Compute detection efficiency in distance bins.
fn compute_efficiency_vs_distance(
    injections: &[InjectionOutcome],
    dist_bins: &[f64],
) -> Vec<(f64, f64)> {
    let mut prev = 0.0;
    dist_bins
        .iter()
        .map(|&d_max| {
            let in_bin: Vec<&InjectionOutcome> = injections
                .iter()
                .filter(|o| o.distance_mpc > prev && o.distance_mpc <= d_max)
                .collect();
            let n_total = in_bin.len();
            let n_recovered = in_bin.iter().filter(|o| o.recovered).count();
            let eff = if n_total > 0 {
                n_recovered as f64 / n_total as f64
            } else {
                0.0
            };
            prev = d_max;
            (d_max, eff)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_campaign() {
        // Run a minimal campaign to verify no panics.
        // Use short window + low background rate to keep debug-mode fast.
        let mut config = CampaignConfig::quick_ztf();
        config.n_injections = 3;
        config.observing_window_days = 1.0;
        config.background_config.rate_per_day = 10.0; // very few BG transients
        config.seed = 77;

        let results = run_injection_campaign(&config);

        assert_eq!(results.injection_outcomes.len(), 3);
        assert!(!results.roc_curve.is_empty());

        // All injection outcomes should have valid distances
        for outcome in &results.injection_outcomes {
            assert!(outcome.distance_mpc > 0.0);
            assert!(outcome.distance_mpc <= config.gw_pop.d_horizon_mpc);
        }
    }

    #[test]
    fn test_roc_monotonic() {
        let mut config = CampaignConfig::quick_ztf();
        config.n_injections = 10;
        config.observing_window_days = 1.0;
        config.background_config.rate_per_day = 10.0;
        config.seed = 88;

        let results = run_injection_campaign(&config);

        // ROC efficiency should be monotonically non-decreasing with FAR threshold
        for window in results.roc_curve.windows(2) {
            assert!(
                window[1].efficiency >= window[0].efficiency,
                "ROC efficiency should be non-decreasing: {} at FAR={:.2e} vs {} at FAR={:.2e}",
                window[0].efficiency,
                window[0].far_threshold,
                window[1].efficiency,
                window[1].far_threshold,
            );
        }
    }

    #[test]
    fn test_log_spaced_thresholds() {
        let thresholds = log_spaced_thresholds(1e-4, 1.0, 5);
        assert_eq!(thresholds.len(), 5);
        assert!((thresholds[0] - 1e-4).abs() < 1e-6);
        assert!((thresholds[4] - 1.0).abs() < 1e-6);
        // Should be monotonically increasing
        for w in thresholds.windows(2) {
            assert!(w[1] > w[0]);
        }
    }
}
