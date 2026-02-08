pub mod afterglow;
pub mod ejecta_properties;
pub mod gcn_circular;
pub mod grb_localization;
pub mod grb_simulation;
pub mod rotation;
pub mod runner;
pub mod satellite;
pub mod voevent;

pub use afterglow::{
    simulate_afterglow, AfterglowConfig, AfterglowProperties, JetStructure,
};
pub use ejecta_properties::{
    compute_ejecta_properties, BinaryParams, BinaryType, EjectaProperties,
};
pub use gcn_circular::GcnCircular;
pub use grb_localization::{
    add_localization_error, ErrorEllipse, GrbInstrument, GrbLocalization,
};
pub use grb_simulation::{
    compute_simulation_stats, simulate_grb_batch, simulate_grb_counterpart,
    simulate_multimessenger_batch, simulate_multimessenger_event, GrbProperties,
    GrbSimulationConfig, GrbSimulationStats, GwEventParams, MultiMessengerEvent, SimulatedGrb,
};
pub use rotation::rotate_skymap;
pub use runner::{SimulationConfig, SimulationResult, SimulationRunner};
pub use satellite::{is_grb_detectable, SatelliteConfig, SkyPosition};
pub use voevent::{GrbAlert, VOEventParser};
