# Installation

## Prerequisites

- **Rust 1.75+** -- install from [rustup.rs](https://rustup.rs)
- **Docker & Docker Compose** -- for Kafka, Redis, Prometheus, Grafana
- **Python 3.8+** with `matplotlib` and `numpy` -- for plotting (optional)

### System Libraries

=== "Linux (Ubuntu/Debian)"

    ```bash
    sudo apt-get update
    sudo apt-get install -y libcfitsio-dev libsasl2-dev libfontconfig1-dev
    ```

=== "macOS"

    ```bash
    brew install cfitsio cyrus-sasl
    ```

## Building

```bash
cargo build --release
```

This produces ~30 binaries in `target/release/`, including:

| Binary | Purpose |
|---|---|
| `mm-correlator-service` | Main real-time correlator service |
| `stream-events` | GW + GRB event generator |
| `stream-optical-alerts` | ZTF optical alert streamer |
| `gcn-correlator` | Live GCN Kafka consumer + correlator |
| `daily-comparison` | Daily GCN vs BOOM comparison service |
| `far-tuning-campaign` | FAR calibration injection campaign |

## Infrastructure

Start the supporting services:

```bash
docker compose up -d
```

This launches:

- **Kafka** (port 9092) -- message bus for event streaming
- **Redis** (port 6379) -- state persistence for superevents
- **Prometheus** (port 9090) -- metrics collection
- **Grafana** (port 3000) -- dashboards (admin/admin)

## Running Tests

```bash
# Unit and integration tests
cargo test --all

# With Redis integration tests (requires running Redis)
cargo test --all -- --ignored
```
