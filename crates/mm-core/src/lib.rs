pub mod error;
pub mod events;
pub mod explosion_time;
pub mod fit_quality;
pub mod gp_features;
pub mod io;
pub mod lightcurve;
pub mod lightcurve_fitting;
// pub mod multistart_fitter; // TODO: Incomplete, has unimplemented!() functions
pub mod optical;
pub mod position;
pub mod pso_fitter;
pub mod redis_compat;
pub mod skymap;
pub mod skymap_parser;
pub mod skymap_storage;
pub mod svi_fitter;
pub mod svi_models;
pub mod t0_profile;
pub mod time;

pub use error::{CoreError, ParseError};
pub use events::{Event, EventType, GWEvent, GammaRayEvent, NeutrinoEvent, XRayEvent};
pub use explosion_time::estimate_explosion_time;
pub use fit_quality::{FitQuality, FitQualityAssessment};
pub use gp_features::{
    background_rejection_score, extract_features, LightCurveFeatures, LightCurveFilterConfig,
};
pub use io::{load_lightcurve_csv, load_lightcurves_dir};
pub use lightcurve::{LightCurve, Photometry};
pub use lightcurve_fitting::{
    fit_lightcurve, fit_lightcurve_with_config, FitConfig, FitModel, LightCurveFitResult,
};
// pub use multistart_fitter::{multistart_fit, MultiStartConfig}; // TODO: Module incomplete
pub use optical::{flux_to_magnitude, Classification, OpticalAlert, PhotometryPoint, Survey};
pub use position::SkyPosition;
pub use skymap::MockSkymap;
pub use skymap_parser::{CredibleRegion, ParsedSkymap, SkymapOrdering, SkymapParseError};
pub use skymap_storage::{SkymapStorage, SkymapStorageError};
pub use t0_profile::fit_lightcurve_profile_t0;
pub use time::GpsTime;
