# Rust Multi-Messenger Superevent Manager

[![CI](https://github.com/yourusername/rust-mm-superevent/workflows/CI/badge.svg)](https://github.com/yourusername/rust-mm-superevent/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

A Rust implementation of a multi-messenger superevent correlator that combines gravitational wave (GW) events, gamma-ray bursts (GRBs), and optical transient alerts. Built for real-time correlation of compact-object formation events across multiple astronomical messengers.

🎯 **Live Demo**: Detects three-way correlations (GW + GRB + Optical) in real-time!

**Status**: ✅ Operational - Three-way correlations actively being detected

## Table of Contents

- [Quick Start](#quick-start)
  - [Prerequisites](#prerequisites)
  - [Running the Demo](#1-clone-and-build)
  - [Troubleshooting](#troubleshooting)
- [Architecture](#architecture)
- [Features](#features)
- [Usage Examples](#usage)
- [Configuration](#configuration)
- [Testing](#testing)
- [Development](#development)
- [References](#references)

## Quick Start

Get the three-way correlation system running in 5 minutes:

### Prerequisites

- **Rust** (1.75+): Install from [rustup.rs](https://rustup.rs)
- **Docker & Docker Compose**: For Kafka infrastructure

### 1. Clone and Build

```bash
git clone https://github.com/mcoughlin/origin.git
cd origin
cargo build --release
```

### 2. Start Infrastructure

```bash
# Start Kafka, Prometheus, Grafana
docker compose up -d

# Verify services are running
docker ps
```

### 3. Run the Three-Way Correlation Demo

Open **4 terminal windows** and run these commands:

**Terminal 1: Start the correlator service**
```bash
RUST_LOG=info ./target/release/mm-correlator-service
```

**Terminal 2: Stream GW + GRB events (30 second intervals)**
```bash
RUST_LOG=info ./target/release/stream-events 0.0333
```

**Terminal 3: Stream optical alerts (10 second intervals, simulation mode)**
```bash
# Default CSV path: /Users/mcoughlin/Code/ORIGIN/lightcurves_csv
# Edit stream_optical_alerts.rs line 64 to change the path
RUST_LOG=info ./target/release/stream-optical-alerts 0.1 --simulation
```

> **Note**: You need ZTF light curve CSV files. If you don't have them, the system will work with just GW-GRB correlations (skip this terminal).

**Terminal 4: Export metrics (optional)**
```bash
RUST_LOG=info ./target/release/correlation-exporter
```

### 4. Watch Three-Way Correlations! 🎉

In **Terminal 1** (correlator), you should see output like:

```
[INFO] === Multi-Messenger Correlator Service ===
[INFO] Time windows:
[INFO]   GW-GRB:     ±5 seconds
[INFO]   GW-Optical: ±86400 seconds (1.0 days)
[INFO] 📡 Subscribed to topics:
[INFO]    • igwn.gwalert
[INFO]    • gcn.notices.grb
[INFO]    • optical.alerts
[INFO]
[INFO] 📡 GW event received: sim_id=8, GPS=0.54
[INFO] 🌟 GRB event received: sim_id=8, GPS=0.54, inst=Fermi GBM
[INFO] ✨ Correlation found! GW 8 ↔ GRB 8 (Δt=0.00s)
[INFO] 🎯 Overlap computed for sim_id=8:
[INFO]    GW 90% CR:       434.3 sq deg
[INFO]    GRB 90% CR:      665.1 sq deg
[INFO]    Overlap:          56.9 sq deg (13.1% of GW, 8.5% of GRB)
[INFO]
[INFO] 🔭 Optical alert received: ZTF25aaabnwi @ MJD=44244.00, (RA,Dec)=(352.00,85.00)
[INFO]    ✨ Found 6 GW event(s) within ±1 day
[INFO]    🎯 THREE-WAY CORRELATION! GW 8 ↔ GRB Fermi GBM ↔ Optical ZTF25aaabnwi
[INFO]       Time offsets: GW→Optical=0.0s, GW→GRB=0.0s
```

**That's the magic!** 🌊💥🔭 The system is correlating gravitational waves, gamma-ray bursts, and optical transients in real-time!

### 5. View Metrics & Dashboards

- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000 (admin/admin)
- **Metrics API**: http://localhost:9091/metrics

### What's Happening?

The system streams three types of events:
- 🌊 **Gravitational Waves** - Simulated LIGO/Virgo/KAGRA detections
- 💥 **Gamma-Ray Bursts** - Simulated Fermi/Swift observations
- 🔭 **Optical Transients** - Real ZTF light curve data (1,019 objects)

When events arrive within the correlation windows (GW-GRB: ±5s, GW-Optical: ±1 day), the correlator detects **three-way matches** and publishes them to the `mm.correlations` Kafka topic!

For detailed documentation, see [THREE_WAY_CORRELATION_DEMO.md](THREE_WAY_CORRELATION_DEMO.md).

### Troubleshooting

**Docker containers not starting?**
```bash
docker compose down
docker compose up -d
docker ps  # Should show mm-kafka, mm-zookeeper, mm-prometheus, mm-grafana
```

**Kafka errors about unknown topics?**
- Topics are created automatically when producers first publish
- Wait 10-20 seconds after starting services before checking for messages

**No optical alerts appearing?**
- Check that the ZTF CSV directory path is correct in `stream_optical_alerts.rs:64`
- Verify CSV files exist: `ls /Users/mcoughlin/Code/ORIGIN/lightcurves_csv/*.csv | wc -l`

**No correlations detected?**
- Make sure all 3 streamers are running (check with `ps aux | grep stream`)
- Correlations require events within time windows (±5s for GW-GRB, ±1 day for GW-Optical)
- In simulation mode, optical alerts use modified times to match GW simulations

### Stopping Services

```bash
# Stop Rust processes (Ctrl+C in each terminal)

# Stop Docker containers
docker compose down
```

### Next Steps

After running the demo:

1. **Explore the data**: Check Prometheus metrics and Grafana dashboards
2. **Read the docs**: See [OPTICAL_INTEGRATION.md](OPTICAL_INTEGRATION.md) for implementation details
3. **Run tests**: `cargo test` to verify all components
4. **Customize**: Edit correlation windows in `mm_correlator_service.rs:298-300`
5. **Production mode**: Remove `--simulation` flag to use real ZTF timestamps

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                   GCN Kafka Topics (NASA)                       │
│  igwn.gwalert                    - GW alerts (LIGO/Virgo/KAGRA)│
│  gcn.notices.swift.bat.guano     - Gamma-ray bursts (Swift)    │
│  gcn.notices.einstein_probe.*    - X-ray transients            │
│  gcn.notices.icecube.*           - Neutrino alerts             │
└────────────────────────────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────┐
│              GCN Kafka Consumer (mm-gcn)                        │
│  - Parse multi-messenger alerts                                │
│  - Normalize to internal Event types                           │
└────────────────────────────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────┐
│              MultiMessenger Superevent Correlator               │
│  - Temporal matching (±time window)                            │
│  - Spatial matching (sky position overlap)                     │
│  - Joint FAR calculation (RAVEN algorithm)                     │
│  - Maintains active superevent state                           │
└────────────────────────────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────┐
│                   BOOM Kafka Consumer                           │
│  Live: kaboom.caltech.edu:9093                                 │
│  Simulation: ZTF CSV light curves                              │
└────────────────────────────────────────────────────────────────┘
```

## Crates

- **mm-core**: Core data structures (events, time, positions, light curves)
- **mm-gcn**: GCN Kafka consumer and alert parsers
- **mm-correlator**: Superevent correlation engine with RAVEN algorithm
- **mm-boom**: BOOM Kafka consumer and simulation mode
- **mm-redis**: Redis state persistence with schema versioning and recovery
- **mm-config**: Configuration management with TOML and environment variables
- **mm-service**: Main binaries and services

## Features

### Phase 3 ✅ Complete
- Temporal clustering using binary search (O(log n))
- Spatial matching with angular separation
- Joint FAR (False Alarm Rate) calculation using RAVEN formula
- Multi-messenger superevent state management
- In-memory BTreeMap index for fast time-based lookups

### Phase 4 ✅ Complete
- BOOM Kafka integration for live optical alerts
- Simulation mode using ZTF CSV light curves
- Light curve parsing and processing
- MJD ↔ GPS time conversion
- Optical candidate matching with GW superevents

### Phase 5 ✅ Partial
- **Configuration management** ✅ - TOML files with environment variable overrides
- **Redis state persistence** ✅ - Automatic recovery on restart with schema versioning
- Kafka producer for publishing multi-messenger superevents 🚧
- Prometheus metrics 🚧

### Redis State Persistence ✅

The correlator now persists state to Redis, enabling service restarts without data loss!

**Features:**
- Automatic state recovery on service restart
- Schema-versioned storage with graceful degradation
- TTL management: 2 hours for GW/GRB events, 1 day for optical alerts
- Time-range queries using Redis sorted sets
- Non-blocking persistence via tokio::spawn
- Graceful degradation when Redis unavailable

See [`crates/mm-redis/RECOVERY_DEMO.md`](crates/mm-redis/RECOVERY_DEMO.md) for detailed documentation.

## Installation

See [Quick Start](#quick-start) above for the fastest way to get running.

**Manual build:**
```bash
cargo build --release

# Build specific binaries
cargo build --release --bin mm-correlator-service
cargo build --release --bin stream-events
cargo build --release --bin stream-optical-alerts
cargo build --release --bin correlation-exporter
```

## Configuration

### Setup

1. **Copy the example configuration:**
   ```bash
   cp config/config.example.toml config/config.toml
   ```

2. **Edit `config/config.toml` with your credentials:**
   ```toml
   [gcn]
   client_id = "your_gcn_client_id"
   client_secret = "your_gcn_client_secret"

   [boom]
   sasl_username = "your_boom_username"
   sasl_password = "your_boom_password"
   ```

3. **Or use environment variables:**
   ```bash
   export GCN_CLIENT_ID=your_client_id
   export GCN_CLIENT_SECRET=your_client_secret
   export BOOM_SASL_USERNAME=your_username
   export BOOM_SASL_PASSWORD=your_password
   export ZTF_CSV_DIR=/path/to/ztf/csv
   ```

### Configuration File Structure

The configuration file (`config/config.toml`) contains:

- **GCN credentials**: For connecting to NASA's GCN Kafka
- **BOOM credentials**: For connecting to BOOM's optical alert stream
- **Correlator parameters**: RAVEN time windows, spatial thresholds, FAR thresholds
- **Simulation settings**: Enable/disable simulation mode, CSV directory path

See [`config/config.example.toml`](config/config.example.toml) for full documentation.

### Generate Config Template

```bash
cargo run --bin generate-config
```

## Usage

### 1. Multi-Messenger Correlator Demo

Process 1,019 real ZTF light curves and correlate with synthetic GW event:

```bash
cargo run --bin mm-correlator-demo
```

**Output:**
```
INFO Starting Multi-Messenger Correlator Demo
INFO Loading ZTF light curves from: /Users/mcoughlin/Code/ORIGIN/lightcurves_csv
INFO Loaded 1019 light curves
INFO First ZTF measurement: MJD 60831.46 → GPS 1433156802.00
INFO Setting GW trigger time to GPS 1433153202.00 (1 hour before)
INFO Processing synthetic GW event: S240101a
INFO Created superevents: ["MS000001"]
...
=== Final Statistics ===
INFO Total superevents: 1
INFO GW-only: 0
INFO With optical: 1
INFO Optical alerts processed: 1019
INFO Optical matches found: 129

=== Multi-Messenger Superevents ===
INFO Superevent MMS240101a:
INFO   GW event: S240101a
INFO   t_0: 1433153202.00 (GPS)
INFO   Classification: GWWithOptical
INFO   Optical candidates: 564
INFO     1. Object: ZTF25aatrvpg
INFO        Time offset: 3600.00 s
INFO        Spatial offset: 0.00 deg
INFO        SNR: 14.45
INFO        Joint FAR: 1.54e-7 /yr
```

### 2. GCN Kafka Consumer

Connect to live GCN Kafka stream (requires credentials):

```bash
cargo run --bin mm-service
```

Subscribes to:
- `igwn.gwalert` - Gravitational wave alerts
- `gcn.notices.swift.bat.guano` - Gamma-ray bursts
- `gcn.notices.einstein_probe.wxt.alert` - X-ray transients
- `gcn.notices.icecube.*` - Neutrino alerts

### 3. Analyze ZTF Light Curves

Analyze ZTF CSV files:

```bash
cargo run --bin analyze-ztf
```

**Output:**
```
INFO Loading light curves from: /Users/mcoughlin/Code/ORIGIN/lightcurves_csv
INFO Loaded 1019 light curves with 110536 total measurements
INFO Analyzing light curves...

Statistics:
  Total objects: 1019
  Total measurements: 110536
  Measurements per object: 108.5 (mean), 39.0 (median)
  Bands: [("r", 59421), ("g", 51115)]
  SNR range: [1.26, 5263.36]
  Time range: MJD [60831.46, 60891.59] (60.1 days)
```

## Correlation Algorithm

### Temporal Matching

Uses binary search on a BTreeMap indexed by GPS time:

```rust
pub struct TemporalIndex {
    times: BTreeMap<OrderedFloat<f64>, String>,
}
```

Search window (RAVEN parameters):
- **Before GW**: -1 second
- **After GW**: +86400 seconds (1 day)

### Spatial Matching

Angular separation calculation:

```rust
let separation = pos1.angular_separation(pos2);
if separation <= spatial_threshold {
    // Match found
}
```

### Joint FAR Calculation

RAVEN formula:
```
FAR = background_rate × time_prob × spatial_prob × trials_factor
```

Where:
- `time_prob = 1 / time_window`
- `spatial_prob = search_area / sky_area`
- `trials_factor = 7` (for 7 photometric bands)
- `background_rate = 1.0` (1 alert per year)

Significance threshold: `FAR < 1/30` (1 per month)

## Testing

### Unit & Integration Tests

Run all tests:

```bash
cargo test
```

Run specific test suite:

```bash
cargo test --package mm-correlator
cargo test --package mm-core
cargo test --package mm-redis
```

### Redis State Recovery Tests

These tests require Redis to be running:

```bash
# Start Redis
docker compose up -d redis

# Run Redis integration tests (with --ignored flag)
cargo test -p mm-redis -- --ignored --test-threads=1
cargo test -p mm-service --test state_recovery_integration -- --ignored --test-threads=1
```

The tests verify:
- Events are persisted to Redis
- Service can restart and recover state
- Time-based filtering works correctly
- TTL expiration is handled properly

### Test Fixtures

Sample data files are provided in [`tests/fixtures/`](tests/fixtures/):
- **Observing scenarios**: 9 GW simulation data files (O5a, O4HL, O5c runs)
- **GRB XMLs**: 10 VOEvent XML files (Fermi, Swift, Einstein Probe, SVOM)
- **Optical light curves**: 10 ZTF CSV files with real transient data

These fixtures enable tests to run without external data dependencies or downloads.

See [`tests/fixtures/README.md`](tests/fixtures/README.md) for details.

## Data Formats

### ZTF CSV Format

Light curves stored in `/Users/mcoughlin/Code/ORIGIN/lightcurves_csv`:

```csv
objectId,jd,flux,flux_err,band
ZTF25aatrvpg,2460831.46,1000.0,10.0,r
ZTF25aatrvpg,2460831.52,1050.0,12.0,g
```

### GCN Alert Format

GW alerts (`igwn.gwalert`):
```json
{
  "superevent_id": "S240101a",
  "alert_type": "PRELIMINARY",
  "time": 1234567890.0,
  "instruments": ["H1", "L1", "V1"],
  "far": 1e-10
}
```

### BOOM Alert Format

Avro-encoded with schema:
- `objectId`: ZTF identifier
- `candid`: Candidate ID
- `ra`, `dec`: Sky position
- `jd`: Julian date
- `magpsf`, `sigmapsf`: PSF magnitude
- `fid`: Filter ID (1=g, 2=r, 3=i)
- `drb`: Real/bogus score
- `prv_candidates`: Previous photometry

## Performance

- **Light curve loading**: 1,019 objects in ~300ms
- **Correlation**: 1,019 alerts processed in ~20ms
- **Memory**: ~50MB for 1,000 superevents with 100k optical candidates
- **Temporal search**: O(log n) binary search

## Time Conventions

- **GPS time**: Gravitational wave standard (seconds since GPS epoch)
- **MJD (Modified Julian Date)**: Optical astronomy standard (days)
- **Unix time**: Standard Unix epoch (seconds since 1970-01-01)

Conversions:
```rust
// MJD → GPS
let gps = (mjd - 40587.0) * 86400.0 - 315964800.0 + 18.0;

// GPS → MJD
let mjd = (gps + 315964800.0 - 18.0) / 86400.0 + 40587.0;
```

## Dependencies

Core:
- `tokio` - Async runtime
- `rdkafka` - Kafka client (SASL authentication)
- `apache-avro` - BOOM alert parsing
- `serde` - Serialization
- `chrono` - Time handling

Astronomy:
- `ordered-float` - Ordered float types for BTreeMap
- `healpix` - Sky pixelization (planned)

## Development

### Pre-commit Hooks

Install [pre-commit](https://pre-commit.com/) to automatically format and lint code before commits:

```bash
# Install pre-commit (macOS)
brew install pre-commit

# Or with pip
pip install pre-commit

# Install git hooks
pre-commit install

# Run manually on all files
pre-commit run --all-files
```

The pre-commit hooks will automatically:
- Format code with `cargo fmt`
- Lint code with `cargo clippy`

### Manual Commands

```bash
# Format code
cargo fmt

# Check formatting without modifying files
cargo fmt --check

# Run linter
cargo clippy -- -D warnings

# Run linter on all targets
cargo clippy --all-targets --all-features -- -D warnings

# Watch mode (requires cargo-watch)
cargo install cargo-watch
cargo watch -x "run --bin mm-correlator-demo"
```

## References

- **RAVEN**: Rapid identification of multi-messenger counterparts ([arXiv:1901.03588](https://arxiv.org/abs/1901.03588))
- **GCN Kafka**: NASA's GCN Kafka archive ([gcn.nasa.gov](https://gcn.nasa.gov/))
- **BOOM**: Rust alert broker for ZTF/LSST
- **SGN-LLAI**: Python GW superevent creation pipeline

## License

MIT
