# Multi-Messenger Correlation Visualization Guide

## 🎯 System Overview

```
Event Producer (30s intervals)
    ↓
Kafka Topics:
  • igwn.gwalert (GW events)
  • gcn.notices.grb (GRB events)
    ↓
Correlator Service (±5s window)
    ↓
mm.correlations topic
    ↓
Prometheus Exporter → Grafana
```

## 📊 Access Services

| Service | URL | Credentials |
|---------|-----|-------------|
| **Grafana** | http://localhost:3000 | admin / admin |
| **Prometheus** | http://localhost:9090 | - |
| **Kafka** | localhost:9092 | - |

## 🚀 Quick Start

### 1. View Correlation Messages (Real-time)

```bash
# Stream correlation messages from Kafka
docker exec mm-kafka kafka-console-consumer \
  --bootstrap-server localhost:9092 \
  --topic mm.correlations \
  --from-beginning

# Pretty-print with jq
docker exec mm-kafka kafka-console-consumer \
  --bootstrap-server localhost:9092 \
  --topic mm.correlations \
  --from-beginning | jq '.'
```

### 2. Check Metrics Exporter Status

```bash
# View exporter logs
tail -f /private/tmp/claude-501/-Users-mcoughlin-Code-ORIGIN-sgn-llai/tasks/b34a500.output

# Check metrics endpoint (if working)
curl http://localhost:9091/metrics
```

### 3. Query Prometheus Directly

Visit http://localhost:9090 and try these PromQL queries:

```promql
# Total correlations
mm_correlations_total

# Average overlap area
mm_avg_overlap_area_sq_deg

# Last GW credible region
mm_last_gw_area_sq_deg

# Last GRB credible region
mm_last_grb_area_sq_deg

# Overlap as fraction of GW
mm_last_overlap_fraction_gw

# Time offset between events
mm_last_time_offset_seconds
```

## 📈 Create Grafana Dashboard

### Option 1: Manual Dashboard Creation

1. Open Grafana: http://localhost:3000
2. Login with admin/admin
3. Click "+" → "Dashboard" → "Add visualization"
4. Select "Prometheus" datasource
5. Add panels with these queries:

**Panel 1: Total Correlations**
```promql
mm_correlations_total
```

**Panel 2: Credible Region Areas**
```promql
mm_last_gw_area_sq_deg
mm_last_grb_area_sq_deg
mm_last_overlap_area_sq_deg
```

**Panel 3: Overlap Fractions**
```promql
mm_last_overlap_fraction_gw * 100
mm_last_overlap_fraction_grb * 100
```

**Panel 4: Time Offsets**
```promql
mm_last_time_offset_seconds
```

### Option 2: Query Kafka Directly in Grafana

Since correlations are published to Kafka, you can use Grafana's JSON API datasource:

1. Install JSON API plugin:
   ```bash
   docker exec mm-grafana grafana-cli plugins install simpod-json-datasource
   docker restart mm-grafana
   ```

2. Create a simple HTTP server that reads from Kafka and serves JSON to Grafana
   (This would require additional implementation)

## 🔍 Monitoring Commands

### Kafka Topics

```bash
# List all topics
docker exec mm-kafka kafka-topics --list --bootstrap-server localhost:9092

# Check topic details
docker exec mm-kafka kafka-topics --describe \
  --topic mm.correlations \
  --bootstrap-server localhost:9092

# Check consumer groups
docker exec mm-kafka kafka-consumer-groups \
  --bootstrap-server localhost:9092 \
  --group mm-correlator \
  --describe
```

### Service Logs

```bash
# Correlator service
tail -f /private/tmp/claude-501/-Users-mcoughlin-Code-ORIGIN-sgn-llai/tasks/b872689.output

# Event producer
tail -f /private/tmp/claude-501/-Users-mcoughlin-Code-ORIGIN-sgn-llai/tasks/b358afc.output

# Metrics exporter
tail -f /private/tmp/claude-501/-Users-mcoughlin-Code-ORIGIN-sgn-llai/tasks/b34a500.output
```

### Docker Services

```bash
# Check all containers
docker-compose ps

# View logs
docker-compose logs -f kafka
docker-compose logs -f grafana
docker-compose logs -f prometheus

# Restart specific service
docker-compose restart grafana
```

## 📊 Example Correlation Message

```json
{
  "simulation_id": 0,
  "gw_gpstime": 2.1691204,
  "grb_detection_time": 2.1691204,
  "time_offset": 0.0,
  "grb_instrument": "Fermi GBM",
  "gw_90cr_area": 294.3,
  "grb_90cr_area": 3997.2,
  "overlap_area": 41.1,
  "overlap_fraction_gw": 0.1397,
  "overlap_fraction_grb": 0.0103,
  "timestamp": 1770435577.018021
}
```

## 🛑 Stop Services

```bash
# Stop background Rust processes
pkill -f stream-events
pkill -f mm-correlator-service
pkill -f correlation-exporter

# Stop Docker services
docker-compose down

# Or keep data and just stop
docker-compose stop
```

## 🔧 Configuration

### Adjust Streaming Rate

```bash
# 1 event per 60 seconds
RUST_LOG=info ./target/release/stream-events 0.01667

# 1 event per 10 seconds
RUST_LOG=info ./target/release/stream-events 0.1
```

### Adjust Correlation Window

Edit `crates/mm-service/src/bin/mm_correlator_service.rs`:

```rust
let time_window = 5.0;  // Change to desired seconds
```

Then rebuild:

```bash
cargo build --release --bin mm-correlator-service
```

## 📝 Next Steps

1. **Persistent Dashboards**: Save Grafana dashboards to JSON and provision them automatically
2. **Alerts**: Set up Grafana alerts for correlation rate drops or anomalies
3. **Real GCN Integration**: Replace mock producer with actual GCN Kafka consumer
4. **Database Storage**: Add PostgreSQL/TimescaleDB for long-term correlation storage
5. **Advanced Metrics**: Track skymap quality, instrument combinations, detection latency
