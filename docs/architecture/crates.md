# Crate Reference

| Crate | Purpose | Key Types |
|---|---|---|
| **mm-core** | Algorithms, data types, models | `Event`, `LightCurve`, `SkyPosition`, `MockSkymap`, `GpsTime` |
| **mm-correlator** | RAVEN superevent correlation | `SupereventCorrelator`, `CorrelatorConfig`, `DailyReport` |
| **mm-simulation** | Synthetic event generation | `GwPopulationModel`, `SurveyModel`, `CampaignConfig`, `BackgroundOpticalConfig` |
| **mm-gcn** | GCN alert parsing | `AlertRouter`, GW/GRB/neutrino/X-ray parsers |
| **mm-boom** | BOOM/ZTF alert parsing | `BoomAlert`, Avro deserialization |
| **mm-config** | Configuration management | `Config`, `DailyComparisonConfig` |
| **mm-redis** | Redis state persistence | `RedisStateStore` |
| **mm-api** | REST API server | HTTP endpoints for events and correlations |
| **mm-service** | Binary entry points | ~30 CLI tools and services |

## mm-core

Core data types and algorithms shared across the workspace.

- **Events**: `GWEvent`, `GammaRayEvent`, `XRayEvent`, `NeutrinoEvent`
- **Photometry**: `Photometry` (MJD, flux in uJy, filter), `LightCurve`
- **Sky coordinates**: `SkyPosition` (RA, Dec, error), `MockSkymap` (2D Gaussian), `ParsedSkymap` (HEALPix)
- **Models**: MetzgerKN forward model (`metzger_kn_eval_batch`), GP features, SVI fitting
- **Time**: `GpsTime` with GPS/Unix/MJD conversions

## mm-correlator

The RAVEN-style correlation engine.

- `SupereventCorrelator::new(config)` -- create a correlator instance
- `process_gcn_event(event)` -- ingest a GCN event (creates superevents for GW triggers)
- `process_optical_lightcurve(lc, pos)` -- match an optical transient against active superevents
- `daily_report` module -- cross-matching, completeness, latency analysis for daily comparison

## mm-simulation

Synthetic event generation for testing and calibration.

- **GW population**: `draw_gw_event()` -- BNS mergers from astrophysical distributions
- **KN synthesis**: `generate_kilonova_lightcurve()` -- MetzgerKN + survey noise
- **Background**: `generate_background_optical()` -- SNe Ia + shock cooling at survey rates
- **Campaigns**: `run_injection_campaign()` -- full Monte Carlo FAR calibration
- **GRB simulation**: `simulate_grb_counterpart()`, `simulate_multimessenger_event()`

## mm-service

CLI binaries. Key services:

| Binary | Description |
|---|---|
| `mm-correlator-service` | Real-time correlator consuming from Kafka |
| `gcn-correlator` | Live GCN stream consumer + RAVEN correlator |
| `daily-comparison` | Daily GCN vs BOOM comparison reports |
| `far-tuning-campaign` | FAR calibration injection campaign |
| `stream-events` | Synthetic GW+GRB event streamer |
| `stream-optical-alerts` | ZTF light curve streamer |
| `stream-o4-simulation` | O4 observing scenario replay |
| `mm_api_server` | REST API for events and correlations |
