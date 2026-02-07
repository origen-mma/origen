# Optical Transient Integration - Status

## ✅ Phase 1: Infrastructure Complete

### New Components

1. **`mm-core/src/optical.rs`** - Optical alert data structures
   - `OpticalAlert` struct with light curve support
   - `PhotometryPoint` for individual measurements
   - Survey enum (ZTF, LSST, DECam, ATLAS)
   - MJD to GPS time conversion
   - Flux to magnitude conversion
   - Rising/fading transient detection
   - 3 unit tests passing

2. **`stream-optical-alerts` binary** - Kafka producer for ZTF light curves
   - Loads CSV files from `/Users/mcoughlin/Code/ORIGIN/lightcurves_csv`
   - 1019 ZTF light curves available
   - Publishes to `optical.alerts` Kafka topic
   - Configurable streaming rate
   - JSON serialization compatible with correlator

### Data Format

**CSV Structure:**
```csv
mjd,flux,flux_err,filter
60675.103090,13.363330,3.102672,g
60675.143912,18.534034,5.837915,r
...
```

**Kafka Message Structure:**
```json
{
  "object_id": "ZTF25aaaalin",
  "mjd": 60675.103090,
  "ra": 123.45,
  "dec": 67.89,
  "survey": "ZTF",
  "magnitude": null,
  "mag_err": null,
  "filter": "g",
  "light_curve": [
    {"mjd": 60675.103090, "flux": 13.363330, "flux_err": 3.102672, "filter": "g"},
    ...
  ],
  "filters_passed": [],
  "classifications": []
}
```

## ✅ Phase 2: Correlation Engine Complete

### Tasks Remaining

1. **Extend Correlator Service**
   ```rust
   // mm-service/src/bin/mm_correlator_service.rs

   // Subscribe to optical alerts topic
   consumer.subscribe(&[gw_topic, grb_topic, "optical.alerts"])?;

   // Handle optical events
   match topic {
       "optical.alerts" => {
           let alert: OpticalAlert = serde_json::from_str(payload)?;
           state.add_optical_alert(alert, producer.clone());
       }
       ...
   }
   ```

2. **Temporal Correlation**
   - GW-Optical window: ±1 day (86400 seconds)
   - Match optical alerts to GW events by GPS time
   - Search for transients appearing after GW trigger

3. **Spatial Correlation**
   - Check if optical alert falls within GW skymap 90% CR
   - Calculate probability at alert position
   - Prioritize candidates by skymap probability

4. **Three-Way Matching**
   - GW + GRB + Optical coincidences
   - Combined significance calculation
   - Multi-messenger classification

### Implementation Plan

```rust
// mm-correlator-service: Add optical handling

struct CorrelatorState {
    gw_events: BTreeMap<u32, GWEvent>,
    grb_events: BTreeMap<u32, GRBEvent>,
    optical_alerts: BTreeMap<String, OpticalAlert>,  // NEW
    time_window_gw_grb: f64,     // ±5s
    time_window_gw_optical: f64, // ±1 day (NEW)
    correlations: Vec<Correlation>,
}

impl CorrelatorState {
    fn add_optical_alert(&mut self, alert: OpticalAlert, producer: FutureProducer) {
        // Convert MJD to GPS time
        let optical_gps = alert.gps_time();

        // Find GW events within ±1 day
        for (_, gw_event) in &self.gw_events {
            let time_offset = (optical_gps - gw_event.gpstime).abs();

            if time_offset <= self.time_window_gw_optical {
                // Check spatial overlap
                if self.check_in_skymap(&alert, &gw_event.skymap) {
                    info!("🌟 GW-Optical correlation! {} ↔ {}",
                          gw_event.graceid, alert.object_id);

                    // Check for three-way correlation with GRB
                    if let Some(grb) = self.find_matching_grb(gw_event) {
                        info!("🎯 THREE-WAY CORRELATION!");
                        self.publish_multimessenger_alert(
                            gw_event, grb, &alert, producer
                        );
                    }
                }
            }
        }

        self.optical_alerts.insert(alert.object_id.clone(), alert);
    }

    fn check_in_skymap(&self, alert: &OpticalAlert, skymap: &Option<Skymap>) -> bool {
        if let Some(skymap) = skymap {
            // Check if alert position falls in 90% CR
            skymap.contains_position(alert.ra, alert.dec)
        } else {
            false
        }
    }
}
```

## 🔬 Testing Strategy

### Unit Tests
```rust
#[test]
fn test_optical_gw_correlation() {
    let gw_time = 1000.0;  // GPS seconds
    let optical_mjd = 60675.0;
    let optical_gps = convert_mjd_to_gps(optical_mjd);

    let time_offset = (optical_gps - gw_time).abs();
    assert!(time_offset < 86400.0);  // Within 1 day
}

#[test]
fn test_optical_in_skymap() {
    let skymap = create_test_skymap();
    let alert = OpticalAlert {
        ra: 123.45,
        dec: 67.89,
        ...
    };

    assert!(skymap.contains_position(alert.ra, alert.dec));
}
```

### Integration Test
```bash
# Terminal 1: Start correlator
RUST_LOG=info ./target/release/mm-correlator-service

# Terminal 2: Stream GW events
RUST_LOG=info ./target/release/stream-events 0.033

# Terminal 3: Stream optical alerts
RUST_LOG=info ./target/release/stream-optical-alerts 0.1

# Watch for three-way correlations!
```

## 📊 Expected Results

With 6013 GW simulations and 1019 optical alerts:
- **Expected GW-Optical matches**: ~10-50 (depending on time window)
- **Expected three-way (GW+GRB+Optical)**: ~5-20
- **False positive rate**: <5%

### Metrics to Add

```promql
# Optical alerts processed
mm_optical_alerts_total

# GW-Optical correlations
mm_gw_optical_correlations_total

# Three-way correlations
mm_three_way_correlations_total

# Optical alerts in GW skymap
mm_optical_in_skymap_total
```

## 🎯 Session Progress

1. [x] Update `mm-correlator-service.rs`:
   - [x] Subscribe to `optical.alerts` topic
   - [x] Add `add_optical_alert()` method
   - [x] Implement GW-Optical matching (±1 day temporal window)
   - [x] Implement three-way correlation detection

2. [ ] Add optical correlation tests
   - [ ] Temporal matching
   - [ ] Spatial matching (skymap containment)
   - [ ] Three-way correlation

3. [ ] Update metrics exporter:
   - [ ] Add optical-specific metrics
   - [ ] Track GW-Optical correlation rate

4. [ ] Test end-to-end:
   - [ ] Stream all three event types
   - [ ] Verify correlations detected
   - [ ] Check Grafana dashboards

5. [ ] Documentation:
   - [ ] Update README with optical support
   - [ ] Add optical correlation guide
   - [ ] Update ROADMAP

## 📁 Files Modified/Created

- ✅ `crates/mm-core/src/optical.rs` (NEW)
- ✅ `crates/mm-core/src/lib.rs` (MODIFIED)
- ✅ `crates/mm-service/src/bin/stream_optical_alerts.rs` (NEW)
- ✅ `crates/mm-service/Cargo.toml` (MODIFIED)
- ✅ `crates/mm-service/src/bin/mm_correlator_service.rs` (MODIFIED - Optical support added!)
- ⏳ `crates/mm-service/src/bin/correlation_exporter.rs` (TODO - metrics)
- ⏳ `crates/mm-correlator/tests/correlation_tests.rs` (TODO - tests)

## 🚀 Ready to Run

**Current state**: Infrastructure complete, binary compiled

**Run optical streamer**:
```bash
cd /Users/mcoughlin/Code/ORIGIN/sgn-llai/rust-mm-superevent

# Build
cargo build --release --bin stream-optical-alerts

# Run (1 alert per 10 seconds)
RUST_LOG=info ./target/release/stream-optical-alerts 0.1
```

**Next**: Extend correlator to consume optical alerts!

---

This is the foundation for three-messenger astrophysics: **GW + GRB + Optical = Complete Picture! 🔭✨**
