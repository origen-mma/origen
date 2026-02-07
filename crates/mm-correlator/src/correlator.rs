use crate::{
    config::CorrelatorConfig,
    spatial::{calculate_joint_far, calculate_spatial_probability, positions_match},
    superevent::{MultiMessengerSuperevent, OpticalCandidate, SupereventClassification, GammaRayCandidate},
    temporal::TemporalIndex,
};
use mm_core::{Event, GWEvent, GammaRayEvent, LightCurve, Photometry, SkyPosition};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CorrelatorError {
    #[error("No GW event found for superevent")]
    NoGWEvent,

    #[error("Invalid time window")]
    InvalidTimeWindow,
}

/// Multi-messenger superevent correlator
/// Maintains state and matches events across different channels
pub struct SupereventCorrelator {
    /// Configuration
    config: CorrelatorConfig,

    /// Temporal index for fast time-based lookups
    temporal_index: TemporalIndex,

    /// Active superevents (id → superevent)
    superevents: HashMap<String, MultiMessengerSuperevent>,

    /// Counter for generating superevent IDs
    next_id: u64,
}

impl SupereventCorrelator {
    /// Create a new correlator
    pub fn new(config: CorrelatorConfig) -> Self {
        Self {
            config,
            temporal_index: TemporalIndex::new(),
            superevents: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create with default RAVEN configuration
    pub fn new_raven() -> Self {
        Self::new(CorrelatorConfig::raven())
    }

    /// Process a GCN event (GW, GRB, X-ray, neutrino)
    pub fn process_gcn_event(&mut self, event: Event) -> Result<Vec<String>, CorrelatorError> {
        match event {
            Event::GravitationalWave(gw) => self.process_gw_event(gw),
            Event::GammaRay(grb) => self.process_grb_event(grb),
            Event::XRay(_) => Ok(Vec::new()),      // TODO: Phase 4
            Event::Neutrino(_) => Ok(Vec::new()),  // TODO: Phase 4
            Event::Circular { .. } => Ok(Vec::new()),
        }
    }

    /// Process a gravitational wave event
    fn process_gw_event(&mut self, gw: GWEvent) -> Result<Vec<String>, CorrelatorError> {
        let gps_time = gw.gps_time.seconds;

        // Create new superevent
        let superevent_id = format!("MS{:06}", self.next_id);
        self.next_id += 1;

        let superevent = MultiMessengerSuperevent::new_from_gw(
            gw.superevent_id.clone(),
            gps_time,
            gw.position.clone(),  // Pass GW position for spatial correlation
        );

        // Add to indices
        self.temporal_index
            .insert(gps_time, superevent_id.clone());
        self.superevents
            .insert(superevent_id.clone(), superevent);

        Ok(vec![superevent_id])
    }

    /// Process a gamma-ray burst event
    fn process_grb_event(&mut self, grb: GammaRayEvent) -> Result<Vec<String>, CorrelatorError> {
        let trigger_time = grb.trigger_time;
        let mut affected_superevents = Vec::new();

        // Search for nearby GW superevents (within ±60 seconds)
        let candidates = self.temporal_index.find_in_window(
            trigger_time,
            -60.0, // 60 seconds before
            60.0,  // 60 seconds after
        );

        if candidates.is_empty() {
            // No GW event found, create standalone GRB superevent
            let superevent_id = format!("MMGRB{}", grb.trigger_id);
            let superevent = MultiMessengerSuperevent::new_from_grb(
                grb.trigger_id.clone(),
                trigger_time,
            );

            self.temporal_index
                .insert(trigger_time, superevent_id.clone());
            self.superevents
                .insert(superevent_id.clone(), superevent);

            affected_superevents.push(superevent_id);
        } else {
            // Associate GRB with existing GW superevent(s)
            for (gw_time, superevent_id) in candidates {
                if let Some(superevent) = self.superevents.get_mut(&superevent_id) {
                    let time_offset = trigger_time - gw_time;

                    // Calculate spatial offset if both have positions
                    // TODO: Extract position from GW skymap when available
                    let spatial_offset = None;

                    let candidate = GammaRayCandidate {
                        trigger_id: grb.trigger_id.clone(),
                        trigger_time,
                        position: grb.position.clone(),
                        time_offset,
                        spatial_offset,
                    };

                    superevent.add_gamma_ray_candidate(candidate);
                    affected_superevents.push(superevent_id.clone());
                }
            }
        }

        affected_superevents.sort();
        affected_superevents.dedup();
        Ok(affected_superevents)
    }

    /// Process optical light curve and match to GW events
    pub fn process_optical_lightcurve(
        &mut self,
        lightcurve: &LightCurve,
        position: &SkyPosition,
    ) -> Result<Vec<String>, CorrelatorError> {
        let mut matched_superevents = Vec::new();

        // Check each detection in the light curve
        for measurement in &lightcurve.measurements {
            let gps_time = measurement.to_gps_time();

            // Find GW superevents that could match this optical detection
            // Search backwards in time (GW before optical)
            let candidates = self.temporal_index.find_in_window(
                gps_time,
                -self.config.time_window_after, // Look back
                -self.config.time_window_before, // Look forward (small window)
            );

            for (gw_time, superevent_id) in candidates {
                if let Some(superevent) = self.superevents.get_mut(&superevent_id) {
                    // Extract GW position from superevent if available
                    let gw_position = superevent
                        .gw_event
                        .as_ref()
                        .and_then(|gw| gw.position.as_ref());

                    // Calculate significance
                    let time_offset = gps_time - gw_time;
                    let spatial_prob = calculate_spatial_probability(
                        position,
                        gw_position,
                        self.config.spatial_threshold,
                    );

                    let joint_far = calculate_joint_far(
                        time_offset,
                        self.config.time_window_after,
                        spatial_prob,
                        self.config.background_rate,
                        self.config.trials_factor,
                    );

                    // Check if significant
                    if joint_far < self.config.far_threshold {
                        let spatial_offset = if let Some(gw_pos) = gw_position.as_ref() {
                            position.angular_separation(gw_pos)
                        } else {
                            0.0 // No skymap, assume match
                        };

                        let candidate = OpticalCandidate {
                            object_id: lightcurve.object_id.clone(),
                            detection_time: gps_time,
                            position: position.clone(),
                            time_offset,
                            spatial_offset,
                            significance: measurement.snr(),
                            joint_far: Some(joint_far),
                        };

                        superevent.add_optical_candidate(candidate);
                        matched_superevents.push(superevent_id.clone());
                    }
                }
            }
        }

        matched_superevents.sort();
        matched_superevents.dedup();
        Ok(matched_superevents)
    }

    /// Get a superevent by ID
    pub fn get_superevent(&self, id: &str) -> Option<&MultiMessengerSuperevent> {
        self.superevents.get(id)
    }

    /// Get all active superevents
    pub fn get_all_superevents(&self) -> Vec<&MultiMessengerSuperevent> {
        self.superevents.values().collect()
    }

    /// Get superevents with optical counterparts
    pub fn get_mm_superevents(&self) -> Vec<&MultiMessengerSuperevent> {
        self.superevents
            .values()
            .filter(|s| s.classification != SupereventClassification::GWOnly)
            .collect()
    }

    /// Cleanup old superevents
    pub fn cleanup_old(&mut self) {
        let now = chrono::Utc::now().timestamp() as f64;
        let cutoff = now - self.config.max_superevent_age;

        let removed = self.temporal_index.cleanup_old(cutoff);
        for id in removed {
            self.superevents.remove(&id);
        }
    }

    /// Get statistics
    pub fn stats(&self) -> CorrelatorStats {
        let total = self.superevents.len();
        let gw_only = self
            .superevents
            .values()
            .filter(|s| s.classification == SupereventClassification::GWOnly)
            .count();
        let with_optical = self
            .superevents
            .values()
            .filter(|s| matches!(
                s.classification,
                SupereventClassification::GWWithOptical | SupereventClassification::MultiMessenger
            ))
            .count();

        CorrelatorStats {
            total_superevents: total,
            gw_only,
            with_optical,
            with_gamma_ray: 0,
            with_xray: 0,
            with_neutrino: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CorrelatorStats {
    pub total_superevents: usize,
    pub gw_only: usize,
    pub with_optical: usize,
    pub with_gamma_ray: usize,
    pub with_xray: usize,
    pub with_neutrino: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_core::GpsTime;

    #[test]
    fn test_correlator_gw_event() {
        let mut correlator = SupereventCorrelator::new(CorrelatorConfig::test());

        let gw = GWEvent {
            superevent_id: "S240101a".to_string(),
            alert_type: "PRELIMINARY".to_string(),
            gps_time: GpsTime::from_seconds(1234567890.0),
            instruments: vec!["H1".to_string(), "L1".to_string()],
            far: 1e-10,
            position: None,
        };

        let result = correlator.process_gw_event(gw).unwrap();
        assert_eq!(result.len(), 1);

        let stats = correlator.stats();
        assert_eq!(stats.total_superevents, 1);
        assert_eq!(stats.gw_only, 1);
        assert_eq!(stats.with_optical, 0);
    }

    #[test]
    fn test_correlator_optical_match() {
        let mut config = CorrelatorConfig::test();
        config.far_threshold = 10.0; // Very permissive for testing
        let mut correlator = SupereventCorrelator::new(config);

        let gw_gps = 1234567890.0;

        // Add GW event
        let gw = GWEvent {
            superevent_id: "S240101a".to_string(),
            alert_type: "PRELIMINARY".to_string(),
            gps_time: GpsTime::from_seconds(gw_gps),
            instruments: vec!["H1".to_string(), "L1".to_string()],
            far: 1e-10,
            position: Some(SkyPosition::new(123.0, 45.0, 5.0)),
        };
        correlator.process_gw_event(gw).unwrap();

        // Add optical detection 1 hour later (in GPS time)
        let optical_gps = gw_gps + 3600.0;

        // Convert GPS to MJD for the photometry
        // MJD = (GPS + 315964800 - 18) / 86400 + 40587
        let mjd = (optical_gps + 315964800.0 - 18.0) / 86400.0 + 40587.0;

        let mut lc = LightCurve::new("ZTF24test".to_string());
        lc.add_measurement(Photometry::new(
            mjd,
            1000.0,
            10.0,
            "r".to_string(),
        ));

        // Nearby position (within threshold)
        let position = SkyPosition::new(123.5, 45.0, 0.1);
        let matches = correlator.process_optical_lightcurve(&lc, &position).unwrap();

        // Should match due to proximity in time and space
        if matches.is_empty() {
            eprintln!("No matches found!");
            eprintln!("GW GPS: {}", gw_gps);
            eprintln!("Optical GPS: {}", optical_gps);
            eprintln!("Optical MJD: {}", mjd);
            eprintln!("Converted back: {}", lc.measurements[0].to_gps_time());
        }
        assert!(!matches.is_empty(), "Expected at least one match");
    }
}
