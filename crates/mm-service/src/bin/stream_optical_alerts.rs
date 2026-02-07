use anyhow::Result;
use mm_core::{OpticalAlert, PhotometryPoint, Survey};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::config::ClientConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tracing::info;

#[derive(Debug, Deserialize)]
struct LightCurveRow {
    mjd: f64,
    flux: f64,
    flux_err: f64,
    filter: String,
}

fn load_light_curve_csv(path: &Path) -> Result<Vec<PhotometryPoint>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut points = Vec::new();

    for result in reader.deserialize() {
        let row: LightCurveRow = result?;
        points.push(PhotometryPoint {
            mjd: row.mjd,
            flux: row.flux,
            flux_err: row.flux_err,
            filter: row.filter,
        });
    }

    Ok(points)
}

fn extract_coordinates_from_filename(filename: &str) -> (f64, f64) {
    // For now, use placeholder coordinates
    // In production, these would come from a catalog or alert packet
    // Use hash of filename to generate pseudo-random but consistent coordinates
    let hash = filename.chars().fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u64));
    let ra = (hash % 360) as f64;
    let dec = ((hash / 360) % 180) as f64 - 90.0;
    (ra, dec)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Optical Alert Streaming Producer ===\n");

    // Kafka configuration
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("message.timeout.ms", "5000")
        .create()?;

    let optical_topic = "optical.alerts";

    info!("Connected to Kafka broker at localhost:9092");
    info!("Topic: {}\n", optical_topic);

    // Load light curves from CSV directory
    let csv_dir = "/Users/mcoughlin/Code/ORIGIN/lightcurves_csv";
    info!("Loading light curves from: {}", csv_dir);

    let mut csv_files: Vec<_> = fs::read_dir(csv_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "csv")
                .unwrap_or(false)
        })
        .collect();

    csv_files.sort_by_key(|e| e.file_name());

    info!("Found {} light curve CSV files\n", csv_files.len());

    // Get streaming parameters from command line
    let args: Vec<String> = std::env::args().collect();
    let rate_hz: f64 = if args.len() > 1 {
        args[1].parse()?
    } else {
        0.1  // Default: 1 alert per 10 seconds
    };
    let delay_ms = (1000.0 / rate_hz) as u64;

    // Simulation mode: use MJD values that match GW simulation GPS times
    let simulation_mode = args.len() > 2 && args[2] == "--simulation";

    info!("Streaming rate: {:.3} Hz ({} ms between alerts)", rate_hz, delay_ms);
    if simulation_mode {
        info!("🎯 SIMULATION MODE: Using MJD times matching GW simulation (GPS 0-10s)");
    }
    info!("Press Ctrl+C to stop\n");

    // Stream optical alerts
    let mut alert_count = 0;

    for entry in csv_files.iter() {
        let path = entry.path();
        let filename = path.file_stem().unwrap().to_str().unwrap();

        // Load light curve
        let light_curve = match load_light_curve_csv(&path) {
            Ok(lc) => lc,
            Err(e) => {
                tracing::warn!("Failed to load {}: {}", filename, e);
                continue;
            }
        };

        if light_curve.is_empty() {
            continue;
        }

        // Extract coordinates (placeholder - would come from catalog)
        let (ra, dec) = extract_coordinates_from_filename(filename);

        // Get first detection time
        let first_mjd = light_curve.iter().map(|p| p.mjd).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

        // In simulation mode, override MJD to match GW simulation GPS times
        // GPS epoch = MJD 44244.0
        // GW simulation uses GPS times 0-10 seconds
        // So we need MJD around 44244.0 + offset in days
        let detection_mjd = if simulation_mode {
            // Spread optical alerts across ~10 days to overlap with GW simulations
            // Each GW event is ~30 seconds apart, so spread optical over similar timeframe
            let offset_days = (alert_count as f64 * 0.5) / 86400.0;  // 0.5 seconds between optical in days
            44244.0 + offset_days
        } else {
            first_mjd
        };

        // Create optical alert
        let alert = OpticalAlert {
            object_id: filename.to_string(),
            mjd: detection_mjd,
            ra,
            dec,
            survey: Survey::ZTF,
            magnitude: None,  // Will be computed from flux
            mag_err: None,
            filter: light_curve[0].filter.clone(),
            light_curve,
            filters_passed: vec![],
            classifications: vec![],
        };

        // Publish to Kafka
        let payload = serde_json::to_string(&alert)?;
        let key = alert.object_id.clone();
        let record = FutureRecord::to(optical_topic)
            .payload(&payload)
            .key(&key);

        producer.send(record, Duration::from_secs(0)).await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send optical alert: {}", e))?;

        alert_count += 1;

        if alert_count % 10 == 0 {
            info!("Streamed {} optical alerts", alert_count);
        }

        // Rate limiting
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    info!("\n✅ Streaming complete!");
    info!("Total optical alerts: {}", alert_count);

    Ok(())
}
