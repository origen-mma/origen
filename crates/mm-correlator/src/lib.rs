pub mod config;
pub mod correlator;
pub mod daily_report;
pub mod spatial;
pub mod superevent;
pub mod temporal;

pub use config::CorrelatorConfig;
pub use correlator::{CorrelatorError, CorrelatorStats, SupereventCorrelator};
pub use daily_report::{AlertSource, CrossMatchResult, DailyReport};
pub use superevent::{
    GWComponent, GammaRayCandidate, MultiMessengerSuperevent, NeutrinoCandidate, OpticalCandidate,
    SupereventClassification, XRayCandidate,
};
pub use temporal::TemporalIndex;
