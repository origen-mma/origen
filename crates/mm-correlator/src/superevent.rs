use mm_core::SkyPosition;
use serde::{Deserialize, Serialize};

/// Multi-messenger superevent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMessengerSuperevent {
    /// Unique superevent identifier
    pub id: String,

    /// Central time (GPS seconds)
    pub t_0: f64,

    /// Time window
    pub t_start: f64,
    pub t_end: f64,

    /// Gravitational wave component
    pub gw_event: Option<GWComponent>,

    /// Optical counterpart candidates
    pub optical_candidates: Vec<OpticalCandidate>,

    /// Other messengers
    pub gamma_ray_candidates: Vec<GammaRayCandidate>,
    pub xray_candidates: Vec<XRayCandidate>,
    pub neutrino_candidates: Vec<NeutrinoCandidate>,

    /// Classification
    pub classification: SupereventClassification,

    /// Combined significance (joint FAR)
    pub joint_far: Option<f64>,

    /// Metadata
    pub created_at: f64,
    pub updated_at: f64,
}

impl MultiMessengerSuperevent {
    /// Create a new superevent from GW event
    pub fn new_from_gw(
        gw_superevent_id: String,
        gps_time: f64,
        position: Option<SkyPosition>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp() as f64;

        Self {
            id: format!("MM{}", gw_superevent_id),
            t_0: gps_time,
            t_start: gps_time - 1.0,   // -1 second
            t_end: gps_time + 86400.0, // +1 day
            gw_event: Some(GWComponent {
                superevent_id: gw_superevent_id,
                gps_time,
                instruments: Vec::new(),
                far: None,
                skymap_available: false,
                position,
            }),
            optical_candidates: Vec::new(),
            gamma_ray_candidates: Vec::new(),
            xray_candidates: Vec::new(),
            neutrino_candidates: Vec::new(),
            classification: SupereventClassification::GWOnly,
            joint_far: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add optical candidate
    pub fn add_optical_candidate(&mut self, candidate: OpticalCandidate) {
        self.optical_candidates.push(candidate);
        self.updated_at = chrono::Utc::now().timestamp() as f64;
        self.update_classification();
    }

    /// Add gamma-ray candidate
    pub fn add_gamma_ray_candidate(&mut self, candidate: GammaRayCandidate) {
        self.gamma_ray_candidates.push(candidate);
        self.updated_at = chrono::Utc::now().timestamp() as f64;
        self.update_classification();
    }

    /// Create a new superevent from gamma-ray event
    pub fn new_from_grb(trigger_id: String, trigger_time: f64) -> Self {
        let now = chrono::Utc::now().timestamp() as f64;

        Self {
            id: format!("MMGRB{}", trigger_id),
            t_0: trigger_time,
            t_start: trigger_time - 60.0,  // -60 seconds
            t_end: trigger_time + 86400.0, // +1 day
            gw_event: None,
            optical_candidates: Vec::new(),
            gamma_ray_candidates: Vec::new(),
            xray_candidates: Vec::new(),
            neutrino_candidates: Vec::new(),
            classification: SupereventClassification::GWWithGammaRay,
            joint_far: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update classification based on components
    fn update_classification(&mut self) {
        let has_optical = !self.optical_candidates.is_empty();
        let has_grb = !self.gamma_ray_candidates.is_empty();
        let has_xray = !self.xray_candidates.is_empty();
        let has_neutrino = !self.neutrino_candidates.is_empty();

        self.classification = match (has_optical, has_grb, has_xray, has_neutrino) {
            (false, false, false, false) => SupereventClassification::GWOnly,
            (true, false, false, false) => SupereventClassification::GWWithOptical,
            (false, true, false, false) => SupereventClassification::GWWithGammaRay,
            (false, false, true, false) => SupereventClassification::GWWithXRay,
            (false, false, false, true) => SupereventClassification::GWWithNeutrino,
            _ => SupereventClassification::MultiMessenger,
        };
    }

    /// Check if within time window
    pub fn is_within_time_window(&self, time: f64) -> bool {
        time >= self.t_start && time <= self.t_end
    }
}

/// GW component of superevent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GWComponent {
    pub superevent_id: String,
    pub gps_time: f64,
    pub instruments: Vec<String>,
    pub far: Option<f64>,
    pub skymap_available: bool,
    pub position: Option<SkyPosition>, // Sky position for correlation
}

/// Optical counterpart candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalCandidate {
    pub object_id: String,
    pub detection_time: f64, // GPS seconds
    pub position: SkyPosition,
    pub time_offset: f64,    // Seconds from GW t_0
    pub spatial_offset: f64, // Degrees from GW skymap
    pub significance: f64,   // SNR or similar
    pub joint_far: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GammaRayCandidate {
    pub trigger_id: String,
    pub trigger_time: f64,
    pub position: Option<SkyPosition>,
    pub time_offset: f64,
    pub spatial_offset: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XRayCandidate {
    pub event_id: String,
    pub trigger_time: f64,
    pub position: Option<SkyPosition>,
    pub time_offset: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutrinoCandidate {
    pub event_id: String,
    pub event_time: f64,
    pub position: Option<SkyPosition>,
    pub time_offset: f64,
    pub energy: Option<f64>,
}

/// Superevent classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupereventClassification {
    GWOnly,
    GWWithOptical,
    GWWithGammaRay,
    GWWithXRay,
    GWWithNeutrino,
    MultiMessenger,
}
