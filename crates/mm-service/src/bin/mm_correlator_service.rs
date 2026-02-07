use anyhow::Result;
use mm_core::{OpticalAlert, ParsedSkymap};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::Message;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GWEvent {
    simulation_id: u32,
    gpstime: f64,
    pipeline: String,
    snr: f32,
    far: f64,
    skymap_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GRBEvent {
    simulation_id: u32,
    detection_time: f64,
    ra: f64,
    dec: f64,
    error_radius: f64,
    instrument: String,
    skymap_path: String,
}

#[derive(Debug)]
struct Correlation {
    gw_event: GWEvent,
    grb_event: GRBEvent,
    time_offset: f64,
    gw_90cr_area: f64,
    grb_90cr_area: f64,
    overlap_area: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CorrelationMessage {
    simulation_id: u32,
    gw_gpstime: f64,
    grb_detection_time: f64,
    time_offset: f64,
    grb_instrument: String,
    gw_90cr_area: f64,
    grb_90cr_area: f64,
    overlap_area: f64,
    overlap_fraction_gw: f64,
    overlap_fraction_grb: f64,
    timestamp: f64, // Unix timestamp when correlation was computed
}

struct CorrelatorState {
    gw_events: BTreeMap<u32, GWEvent>,
    grb_events: BTreeMap<u32, GRBEvent>,
    optical_alerts: BTreeMap<String, OpticalAlert>,
    time_window_grb: f64,     // GW-GRB window (seconds)
    time_window_optical: f64, // GW-Optical window (seconds)
    correlations: Vec<Correlation>,
}

impl CorrelatorState {
    fn new(time_window_grb: f64, time_window_optical: f64) -> Self {
        Self {
            gw_events: BTreeMap::new(),
            grb_events: BTreeMap::new(),
            optical_alerts: BTreeMap::new(),
            time_window_grb,
            time_window_optical,
            correlations: Vec::new(),
        }
    }

    fn add_gw_event(&mut self, event: GWEvent, producer: FutureProducer) {
        info!(
            "📡 GW event received: sim_id={}, GPS={:.2}",
            event.simulation_id, event.gpstime
        );

        // Check for matching GRB events
        if let Some(grb_event) = self.grb_events.get(&event.simulation_id) {
            let time_offset = (grb_event.detection_time - event.gpstime).abs();
            if time_offset <= self.time_window_grb {
                info!(
                    "✨ Correlation found! GW {} ↔ GRB {} (Δt={:.2}s)",
                    event.simulation_id, grb_event.simulation_id, time_offset
                );

                // Compute overlap asynchronously and publish to Kafka
                tokio::spawn(Self::compute_and_publish_overlap(
                    event.clone(),
                    grb_event.clone(),
                    time_offset,
                    producer,
                ));
            }
        }

        self.gw_events.insert(event.simulation_id, event);
    }

    fn add_grb_event(&mut self, event: GRBEvent, producer: FutureProducer) {
        info!(
            "🌟 GRB event received: sim_id={}, GPS={:.2}, inst={}",
            event.simulation_id, event.detection_time, event.instrument
        );

        // Check for matching GW events
        if let Some(gw_event) = self.gw_events.get(&event.simulation_id) {
            let time_offset = (event.detection_time - gw_event.gpstime).abs();
            if time_offset <= self.time_window_grb {
                info!(
                    "✨ Correlation found! GW {} ↔ GRB {} (Δt={:.2}s)",
                    gw_event.simulation_id, event.simulation_id, time_offset
                );

                // Compute overlap asynchronously and publish to Kafka
                tokio::spawn(Self::compute_and_publish_overlap(
                    gw_event.clone(),
                    event.clone(),
                    time_offset,
                    producer,
                ));
            }
        }

        self.grb_events.insert(event.simulation_id, event);
    }

    fn add_optical_alert(&mut self, alert: OpticalAlert, producer: FutureProducer) {
        info!(
            "🔭 Optical alert received: {} @ MJD={:.2}, (RA,Dec)=({:.2},{:.2})",
            alert.object_id, alert.mjd, alert.ra, alert.dec
        );

        let optical_gps = alert.gps_time();

        // Find GW events within ±1 day
        let mut matched_gw = Vec::new();
        for (sim_id, gw_event) in &self.gw_events {
            let time_offset = (optical_gps - gw_event.gpstime).abs();
            if time_offset <= self.time_window_optical {
                matched_gw.push((*sim_id, time_offset));
            }
        }

        if !matched_gw.is_empty() {
            info!("   ✨ Found {} GW event(s) within ±1 day", matched_gw.len());

            // Check for three-way correlations (GW + GRB + Optical)
            for (sim_id, time_offset) in matched_gw {
                if let Some(grb_event) = self.grb_events.get(&sim_id) {
                    info!(
                        "   🎯 THREE-WAY CORRELATION! GW {} ↔ GRB {} ↔ Optical {}",
                        sim_id, grb_event.instrument, alert.object_id
                    );
                    info!(
                        "      Time offsets: GW→Optical={:.1}s, GW→GRB={:.1}s",
                        time_offset,
                        (grb_event.detection_time - self.gw_events[&sim_id].gpstime).abs()
                    );
                } else {
                    info!(
                        "   🌟 GW-Optical correlation: GW {} ↔ Optical {} (Δt={:.1}s)",
                        sim_id, alert.object_id, time_offset
                    );
                }
            }
        }

        self.optical_alerts.insert(alert.object_id.clone(), alert);
    }

    async fn compute_and_publish_overlap(
        gw_event: GWEvent,
        grb_event: GRBEvent,
        time_offset: f64,
        producer: FutureProducer,
    ) {
        match Self::compute_overlap_async(&gw_event, &grb_event).await {
            Ok((gw_area, grb_area, overlap_area)) => {
                let overlap_frac_gw = overlap_area / gw_area;
                let overlap_frac_grb = overlap_area / grb_area;

                info!("🎯 Overlap computed for sim_id={}:", gw_event.simulation_id);
                info!("   GW 90% CR:    {:>8.1} sq deg", gw_area);
                info!("   GRB 90% CR:   {:>8.1} sq deg", grb_area);
                info!(
                    "   Overlap:      {:>8.1} sq deg ({:.1}% of GW, {:.1}% of GRB)",
                    overlap_area,
                    100.0 * overlap_frac_gw,
                    100.0 * overlap_frac_grb
                );

                // Create correlation message
                let correlation = CorrelationMessage {
                    simulation_id: gw_event.simulation_id,
                    gw_gpstime: gw_event.gpstime,
                    grb_detection_time: grb_event.detection_time,
                    time_offset,
                    grb_instrument: grb_event.instrument.clone(),
                    gw_90cr_area: gw_area,
                    grb_90cr_area: grb_area,
                    overlap_area,
                    overlap_fraction_gw: overlap_frac_gw,
                    overlap_fraction_grb: overlap_frac_grb,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64(),
                };

                // Publish to Kafka
                let topic = "mm.correlations";
                match serde_json::to_string(&correlation) {
                    Ok(payload) => {
                        let key = gw_event.simulation_id.to_string();
                        let record = FutureRecord::to(topic).payload(&payload).key(&key);

                        match producer.send(record, Duration::from_secs(0)).await {
                            Ok(_) => {
                                info!(
                                    "📤 Published correlation for sim_id={} to {}",
                                    gw_event.simulation_id, topic
                                );
                            }
                            Err((e, _)) => {
                                warn!("❌ Failed to publish correlation: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("❌ Failed to serialize correlation: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!(
                    "❌ Failed to compute overlap for sim_id={}: {}",
                    gw_event.simulation_id, e
                );
            }
        }
    }

    async fn compute_overlap_async(
        gw_event: &GWEvent,
        grb_event: &GRBEvent,
    ) -> Result<(f64, f64, f64)> {
        // Load skymaps
        let gw_skymap = ParsedSkymap::from_fits(&gw_event.skymap_path)?;
        let grb_skymap = ParsedSkymap::from_fits(&grb_event.skymap_path)?;

        // Compute areas
        let gw_90cr_area = gw_skymap.area_90();
        let grb_90cr_area = PI * grb_event.error_radius.powi(2);

        // Compute overlap
        let overlap_area = compute_overlap(&gw_skymap, &grb_skymap)?;

        Ok((gw_90cr_area, grb_90cr_area, overlap_area))
    }
}

fn compute_overlap(gw_skymap: &ParsedSkymap, grb_skymap: &ParsedSkymap) -> Result<f64> {
    // Resample to common resolution
    let target_nside = gw_skymap.nside.min(grb_skymap.nside);

    let gw_probs = resample_skymap(&gw_skymap.probabilities, gw_skymap.nside, target_nside);
    let grb_probs = resample_skymap(&grb_skymap.probabilities, grb_skymap.nside, target_nside);

    // Multiply probability maps
    let mut combined_probs: Vec<f64> = gw_probs
        .iter()
        .zip(grb_probs.iter())
        .map(|(gw_p, grb_p)| gw_p * grb_p)
        .collect();

    // Normalize
    let combined_sum: f64 = combined_probs.iter().sum();
    if combined_sum <= 0.0 {
        return Ok(0.0);
    }

    for p in &mut combined_probs {
        *p /= combined_sum;
    }

    // Find 90% credible region
    let mut indexed_probs: Vec<(usize, f64)> = combined_probs
        .iter()
        .enumerate()
        .map(|(i, &p)| (i, p))
        .collect();
    indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut cumulative_prob = 0.0;
    let mut cr_90_pixels = 0;

    for &(_idx, prob) in &indexed_probs {
        cumulative_prob += prob;
        cr_90_pixels += 1;
        if cumulative_prob >= 0.9 {
            break;
        }
    }

    // Calculate area
    let npix = 12 * target_nside * target_nside;
    let pixel_area = 4.0 * PI / (npix as f64);
    let area_sq_deg = (cr_90_pixels as f64) * pixel_area * (180.0 / PI).powi(2);

    Ok(area_sq_deg)
}

fn resample_skymap(probs: &[f64], from_nside: i64, to_nside: i64) -> Vec<f64> {
    if from_nside == to_nside {
        return probs.to_vec();
    }

    let to_npix = (12 * to_nside * to_nside) as usize;

    if from_nside > to_nside {
        let ratio = ((from_nside / to_nside).pow(2)) as usize;
        let mut resampled = vec![0.0; to_npix];
        for i in 0..to_npix {
            let start_idx = i * ratio;
            resampled[i] = probs[start_idx..start_idx + ratio].iter().sum();
        }
        resampled
    } else {
        let ratio = ((to_nside / from_nside).pow(2)) as usize;
        let mut resampled = vec![0.0; to_npix];
        for (i, &p) in probs.iter().enumerate() {
            let start_idx = i * ratio;
            let child_prob = p / ratio as f64;
            for j in 0..ratio {
                resampled[start_idx + j] = child_prob;
            }
        }
        resampled
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Multi-Messenger Correlator Service ===\n");

    // Configuration
    let time_window_grb = 5.0; // ±5 seconds for GW-GRB
    let time_window_optical = 86400.0; // ±1 day for GW-Optical
    info!("Time windows:");
    info!("  GW-GRB:     ±{} seconds", time_window_grb);
    info!(
        "  GW-Optical: ±{} seconds ({:.1} days)\n",
        time_window_optical,
        time_window_optical / 86400.0
    );

    // Kafka consumer
    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", "mm-correlator")
        .set("bootstrap.servers", "localhost:9092")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .create()?;

    // Kafka producer for correlation results
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("message.timeout.ms", "5000")
        .create()?;

    let gw_topic = "igwn.gwalert";
    let grb_topic = "gcn.notices.grb";
    let optical_topic = "optical.alerts";
    let correlation_topic = "mm.correlations";

    consumer.subscribe(&[gw_topic, grb_topic, optical_topic])?;

    info!("📡 Subscribed to topics:");
    info!("   • {}", gw_topic);
    info!("   • {}", grb_topic);
    info!("   • {}", optical_topic);
    info!("\n📤 Publishing correlations to: {}", correlation_topic);
    info!("\nWaiting for events...\n");

    // Correlator state
    let mut state = CorrelatorState::new(time_window_grb, time_window_optical);
    let mut event_count = 0;

    // Main event loop
    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let topic = msg.topic();
                let payload = match msg.payload_view::<str>() {
                    Some(Ok(p)) => p,
                    Some(Err(e)) => {
                        warn!("Failed to decode message: {}", e);
                        continue;
                    }
                    None => continue,
                };

                event_count += 1;

                // Route to appropriate handler
                match topic {
                    t if t == gw_topic => match serde_json::from_str::<GWEvent>(payload) {
                        Ok(event) => state.add_gw_event(event, producer.clone()),
                        Err(e) => warn!("Failed to parse GW event: {}", e),
                    },
                    t if t == grb_topic => match serde_json::from_str::<GRBEvent>(payload) {
                        Ok(event) => state.add_grb_event(event, producer.clone()),
                        Err(e) => warn!("Failed to parse GRB event: {}", e),
                    },
                    t if t == optical_topic => {
                        match serde_json::from_str::<OpticalAlert>(payload) {
                            Ok(alert) => state.add_optical_alert(alert, producer.clone()),
                            Err(e) => warn!("Failed to parse optical alert: {}", e),
                        }
                    }
                    _ => {}
                }

                if event_count % 100 == 0 {
                    info!("📊 Status: {} GW events, {} GRB events, {} optical alerts, {} correlations",
                         state.gw_events.len(), state.grb_events.len(), state.optical_alerts.len(), state.correlations.len());
                }
            }
            Err(e) => {
                warn!("Kafka error: {}", e);
            }
        }
    }
}
