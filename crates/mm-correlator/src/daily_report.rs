use crate::{CorrelatorConfig, SupereventClassification, SupereventCorrelator};
use mm_core::{Event, EventType, LightCurve, SkyPosition};
use serde::{Deserialize, Serialize};

/// Which stream an event came from
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertSource {
    Gcn,
    Boom,
}

/// Normalized event record stored per-day for comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub event_id: String,
    pub source: AlertSource,
    pub event_type: EventType,
    pub gps_time: f64,
    pub position: Option<SkyPosition>,
    /// Wall-clock time when the service received this alert (Unix seconds)
    pub received_at: f64,
}

/// Result of cross-matching a single pair of events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossMatchResult {
    pub gcn_event_id: String,
    pub boom_event_id: String,
    pub angular_separation_deg: f64,
    pub time_difference_s: f64,
    /// Which source reported first (by received_at)
    pub first_reporter: AlertSource,
    /// How much earlier the faster source received the alert (seconds)
    pub latency_advantage_s: f64,
}

/// Completeness tracking for one source relative to the other
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletenessMetrics {
    pub total_events: usize,
    pub matched_events: usize,
    pub unmatched_events: usize,
    pub completeness_fraction: f64,
}

/// Per-type event counts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventTypeCounts {
    pub gravitational_wave: usize,
    pub gamma_ray: usize,
    pub x_ray: usize,
    pub neutrino: usize,
    pub optical: usize,
}

impl EventTypeCounts {
    pub fn increment(&mut self, event_type: EventType) {
        match event_type {
            EventType::GravitationalWave => self.gravitational_wave += 1,
            EventType::GammaRay => self.gamma_ray += 1,
            EventType::XRay => self.x_ray += 1,
            EventType::Neutrino => self.neutrino += 1,
            EventType::Circular => {} // not counted
        }
    }

    pub fn increment_optical(&mut self) {
        self.optical += 1;
    }
}

/// Summary of the RAVEN correlator batch run for one day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCorrelatorSummary {
    pub total_superevents: usize,
    pub gw_only: usize,
    pub with_grb: usize,
    pub with_optical: usize,
    pub multi_messenger: usize,
    pub best_joint_far: Option<f64>,
    pub significant_candidates: Vec<String>,
}

/// The complete daily comparison report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: String,
    pub generated_at: String,
    pub uptime_s: f64,

    // Inventory
    pub gcn_event_count: usize,
    pub boom_event_count: usize,
    pub gcn_by_type: EventTypeCounts,
    pub boom_by_type: EventTypeCounts,

    // Cross-matching
    pub cross_matches: Vec<CrossMatchResult>,
    pub total_cross_matches: usize,

    // Completeness
    pub gcn_completeness: CompletenessMetrics,
    pub boom_completeness: CompletenessMetrics,

    // Latency
    pub median_latency_advantage_gcn_s: Option<f64>,
    pub median_latency_advantage_boom_s: Option<f64>,
    pub gcn_first_count: usize,
    pub boom_first_count: usize,

    // RAVEN correlator
    pub correlator_summary: DailyCorrelatorSummary,

    // Errors
    pub gcn_parse_errors: usize,
    pub boom_parse_errors: usize,
    pub gcn_kafka_errors: usize,
    pub boom_kafka_errors: usize,
}

/// Cross-match GCN and BOOM events by spatial proximity and temporal overlap.
///
/// Uses greedy nearest-neighbor: sorts candidate pairs by angular separation,
/// assigns each BOOM event to its closest unmatched GCN event.
pub fn cross_match_events(
    gcn_inventory: &[InventoryEntry],
    boom_inventory: &[InventoryEntry],
    spatial_threshold_deg: f64,
    temporal_threshold_s: f64,
) -> Vec<CrossMatchResult> {
    // Collect all candidate pairs (gcn_idx, boom_idx, separation, time_diff)
    let mut candidates: Vec<(usize, usize, f64, f64)> = Vec::new();

    for (gi, gcn) in gcn_inventory.iter().enumerate() {
        let gcn_pos = match &gcn.position {
            Some(p) => p,
            None => continue,
        };

        for (bi, boom) in boom_inventory.iter().enumerate() {
            let boom_pos = match &boom.position {
                Some(p) => p,
                None => continue,
            };

            let time_diff = boom.gps_time - gcn.gps_time;
            if time_diff.abs() > temporal_threshold_s {
                continue;
            }

            let sep = gcn_pos.angular_separation(boom_pos);
            if sep <= spatial_threshold_deg {
                candidates.push((gi, bi, sep, time_diff));
            }
        }
    }

    // Sort by angular separation (greedy nearest-neighbor)
    candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut matched_gcn = vec![false; gcn_inventory.len()];
    let mut matched_boom = vec![false; boom_inventory.len()];
    let mut results = Vec::new();

    for (gi, bi, sep, time_diff) in candidates {
        if matched_gcn[gi] || matched_boom[bi] {
            continue;
        }
        matched_gcn[gi] = true;
        matched_boom[bi] = true;

        let gcn_entry = &gcn_inventory[gi];
        let boom_entry = &boom_inventory[bi];

        let (first_reporter, latency_advantage_s) =
            if gcn_entry.received_at <= boom_entry.received_at {
                (
                    AlertSource::Gcn,
                    boom_entry.received_at - gcn_entry.received_at,
                )
            } else {
                (
                    AlertSource::Boom,
                    gcn_entry.received_at - boom_entry.received_at,
                )
            };

        results.push(CrossMatchResult {
            gcn_event_id: gcn_entry.event_id.clone(),
            boom_event_id: boom_entry.event_id.clone(),
            angular_separation_deg: sep,
            time_difference_s: time_diff,
            first_reporter,
            latency_advantage_s,
        });
    }

    results
}

/// Compute completeness for one source given cross-match results.
pub fn compute_completeness(
    inventory: &[InventoryEntry],
    cross_matches: &[CrossMatchResult],
    source: AlertSource,
) -> CompletenessMetrics {
    let total = inventory.len();
    if total == 0 {
        return CompletenessMetrics {
            total_events: 0,
            matched_events: 0,
            unmatched_events: 0,
            completeness_fraction: 0.0,
        };
    }

    let matched = match source {
        AlertSource::Gcn => {
            let matched_ids: std::collections::HashSet<&str> = cross_matches
                .iter()
                .map(|m| m.gcn_event_id.as_str())
                .collect();
            inventory
                .iter()
                .filter(|e| matched_ids.contains(e.event_id.as_str()))
                .count()
        }
        AlertSource::Boom => {
            let matched_ids: std::collections::HashSet<&str> = cross_matches
                .iter()
                .map(|m| m.boom_event_id.as_str())
                .collect();
            inventory
                .iter()
                .filter(|e| matched_ids.contains(e.event_id.as_str()))
                .count()
        }
    };

    CompletenessMetrics {
        total_events: total,
        matched_events: matched,
        unmatched_events: total - matched,
        completeness_fraction: matched as f64 / total as f64,
    }
}

/// Compute latency statistics from cross-match results.
///
/// Returns (median_gcn_advantage, median_boom_advantage, gcn_first_count, boom_first_count).
pub fn compute_latency_stats(
    cross_matches: &[CrossMatchResult],
) -> (Option<f64>, Option<f64>, usize, usize) {
    if cross_matches.is_empty() {
        return (None, None, 0, 0);
    }

    let mut gcn_advantages: Vec<f64> = Vec::new();
    let mut boom_advantages: Vec<f64> = Vec::new();

    for m in cross_matches {
        match m.first_reporter {
            AlertSource::Gcn => gcn_advantages.push(m.latency_advantage_s),
            AlertSource::Boom => boom_advantages.push(m.latency_advantage_s),
        }
    }

    let gcn_first_count = gcn_advantages.len();
    let boom_first_count = boom_advantages.len();

    let median_gcn = median(&mut gcn_advantages);
    let median_boom = median(&mut boom_advantages);

    (median_gcn, median_boom, gcn_first_count, boom_first_count)
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

/// Run the RAVEN correlator in batch mode over one day's events.
///
/// Replays GCN events and BOOM light curves through a fresh `SupereventCorrelator`,
/// then extracts summary statistics.
pub fn run_daily_correlation(
    gcn_events: &[Event],
    boom_entries: &[(LightCurve, SkyPosition, String)],
    config: &CorrelatorConfig,
) -> DailyCorrelatorSummary {
    let mut correlator = SupereventCorrelator::new(config.clone());

    // Process GCN events (GW triggers must come first to create superevents)
    for event in gcn_events {
        if let Err(e) = correlator.process_gcn_event(event.clone()) {
            tracing::warn!("Daily correlation: GCN event error: {}", e);
        }
    }

    // Process BOOM optical light curves
    for (lc, pos, _object_id) in boom_entries {
        if let Err(e) = correlator.process_optical_lightcurve(lc, pos) {
            tracing::debug!("Daily correlation: optical LC error: {}", e);
        }
    }

    let stats = correlator.stats();
    let superevents = correlator.get_all_superevents();

    let with_grb = superevents
        .iter()
        .filter(|s| {
            matches!(
                s.classification,
                SupereventClassification::GWWithGammaRay | SupereventClassification::MultiMessenger
            )
        })
        .count();

    let multi_messenger = superevents
        .iter()
        .filter(|s| s.classification == SupereventClassification::MultiMessenger)
        .count();

    // Find best (lowest) joint FAR and significant candidates
    let mut best_far: Option<f64> = None;
    let mut significant = Vec::new();
    let far_threshold = config.far_threshold;

    for se in &superevents {
        if let Some(far) = se.joint_far {
            if best_far.is_none() || far < best_far.unwrap() {
                best_far = Some(far);
            }
            if far < far_threshold {
                significant.push(se.id.clone());
            }
        }
    }

    DailyCorrelatorSummary {
        total_superevents: stats.total_superevents,
        gw_only: stats.gw_only,
        with_grb,
        with_optical: stats.with_optical,
        multi_messenger,
        best_joint_far: best_far,
        significant_candidates: significant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        id: &str,
        source: AlertSource,
        ra: f64,
        dec: f64,
        gps_time: f64,
        received_at: f64,
    ) -> InventoryEntry {
        InventoryEntry {
            event_id: id.to_string(),
            source,
            event_type: if source == AlertSource::Gcn {
                EventType::GammaRay
            } else {
                EventType::Circular // no Optical variant; Circular is inert
            },
            gps_time,
            position: Some(SkyPosition::new(ra, dec, 1.0)),
            received_at,
        }
    }

    fn make_entry_no_pos(
        id: &str,
        source: AlertSource,
        gps_time: f64,
        received_at: f64,
    ) -> InventoryEntry {
        InventoryEntry {
            event_id: id.to_string(),
            source,
            event_type: EventType::GammaRay,
            gps_time,
            position: None,
            received_at,
        }
    }

    #[test]
    fn test_cross_match_exact_position() {
        let gcn = vec![make_entry(
            "G1",
            AlertSource::Gcn,
            180.0,
            45.0,
            1000.0,
            100.0,
        )];
        let boom = vec![make_entry(
            "B1",
            AlertSource::Boom,
            180.0,
            45.0,
            1000.0,
            105.0,
        )];

        let matches = cross_match_events(&gcn, &boom, 5.0, 86400.0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].gcn_event_id, "G1");
        assert_eq!(matches[0].boom_event_id, "B1");
        assert!(matches[0].angular_separation_deg < 0.001);
        assert_eq!(matches[0].first_reporter, AlertSource::Gcn);
        assert!((matches[0].latency_advantage_s - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_cross_match_no_match_spatial() {
        let gcn = vec![make_entry("G1", AlertSource::Gcn, 0.0, 0.0, 1000.0, 100.0)];
        let boom = vec![make_entry(
            "B1",
            AlertSource::Boom,
            10.0,
            10.0,
            1000.0,
            105.0,
        )];

        let matches = cross_match_events(&gcn, &boom, 5.0, 86400.0);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_cross_match_no_match_temporal() {
        let gcn = vec![make_entry(
            "G1",
            AlertSource::Gcn,
            180.0,
            45.0,
            1000.0,
            100.0,
        )];
        // More than 86400s apart
        let boom = vec![make_entry(
            "B1",
            AlertSource::Boom,
            180.0,
            45.0,
            100000.0,
            200.0,
        )];

        let matches = cross_match_events(&gcn, &boom, 5.0, 86400.0);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_cross_match_greedy_assignment() {
        // Two GCN events, one BOOM event close to both but closer to G2
        let gcn = vec![
            make_entry("G1", AlertSource::Gcn, 180.0, 45.0, 1000.0, 100.0),
            make_entry("G2", AlertSource::Gcn, 180.5, 45.0, 1000.0, 100.0),
        ];
        let boom = vec![make_entry(
            "B1",
            AlertSource::Boom,
            180.4,
            45.0,
            1000.0,
            105.0,
        )];

        let matches = cross_match_events(&gcn, &boom, 5.0, 86400.0);
        assert_eq!(matches.len(), 1);
        // B1 is closer to G2 (0.1 deg) than G1 (0.4 deg), greedy picks closest first
        assert_eq!(matches[0].gcn_event_id, "G2");
        assert_eq!(matches[0].boom_event_id, "B1");
    }

    #[test]
    fn test_cross_match_skips_no_position() {
        let gcn = vec![make_entry_no_pos("G1", AlertSource::Gcn, 1000.0, 100.0)];
        let boom = vec![make_entry(
            "B1",
            AlertSource::Boom,
            180.0,
            45.0,
            1000.0,
            105.0,
        )];

        let matches = cross_match_events(&gcn, &boom, 5.0, 86400.0);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_completeness_all_matched() {
        let inv = vec![
            make_entry("G1", AlertSource::Gcn, 0.0, 0.0, 1000.0, 100.0),
            make_entry("G2", AlertSource::Gcn, 1.0, 1.0, 2000.0, 200.0),
        ];
        let matches = vec![
            CrossMatchResult {
                gcn_event_id: "G1".into(),
                boom_event_id: "B1".into(),
                angular_separation_deg: 0.1,
                time_difference_s: 10.0,
                first_reporter: AlertSource::Gcn,
                latency_advantage_s: 5.0,
            },
            CrossMatchResult {
                gcn_event_id: "G2".into(),
                boom_event_id: "B2".into(),
                angular_separation_deg: 0.2,
                time_difference_s: 20.0,
                first_reporter: AlertSource::Boom,
                latency_advantage_s: 3.0,
            },
        ];

        let c = compute_completeness(&inv, &matches, AlertSource::Gcn);
        assert_eq!(c.total_events, 2);
        assert_eq!(c.matched_events, 2);
        assert_eq!(c.unmatched_events, 0);
        assert!((c.completeness_fraction - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_completeness_none_matched() {
        let inv = vec![
            make_entry("G1", AlertSource::Gcn, 0.0, 0.0, 1000.0, 100.0),
            make_entry("G2", AlertSource::Gcn, 1.0, 1.0, 2000.0, 200.0),
        ];
        let matches: Vec<CrossMatchResult> = vec![];

        let c = compute_completeness(&inv, &matches, AlertSource::Gcn);
        assert_eq!(c.total_events, 2);
        assert_eq!(c.matched_events, 0);
        assert_eq!(c.unmatched_events, 2);
        assert!((c.completeness_fraction).abs() < 1e-10);
    }

    #[test]
    fn test_completeness_empty_inventory() {
        let c = compute_completeness(&[], &[], AlertSource::Gcn);
        assert_eq!(c.total_events, 0);
        assert_eq!(c.completeness_fraction, 0.0);
    }

    #[test]
    fn test_latency_stats() {
        let matches = vec![
            CrossMatchResult {
                gcn_event_id: "G1".into(),
                boom_event_id: "B1".into(),
                angular_separation_deg: 0.1,
                time_difference_s: 10.0,
                first_reporter: AlertSource::Gcn,
                latency_advantage_s: 120.0,
            },
            CrossMatchResult {
                gcn_event_id: "G2".into(),
                boom_event_id: "B2".into(),
                angular_separation_deg: 0.2,
                time_difference_s: 20.0,
                first_reporter: AlertSource::Boom,
                latency_advantage_s: 30.0,
            },
            CrossMatchResult {
                gcn_event_id: "G3".into(),
                boom_event_id: "B3".into(),
                angular_separation_deg: 0.3,
                time_difference_s: 30.0,
                first_reporter: AlertSource::Gcn,
                latency_advantage_s: 60.0,
            },
        ];

        let (med_gcn, med_boom, gcn_first, boom_first) = compute_latency_stats(&matches);
        assert_eq!(gcn_first, 2);
        assert_eq!(boom_first, 1);
        // GCN advantages: [120, 60] -> median = 90
        assert!((med_gcn.unwrap() - 90.0).abs() < 0.001);
        // BOOM advantages: [30] -> median = 30
        assert!((med_boom.unwrap() - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_latency_stats_empty() {
        let (med_gcn, med_boom, gcn_first, boom_first) = compute_latency_stats(&[]);
        assert!(med_gcn.is_none());
        assert!(med_boom.is_none());
        assert_eq!(gcn_first, 0);
        assert_eq!(boom_first, 0);
    }

    #[test]
    fn test_daily_report_serialization() {
        let report = DailyReport {
            date: "2026-02-28".to_string(),
            generated_at: "2026-03-01T00:00:05Z".to_string(),
            uptime_s: 86395.0,
            gcn_event_count: 5,
            boom_event_count: 100,
            gcn_by_type: EventTypeCounts {
                gravitational_wave: 2,
                gamma_ray: 3,
                ..Default::default()
            },
            boom_by_type: EventTypeCounts {
                optical: 100,
                ..Default::default()
            },
            cross_matches: vec![],
            total_cross_matches: 0,
            gcn_completeness: CompletenessMetrics {
                total_events: 5,
                matched_events: 0,
                unmatched_events: 5,
                completeness_fraction: 0.0,
            },
            boom_completeness: CompletenessMetrics {
                total_events: 100,
                matched_events: 0,
                unmatched_events: 100,
                completeness_fraction: 0.0,
            },
            median_latency_advantage_gcn_s: None,
            median_latency_advantage_boom_s: None,
            gcn_first_count: 0,
            boom_first_count: 0,
            correlator_summary: DailyCorrelatorSummary {
                total_superevents: 2,
                gw_only: 2,
                with_grb: 0,
                with_optical: 0,
                multi_messenger: 0,
                best_joint_far: None,
                significant_candidates: vec![],
            },
            gcn_parse_errors: 0,
            boom_parse_errors: 0,
            gcn_kafka_errors: 0,
            boom_kafka_errors: 0,
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let roundtrip: DailyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.date, "2026-02-28");
        assert_eq!(roundtrip.gcn_event_count, 5);
        assert_eq!(roundtrip.boom_event_count, 100);
        assert_eq!(roundtrip.correlator_summary.total_superevents, 2);
    }

    #[test]
    fn test_event_type_counts() {
        let mut counts = EventTypeCounts::default();
        counts.increment(EventType::GravitationalWave);
        counts.increment(EventType::GravitationalWave);
        counts.increment(EventType::GammaRay);
        counts.increment(EventType::Neutrino);
        counts.increment_optical();
        counts.increment_optical();

        assert_eq!(counts.gravitational_wave, 2);
        assert_eq!(counts.gamma_ray, 1);
        assert_eq!(counts.neutrino, 1);
        assert_eq!(counts.optical, 2);
        assert_eq!(counts.x_ray, 0);
    }
}
