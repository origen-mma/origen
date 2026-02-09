# ORIGIN: Multi-Messenger Superevent Simulation Framework

[![CI](https://github.com/mcoughlin/origin/workflows/CI/badge.svg)](https://github.com/mcoughlin/origin/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

ORIGIN is a comprehensive simulation framework for testing multi-messenger superevent creation and correlation. It generates synthetic gravitational wave (GW), gamma-ray burst (GRB), and optical transient events, streams them through a Kafka-based pipeline, and correlates them in real-time using the RAVEN algorithm to form multi-messenger superevents.

The framework is designed to validate and stress-test the end-to-end pipeline that will process real alerts from LIGO/Virgo/KAGRA, Fermi/Swift, and optical surveys like ZTF and LSST during observing runs.

## Table of Contents

- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Crates](#crates)
- [Capabilities](#capabilities)
- [Running the Demo](#running-the-demo)
- [Configuration](#configuration)
- [Testing](#testing)
- [Future Development](#future-development)
- [References](#references)

## Quick Start

### Prerequisites

- **Rust** (1.75+): Install from [rustup.rs](https://rustup.rs)
- **Docker & Docker Compose**: For Kafka, Redis, Prometheus, and Grafana

### Build and Run

```bash
git clone https://github.com/mcoughlin/origin.git
cd origin
cargo build --release

# Start infrastructure (Kafka, Redis, Prometheus, Grafana)
docker compose up -d
```

See [Running the Demo](#running-the-demo) for the full multi-terminal walkthrough.

## Architecture

```
                        Simulation Layer
 ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
 │  GW + GRB Event  │  │  O4 Observing    │  │  Optical Alert   │
 │  Generator       │  │  Scenario Sim    │  │  Streamer (ZTF)  │
 │  (stream-events) │  │  (stream-o4-sim) │  │  (stream-optical)│
 └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
          │                     │                      │
          ▼                     ▼                      ▼
 ┌────────────────────────────────────────────────────────────────┐
 │                     Kafka Message Bus                          │
 │  igwn.gwalert  │  gcn.notices.grb  │  optical.alerts          │
 └────────────────────────────┬───────────────────────────────────┘
                              │
                              ▼
 ┌────────────────────────────────────────────────────────────────┐
 │               Superevent Correlator (mm-correlator)            │
 │                                                                │
 │  Temporal matching ──► Spatial matching ──► Joint FAR (RAVEN)  │
 │         │                                        │             │
 │         ▼                                        ▼             │
 │  SVI light curve fitting              GP feature extraction    │
 │  (t0 estimation)                      (background rejection)   │
 └──────────────────────────┬─────────────────────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
 ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
 │    Redis     │  │  Prometheus  │  │  REST API    │
 │  (state)     │  │  + Grafana   │  │  (mm-api)    │
 └──────────────┘  └──────────────┘  └──────────────┘
```

## Crates

| Crate | Purpose |
|-------|---------|
| **mm-core** | Core data structures, light curve models, GP feature extraction, skymap handling, SVI fitting |
| **mm-correlator** | Superevent correlation engine implementing the RAVEN algorithm |
| **mm-simulation** | Synthetic event generation: GW mergers, GRB counterparts, optical transients, background populations |
| **mm-gcn** | GCN Kafka consumer and VOEvent/JSON alert parsing |
| **mm-boom** | BOOM Kafka consumer for ZTF/LSST optical alerts (Avro format) |
| **mm-redis** | Redis state persistence with schema versioning and automatic recovery |
| **mm-config** | TOML configuration with environment variable overrides |
| **mm-api** | REST API server for event queries and Grafana integration |
| **mm-service** | Executable binaries for all services, demos, and analysis tools |

## Capabilities

### Event Simulation

- **Gravitational wave events**: Synthetic GW detections with realistic SNR, distance, and FAR distributions
- **GRB counterparts**: Simulated Fermi GBM / Swift BAT detections with flux, fluence, localization error ellipses, and jet afterglow modeling
- **Optical transients**: Kilonova, supernova, and fast transient light curves with survey-specific properties (ZTF, LSST)
- **Background populations**: Poisson-distributed background GRBs and optical transients for false-positive characterization
- **O4 observing scenarios**: Full O4 injection sets with realistic skymap localizations (~1000 events)

### Correlation Engine

- **Temporal matching**: O(log n) binary search on BTreeMap indexed by GPS time, configurable windows (GW-GRB: +/-5s, GW-Optical: -1s to +1 day)
- **Spatial matching**: Angular separation and HEALPix credible region overlap computation
- **Joint FAR**: RAVEN false alarm rate calculation combining temporal, spatial, and trial factor probabilities
- **Superevent classification**: Automatic state tracking (GWOnly, GWWithOptical, GWWithGammaRay, MultiMessenger)

### Light Curve Fitting (SVI)

Stochastic Variational Inference for physical t0 (merger/explosion time) estimation:

- **4 models**: Bazin (empirical SN), Villar (improved empirical), PowerLaw (simple rise+decay), MetzgerKN (physical kilonova validated against NMMA)
- **Sub-day precision**: 0.35-0.38 day t0 uncertainties on real ZTF data
- **Real-time performance**: 0.5-1.5s per fit (~60 alerts/minute)
- **Correlator integration**: Automatic t0-based GW+optical correlation with per-measurement fallback

### GP-Based Background Rejection

Gaussian Process regression for light curve feature extraction and background rejection:

- **GP fitting**: RBF + Constant + White kernel with grid search over hyperparameters (amplitude, lengthscale, noise)
- **Feature extraction**: Rise rate, decay rate, peak magnitude, FWHM, derivative features (dfdt_now, dfdt_max, dfdt_min)
- **Soft downweighting**: FAR multiplier based on light curve evolution rates -- fast risers (> 1 mag/day) are boosted as KN-consistent, slow decayers (< 0.3 mag/day) are penalized as SN-like background
- **Configurable**: Enable/disable, adjustable rate thresholds and penalty bounds via `LightCurveFilterConfig`

### Infrastructure

- **Kafka streaming**: Local broker for simulation; NASA GCN Kafka and BOOM (ZTF) for live alerts
- **Redis persistence**: Schema-versioned state storage with TTL management, time-range queries, and automatic recovery on service restart
- **Prometheus + Grafana**: Metrics export (correlation counts, FAR distributions, event rates) with pre-provisioned dashboards
- **REST API**: Event listing, skymap serving (FITS and MOC), and health checks for external integration

## Running the Demo

Open 4 terminal windows from the repository root:

**Terminal 1 -- Correlator service**
```bash
RUST_LOG=info ./target/release/mm-correlator-service
```

**Terminal 2 -- GW + GRB event stream**
```bash
RUST_LOG=info ./target/release/stream-events 0.0333
```

**Terminal 3 -- Optical alert stream** (requires ZTF CSV light curves)
```bash
RUST_LOG=info ./target/release/stream-optical-alerts 0.1 --simulation
```

**Terminal 4 -- Metrics exporter** (optional)
```bash
RUST_LOG=info ./target/release/correlation-exporter
```

The correlator will log multi-messenger detections as events arrive:

```
[INFO] GW event received: sim_id=8, GPS=0.54
[INFO] GRB event received: sim_id=8, GPS=0.54, inst=Fermi GBM
[INFO] Correlation found! GW 8 <-> GRB 8 (dt=0.00s)
[INFO] Overlap: 56.9 sq deg (13.1% of GW, 8.5% of GRB)
[INFO] Optical alert received: ZTF25aaabnwi @ MJD=44244.00
[INFO] THREE-WAY CORRELATION! GW 8 <-> GRB Fermi GBM <-> Optical ZTF25aaabnwi
```

Dashboards are available at:
- **Grafana**: http://localhost:3000 (admin/admin)
- **Prometheus**: http://localhost:9090

### O4 Simulation Mode

For realistic O4 observing run simulations with background populations:

```bash
RUST_LOG=info ./target/release/stream-o4-simulation
```

## Configuration

Generate a config template, then edit with your credentials:

```bash
cargo run --bin generate-config
cp config/config.example.toml config/config.toml
```

Key sections:

```toml
[gcn]
client_id = "your_gcn_client_id"
client_secret = "your_gcn_client_secret"

[boom]
sasl_username = "your_boom_username"
sasl_password = "your_boom_password"

[correlator]
time_window_before = -1.0       # seconds
time_window_after = 86400.0     # seconds (1 day)
spatial_threshold = 5.0         # degrees
far_threshold = 0.0333          # 1/month
```

Environment variables override config file values:

```bash
export GCN_CLIENT_ID=your_client_id
export GCN_CLIENT_SECRET=your_client_secret
export BOOM_SASL_USERNAME=your_username
export BOOM_SASL_PASSWORD=your_password
```

## Testing

```bash
# All tests
cargo test

# Specific crates
cargo test -p mm-core
cargo test -p mm-correlator
cargo test -p mm-simulation

# Redis integration tests (requires running Redis)
docker compose up -d redis
cargo test -p mm-redis -- --ignored --test-threads=1

# Light curve fitting with visual output
cargo test -p mm-core --test lightcurve_fitting_test -- --nocapture
```

Test fixtures in `tests/fixtures/` include O4 observing scenario data, HEALPix skymaps (FITS), GRB VOEvent XMLs, and ZTF light curve CSVs, enabling the full test suite to run without external data dependencies.

## Future Development

- **Skymap-based spatial correlation**: Replace point-source angular separation with full HEALPix skymap overlap for GW-optical matching, using the parsed skymap credible regions already available in mm-core
- **Live alert pipeline**: End-to-end integration with NASA GCN Kafka and BOOM for real O4/O5 alert processing
- **Enhanced kilonova models**: Multi-component ejecta (dynamical + wind), multi-band fitting, and color evolution constraints
- **Neutrino and X-ray correlation**: Extend the correlator to handle IceCube neutrino and Einstein Probe X-ray alerts as full messenger channels (data structures exist, correlation logic pending)
- **Population inference**: Use the simulation framework to characterize detection efficiency and false alarm rates across a population of mergers, informing RAVEN parameter tuning for O5
- **Improved t0 estimation**: Leverage GP features (rise rate, FWHM) as priors for SVI fitting to improve convergence on sparse early-time light curves

## References

- **RAVEN**: Urban et al. (2016), [arXiv:1901.03588](https://arxiv.org/abs/1901.03588) -- Rapid identification of multi-messenger counterparts
- **GCN Kafka**: NASA's General Coordinates Network, [gcn.nasa.gov](https://gcn.nasa.gov/)
- **BOOM**: Rust alert broker for ZTF/LSST optical streams
- **NMMA**: Nuclear physics and Multi-Messenger Astronomy framework for kilonova model validation

## License

MIT
