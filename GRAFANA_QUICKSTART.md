# Grafana Quickstart - O4 Multi-Messenger Correlations

## See Your MMA Correlations in Real-Time! 📊

The system is fully configured to visualize multi-messenger associations in Grafana with:
- Joint FAR (False Alarm Rates)
- Statistical significance (σ)
- P_astro (astrophysical probability)
- Association types (GW+GRB, GW+Optical, Three-way)

## Quick Start (3 Steps)

### 1. Start the Kafka Simulation Stream

In **Terminal 1**:
```bash
cd /Users/mcoughlin/Code/ORIGIN/origin

# Stream O4 events (runs continuously)
./target/release/stream-o4-simulation \
    /Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp \
    --rate 2.0 \
    --max-events 100 \
    --limiting-magnitude 24.5
```

### 2. Start the Correlation Metrics Exporter

In **Terminal 2**:
```bash
cd /Users/mcoughlin/Code/ORIGIN/origin

# Export correlations to Prometheus
RUST_LOG=info ./target/release/correlation-exporter
```

You should see:
```
=== Multi-Messenger Correlation Metrics Exporter ===
📊 Prometheus metrics available at http://localhost:9091/metrics
📡 Subscribed to mm.correlations topic
📈 Updated metrics: 1 total correlations
📈 Updated metrics: 2 total correlations
...
```

### 3. Open Grafana Dashboard

1. **Open browser**: http://localhost:3000
2. **Login**: username `admin`, password `admin`
3. **View dashboard**: Go to Dashboards → "O4 Multi-Messenger Correlations"

## What You'll See

### Dashboard Panels:

1. **Total MMA Correlations** (Gauge)
   - Real-time count of multi-messenger associations
   - Updates every 5 seconds

2. **Significance (σ)** (Time Series)
   - Last significance: Current event's sigma value
   - Max significance: Highest sigma observed (discovery events!)
   - Avg significance: Mean across all correlations

3. **P_astro** (Gauge)
   - Astrophysical probability (0-1)
   - Green = high confidence (>90%)
   - Yellow = moderate (50-90%)
   - Red = low (<50%)

4. **Association Types** (Stacked Bars)
   - GW + GRB detections
   - GW + Optical detections
   - Three-way correlations (GW+GRB+Optical)

5. **Joint False Alarm Rate** (Log Scale)
   - Current FAR per year
   - Minimum FAR (most significant event)

6. **Optical Magnitude** (Time Series)
   - Shows detected optical transient brightness
   - Threshold lines: ZTF (21 mag), LSST (24.5 mag)

7. **Gravitational Wave SNR** (Time Series)
   - GW detection signal-to-noise ratio

## Verify Metrics are Flowing

Check Prometheus directly:
```bash
curl http://localhost:9091/metrics | grep mm_
```

You should see:
```
mm_correlations_total 30
mm_correlations_with_grb 0
mm_correlations_with_optical 1
mm_last_significance_sigma 9.999e308
mm_last_pastro 1.0
mm_last_joint_far_per_year 2.16e-5
...
```

## Troubleshooting

### Dashboard Not Showing Data?

1. **Check Prometheus datasource**:
   - Go to Connections → Data sources
   - Verify "Prometheus" points to `http://prometheus:9090`
   - Test connection

2. **Check metrics exporter is running**:
   ```bash
   curl http://localhost:9091/metrics
   ```

3. **Check Kafka messages are flowing**:
   ```bash
   docker exec mm-kafka kafka-console-consumer \
       --bootstrap-server localhost:9092 \
       --topic mm.correlations \
       --from-beginning --max-messages 5
   ```

### No Correlations Appearing?

- Make sure both the simulator AND exporter are running
- Check that Kafka is running: `docker ps | grep kafka`
- Restart exporter if it was started before the simulator

## Example Output

After running 100 O4 events, you should see:

**Metrics Summary:**
```
Total Correlations: 100
  - With GRB: ~3 (2.7% on-axis rate)
  - With Optical: ~2-3 (LSST detections)
  - Three-way: ~0-1 (rare!)

Max Significance: ~5-35σ (if GW170817-like event in sample)
Avg Significance: ~0.4σ
Min Joint FAR: ~1e-8 to 1e-10 per year

Detection Rates:
  - P_astro > 90%: ~5-10 events
  - P_astro > 99%: ~2-5 events
```

## Advanced: Query Prometheus Directly

Visit http://localhost:9090 and try queries:

```promql
# Correlation rate (events/second)
rate(mm_correlations_total[1m])

# Percentage with GRB
100 * mm_correlations_with_grb / mm_correlations_total

# Three-way correlation fraction
100 * mm_correlations_three_way / mm_correlations_total
```

## Scientific Interpretation

**High Significance Events (>5σ):**
- Extremely rare (~1% of sample)
- Require nearby distance + on-axis GRB + optical detection
- Example: GW170817 at 40 Mpc with optical counterpart → ~35σ

**Typical O4 Event:**
- Distance: ~400 Mpc
- GW SNR: ~8-10
- Joint FAR: ~1e-8 per year (if GRB detected)
- Significance: ~0-3σ (marginal)
- P_astro: ~50-90% (moderate confidence)

**Three-Way Correlations:**
- Require: GW + on-axis GRB (~2.7%) + detectable optical (~2%)
- Rate: ~0.05% of all GW events
- These are the **golden multi-messenger events**!

---

**Status**: ✅ Fully configured and ready to visualize!
**Refresh Rate**: 5 seconds
**Auto-refresh**: Enabled
