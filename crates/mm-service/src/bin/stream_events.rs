use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct GWEvent {
    simulation_id: u32,
    gpstime: f64,
    pipeline: String,
    snr: f32,
    far: f64,
    skymap_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GRBEvent {
    simulation_id: u32,
    detection_time: f64,  // GPS time
    ra: f64,
    dec: f64,
    error_radius: f64,
    instrument: String,
    skymap_path: String,
}

#[derive(Debug)]
struct InjectionParams {
    simulation_id: u32,
    gpstime: f64,
}

#[derive(Debug)]
struct GrbParams {
    simulation_id: u32,
    ra: f64,
    dec: f64,
    error_radius: f64,
    instrument: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Multi-Messenger Event Streaming Producer ===\n");

    // Kafka configuration
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("message.timeout.ms", "5000")
        .create()?;

    let gw_topic = "igwn.gwalert";
    let grb_topic = "gcn.notices.grb";

    info!("Connected to Kafka broker at localhost:9092");
    info!("Topics: {} and {}\n", gw_topic, grb_topic);

    // Load injection parameters
    info!("Loading simulation data...");
    let base_path = "/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp";
    let injections_file = format!("{}/injections.dat", base_path);
    let injections = read_injection_params(&injections_file)?;

    let grb_params_file = "simulated_grbs/O4HL/bgp/grb_params.dat";
    let grb_params = read_grb_params(grb_params_file)?;

    info!("Loaded {} GW injections", injections.len());
    info!("Loaded {} GRB parameters\n", grb_params.len());

    // Get streaming parameters from command line
    let args: Vec<String> = std::env::args().collect();
    let rate_hz: f64 = if args.len() > 1 {
        args[1].parse()?
    } else {
        1.0  // Default: 1 event per second
    };
    let delay_ms = (1000.0 / rate_hz) as u64;

    info!("Streaming rate: {:.1} Hz ({} ms between events)", rate_hz, delay_ms);
    info!("Press Ctrl+C to stop\n");

    // Stream events
    let mut gw_count = 0;
    let mut grb_count = 0;

    for (i, injection) in injections.iter().enumerate() {
        if i % 100 == 0 && i > 0 {
            info!("Streamed {} GW events, {} GRB events", gw_count, grb_count);
        }

        // Publish GW event
        let gw_event = GWEvent {
            simulation_id: injection.simulation_id,
            gpstime: injection.gpstime,
            pipeline: "SGNL".to_string(),
            snr: 10.0 + (injection.simulation_id % 20) as f32,  // Simulated SNR
            far: 1e-8,  // Simulated FAR
            skymap_path: format!("{}/allsky/{}.fits", base_path, injection.simulation_id),
        };

        let gw_payload = serde_json::to_string(&gw_event)?;
        let key = injection.simulation_id.to_string();
        let record = FutureRecord::to(gw_topic)
            .payload(&gw_payload)
            .key(&key);

        producer.send(record, Duration::from_secs(0)).await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send GW event: {}", e))?;

        gw_count += 1;

        // Publish corresponding GRB event (with time delay)
        if let Some(grb) = grb_params.iter().find(|g| g.simulation_id == injection.simulation_id) {
            // Simulate GRB detection with random time offset (0-10 seconds)
            let time_offset = (injection.simulation_id % 10) as f64;

            let grb_event = GRBEvent {
                simulation_id: injection.simulation_id,
                detection_time: injection.gpstime + time_offset,
                ra: grb.ra,
                dec: grb.dec,
                error_radius: grb.error_radius,
                instrument: grb.instrument.clone(),
                skymap_path: format!("simulated_grbs/O4HL/bgp/allsky/{}.fits", injection.simulation_id),
            };

            let grb_payload = serde_json::to_string(&grb_event)?;
            let key = injection.simulation_id.to_string();
            let record = FutureRecord::to(grb_topic)
                .payload(&grb_payload)
                .key(&key);

            producer.send(record, Duration::from_secs(0)).await
                .map_err(|(e, _)| anyhow::anyhow!("Failed to send GRB event: {}", e))?;

            grb_count += 1;
        }

        // Rate limiting
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    info!("\n✅ Streaming complete!");
    info!("Total GW events:  {}", gw_count);
    info!("Total GRB events: {}", grb_count);

    Ok(())
}

fn read_injection_params(path: &str) -> Result<Vec<InjectionParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut injections = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 9 {
            continue;
        }

        // Parse GPS time from longitude/latitude fields (assuming they're in radians)
        // Actually, let's use the geocent_time field
        let geocent_time: f64 = parts[3].parse()?;

        injections.push(InjectionParams {
            simulation_id: parts[0].parse()?,
            gpstime: geocent_time,
        });
    }

    Ok(injections)
}

fn read_grb_params(path: &str) -> Result<Vec<GrbParams>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut params = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 5 {
            continue;
        }

        params.push(GrbParams {
            simulation_id: parts[0].parse()?,
            ra: parts[1].parse()?,
            dec: parts[2].parse()?,
            error_radius: parts[3].parse()?,
            instrument: parts[4].to_string(),
        });
    }

    Ok(params)
}
