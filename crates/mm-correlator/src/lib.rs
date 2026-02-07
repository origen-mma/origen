pub mod config;
pub mod correlator;
pub mod spatial;
pub mod superevent;
pub mod temporal;

pub use config::CorrelatorConfig;
pub use correlator::{SupereventCorrelator, CorrelatorStats, CorrelatorError};
pub use superevent::{
    MultiMessengerSuperevent, OpticalCandidate, SupereventClassification,
    GWComponent, GammaRayCandidate, XRayCandidate, NeutrinoCandidate,
};
pub use temporal::TemporalIndex;
