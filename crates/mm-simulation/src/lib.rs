pub mod afterglow;
pub mod background_grbs;
pub mod background_optical;
pub mod ejecta_properties;
pub mod gcn_circular;
pub mod grb_localization;
pub mod grb_simulation;
pub mod joint_far;
pub mod rotation;
pub mod runner;
pub mod satellite;
pub mod voevent;

pub use afterglow::{simulate_afterglow, AfterglowConfig, AfterglowProperties, JetStructure};
pub use background_grbs::{
    calculate_chance_coincidences, expected_chance_coincidences, generate_background_grbs,
    BackgroundGrb, BackgroundGrbConfig, ChanceCoincidenceStats, GrbSatellite,
};
pub use background_optical::{
    calculate_optical_coincidences, generate_background_optical, BackgroundOpticalConfig,
    BackgroundOpticalTransient, OpticalCoincidenceStats, OpticalSurvey, OpticalTransientType,
};
pub use ejecta_properties::{
    compute_ejecta_properties, BinaryParams, BinaryType, EjectaProperties,
};
pub use gcn_circular::GcnCircular;
pub use grb_localization::{add_localization_error, ErrorEllipse, GrbInstrument, GrbLocalization};
pub use grb_simulation::{
    compute_simulation_stats, simulate_grb_batch, simulate_grb_counterpart,
    simulate_multimessenger_batch, simulate_multimessenger_event, GrbProperties,
    GrbSimulationConfig, GrbSimulationStats, GwEventParams, MultiMessengerEvent, SimulatedGrb,
};
pub use joint_far::{
    calculate_joint_far, calculate_pastro, FarComponents, JointFarConfig, JointFarResult,
    MultiMessengerAssociation as FarAssociation,
};
pub use rotation::rotate_skymap;
pub use runner::{SimulationConfig, SimulationResult, SimulationRunner};
pub use satellite::{is_grb_detectable, SatelliteConfig, SkyPosition};
pub use voevent::{GrbAlert, VOEventParser};
