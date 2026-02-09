use crate::{GpsTime, SkyPosition};
use serde::{Deserialize, Serialize};

/// Multi-messenger event type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    GravitationalWave(GWEvent),
    GammaRay(GammaRayEvent),
    XRay(XRayEvent),
    Neutrino(NeutrinoEvent),
    Circular { text: String },
}

/// Event source type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    GravitationalWave,
    GammaRay,
    XRay,
    Neutrino,
    Circular,
}

impl Event {
    pub fn event_type(&self) -> EventType {
        match self {
            Event::GravitationalWave(_) => EventType::GravitationalWave,
            Event::GammaRay(_) => EventType::GammaRay,
            Event::XRay(_) => EventType::XRay,
            Event::Neutrino(_) => EventType::Neutrino,
            Event::Circular { .. } => EventType::Circular,
        }
    }

    pub fn timestamp(&self) -> Option<f64> {
        match self {
            Event::GravitationalWave(e) => Some(e.gps_time.seconds),
            Event::GammaRay(e) => Some(e.trigger_time),
            Event::XRay(e) => Some(e.trigger_time),
            Event::Neutrino(e) => Some(e.event_time),
            Event::Circular { .. } => None,
        }
    }

    pub fn sky_position(&self) -> Option<&SkyPosition> {
        match self {
            Event::GravitationalWave(e) => e.position.as_ref(),
            Event::GammaRay(e) => e.position.as_ref(),
            Event::XRay(e) => e.position.as_ref(),
            Event::Neutrino(e) => e.position.as_ref(),
            Event::Circular { .. } => None,
        }
    }
}

/// Gravitational wave event (from igwn.gwalert)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GWEvent {
    pub superevent_id: String,
    pub alert_type: String,
    pub gps_time: GpsTime,
    pub instruments: Vec<String>,
    pub far: f64,
    pub position: Option<SkyPosition>,
    pub skymap: Option<crate::ParsedSkymap>,
}

/// Gamma-ray burst event (from Swift BAT, Fermi)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GammaRayEvent {
    pub trigger_id: String,
    pub instrument: String,
    pub trigger_time: f64,
    pub position: Option<SkyPosition>,
    pub significance: f64,
    pub skymap_url: Option<String>, // URL to HEALPix FITS skymap
    pub error_radius: Option<f64>,  // Position error radius (degrees)
}

/// X-ray transient event (from Einstein Probe)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XRayEvent {
    pub event_id: String,
    pub trigger_time: f64,
    pub position: Option<SkyPosition>,
    pub flux: Option<f64>,
}

/// Neutrino event (from IceCube)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutrinoEvent {
    pub event_id: String,
    pub event_time: f64,
    pub position: Option<SkyPosition>,
    pub energy: Option<f64>,
    pub significance: f64,
}
