pub mod config;
pub mod correlator;
pub mod spatial;
pub mod superevent;
pub mod temporal;

pub use config::CorrelatorConfig;
pub use correlator::{CorrelatorError, CorrelatorStats, SupereventCorrelator};
pub use superevent::{
    GWComponent, GammaRayCandidate, MultiMessengerSuperevent, NeutrinoCandidate, OpticalCandidate,
    SupereventClassification, XRayCandidate,
};
pub use temporal::TemporalIndex;
