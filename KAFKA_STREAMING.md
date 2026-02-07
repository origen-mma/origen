# Multi-Messenger Event Streaming & Correlation

This demonstrates real-time multi-messenger event correlation using Kafka.

## Architecture

```
┌─────────────────────┐
│  stream-events      │  Producer: Publishes GW and GRB events
│  (Producer)         │  to Kafka topics
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│   Kafka Broker      │  Topics:
│   (Docker)          │    • igwn.gwalert (GW events)
│                     │    • gcn.notices.grb (GRB events)
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ mm-correlator-      │  Consumer: Subscribes to both topics,
│ service             │  correlates events within time window,
│  (Consumer)         │  computes skymap overlap
└─────────────────────┘
```

## Quick Start

### 1. Start Kafka Broker

```bash
# From rust-mm-superevent directory
docker-compose up -d

# Verify containers are running
docker ps

# You should see:
#  - mm-kafka
#  - mm-zookeeper
```

### 2. Build Rust Binaries

```bash
cargo build --release --bin stream-events
cargo build --release --bin mm-correlator-service
```

### 3. Start the Correlator Service (Consumer)

In one terminal:

```bash
RUST_LOG=info ./target/release/mm-correlator-service
```

You should see:
```
=== Multi-Messenger Correlator Service ===

Time window: ±30 seconds

📡 Subscribed to topics:
   • igwn.gwalert
   • gcn.notices.grb

Waiting for events...
```

### 4. Stream Events (Producer)

In another terminal:

```bash
# Stream at 1 Hz (default: 1 event per second)
RUST_LOG=info ./target/release/stream-events 1.0

# Or faster (10 Hz):
RUST_LOG=info ./target/release/stream-events 10.0
```

### 5. Watch Correlations Happen!

The correlator will automatically:
1. Receive GW and GRB events from Kafka
2. Match events within ±30 second time window
3. Compute skymap overlap for matching pairs
4. Log results in real-time

Example output:
```
📡 GW event received: sim_id=1, GPS=1234567890.52
🌟 GRB event received: sim_id=1, GPS=1234567895.32, inst=Fermi GBM
✨ Correlation found! GW 1 ↔ GRB 1 (Δt=4.80s)
🎯 Overlap computed for sim_id=1:
   GW 90% CR:       280.8 sq deg
   GRB 90% CR:       314.2 sq deg
   Overlap:          152.5 sq deg (54.3% of GW, 48.5% of GRB)
```

## Event Format

### GW Event (igwn.gwalert)
```json
{
  "simulation_id": 1,
  "gpstime": 1234567890.52,
  "pipeline": "SGNL",
  "snr": 15.3,
  "far": 1e-8,
  "skymap_path": "/path/to/skymap.fits"
}
```

### GRB Event (gcn.notices.grb)
```json
{
  "simulation_id": 1,
  "detection_time": 1234567895.32,
  "ra": 123.456,
  "dec": 45.123,
  "error_radius": 10.0,
  "instrument": "Fermi GBM",
  "skymap_path": "/path/to/grb_skymap.fits"
}
```

## Configuration

### Time Window
Edit `mm_correlator_service.rs`:
```rust
let time_window = 30.0;  // seconds
```

### Streaming Rate
```bash
./target/release/stream-events <RATE_HZ>

# Examples:
./target/release/stream-events 0.1   # 1 event per 10 seconds
./target/release/stream-events 1.0   # 1 event per second
./target/release/stream-events 10.0  # 10 events per second
```

## Monitoring Kafka

### List Topics
```bash
docker exec mm-kafka kafka-topics --list --bootstrap-server localhost:9092
```

### Consume Messages Manually
```bash
# GW events
docker exec mm-kafka kafka-console-consumer --bootstrap-server localhost:9092 \
  --topic igwn.gwalert --from-beginning

# GRB events
docker exec mm-kafka kafka-console-consumer --bootstrap-server localhost:9092 \
  --topic gcn.notices.grb --from-beginning
```

### Check Consumer Groups
```bash
docker exec mm-kafka kafka-consumer-groups --bootstrap-server localhost:9092 --list
docker exec mm-kafka kafka-consumer-groups --bootstrap-server localhost:9092 \
  --group mm-correlator --describe
```

## Shutdown

```bash
# Stop correlator service: Ctrl+C

# Stop Kafka broker
docker-compose down

# Stop and remove volumes (clean slate)
docker-compose down -v
```

## Production Considerations

For real GCN integration:

1. **Replace Mock Producer** with actual GCN Kafka consumer
   - Use `gcn-kafka` crate for authentication
   - Subscribe to real topics: `igwn.gwalert`, `gcn.notices.swift.bat.guano`, etc.

2. **Add State Persistence**
   - Store correlations in database (PostgreSQL, Redis)
   - Persist correlation state for crash recovery

3. **Add Metrics**
   - Prometheus metrics for event rates, correlation rates, latencies
   - Grafana dashboards

4. **Add Output Sink**
   - Publish correlations to new Kafka topic
   - Upload to GraceDB
   - Send alerts (email, Slack, etc.)

5. **Distributed Deployment**
   - Multiple Kafka brokers for HA
   - Multiple consumer instances for parallelism
   - Kubernetes for orchestration

## Troubleshooting

**Kafka connection refused:**
```bash
# Check if Kafka is running
docker ps

# Check Kafka logs
docker logs mm-kafka
```

**No events received:**
```bash
# Check if topics exist
docker exec mm-kafka kafka-topics --list --bootstrap-server localhost:9092

# Reset consumer offset to beginning
docker exec mm-kafka kafka-consumer-groups --bootstrap-server localhost:9092 \
  --group mm-correlator --reset-offsets --to-earliest --all-topics --execute
```

**Skymap files not found:**
- Ensure you've run `generate-grb-simulations` and `generate-grb-skymaps`
- Check paths in injections.dat and grb_params.dat
