///! Stream O4 multi-messenger events to Kafka in real-time
///!
///! This demonstrates the full simulation pipeline:
///! - Reads O4 GW events from injections.dat
///! - Simulates GRB emission (beaming, jet structure)
///! - Simulates optical afterglows and kilonovae (magnitude-based)
///! - Publishes to Kafka topics for real-time correlation
///! - Calculates joint FARs for multi-messenger associations
///!
///! Usage:
///! ```bash
///! cargo run --release --bin stream-o4-simulation -- \
///!     /path/to/O4HL/bgp \
///!     --rate 1.0 \
///!     --max-events 100
///! ```
use anyhow::Result;
use clap::Parser;
use mm_simulation::{
    calculate_joint_far, simulate_multimessenger_event, BinaryParams, FarAssociation,
    GrbSimulationConfig, GwEventParams, JointFarConfig,
};
use rand::{rngs::StdRng, SeedableRng};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "stream-o4-simulation")]
#[command(about = "Stream O4 multi-messenger simulations to Kafka")]
struct Args {
    /// Path to O4HL bgp directory
    bgp_path: PathBuf,

    /// Events per second
    #[arg(long, default_value = "1.0")]
    rate: f64,

    /// Maximum events to process (0 = all)
    #[arg(long, default_value = "0")]
    max_events: usize,

    /// Kafka bootstrap servers
    #[arg(long, default_value = "localhost:9092")]
    kafka_brokers: String,

    /// Random seed
    #[arg(long, default_value = "42")]
    seed: u64,

    /// Survey limiting magnitude
    #[arg(long, default_value = "24.5")]
    limiting_magnitude: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct GwAlert {
    simulation_id: usize,
    gpstime: f64,
    pipeline: String,
    snr: f64,
    far: f64,
    distance: f64,
    mass1: f64,
    mass2: f64,
    has_em_counterpart: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct GrbAlert {
    simulation_id: usize,
    detection_time: f64,
    instrument: String,
    fluence: f64,
    time_offset: f64,
    on_axis: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpticalAlert {
    simulation_id: usize,
    detection_time: f64,
    survey: String,
    magnitude: f64,
    mag_error: f64,
    time_offset: f64,
    source_type: String, // "afterglow" or "kilonova"
}

#[derive(Debug, Serialize, Deserialize)]
struct MultiMessengerCorrelation {
    simulation_id: usize,
    gw_snr: f64,
    has_grb: bool,
    has_optical: bool,
    optical_magnitude: Option<f64>,
    joint_far_per_year: f64,
    significance_sigma: f64,
    pastro: f64,
}

async fn create_producer(brokers: &str) -> Result<FutureProducer> {
    let producer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("message.timeout.ms", "5000")
        .create()?;
    Ok(producer)
}

async fn publish_json<T: Serialize>(
    producer: &FutureProducer,
    topic: &str,
    key: &str,
    payload: &T,
) -> Result<()> {
    let json = serde_json::to_string(payload)?;
    let record = FutureRecord::to(topic).key(key).payload(&json);

    producer
        .send(record, Duration::from_secs(0))
        .await
        .map_err(|(err, _)| anyhow::anyhow!("Kafka send error: {}", err))?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║      O4 Multi-Messenger Simulation Kafka Stream             ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");
    info!("Kafka brokers: {}", args.kafka_brokers);
    info!("Event rate: {} Hz", args.rate);
    info!("Limiting magnitude: {} mag (LSST)", args.limiting_magnitude);
    info!("");

    // Initialize Kafka producer
    let producer = create_producer(&args.kafka_brokers).await?;
    info!("✅ Connected to Kafka");

    // Initialize RNG
    let mut rng = StdRng::seed_from_u64(args.seed);

    // Read injections file
    let injections_file = args.bgp_path.join("injections.dat");
    info!("📖 Reading O4 injections from: {:?}", injections_file);

    let file = File::open(&injections_file)?;
    let reader = BufReader::new(file);

    // Simulation configs
    let grb_config = GrbSimulationConfig::default();
    let far_config = JointFarConfig {
        gw_observing_time: 1.0,
        grb_rate_per_year: 300.0,
        optical_rate_per_sqdeg_per_year: 0.1, // Rare transients like kilonovae/afterglows
        optical_time_window_days: 14.0,
        grb_time_window_seconds: 10.0,
    };

    // Statistics
    let mut n_events = 0;
    let mut n_gw_published = 0;
    let mut n_grb_published = 0;
    let mut n_optical_published = 0;
    let mut n_correlations_published = 0;

    let start_time = Instant::now();
    let event_interval = Duration::from_secs_f64(1.0 / args.rate);

    info!("🚀 Starting event stream...");
    info!("");

    for (line_num, line) in reader.lines().enumerate() {
        // Skip header
        if line_num == 0 {
            continue;
        }

        // Check max events
        if args.max_events > 0 && n_events >= args.max_events {
            break;
        }

        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() < 9 {
            warn!("Line {} has insufficient columns, skipping", line_num + 1);
            continue;
        }

        // Parse O4 injection
        let mass1: f64 = parts[5].parse()?;
        let mass2: f64 = parts[6].parse()?;
        let distance: f64 = parts[4].parse()?;
        let inclination: f64 = parts[3].parse()?;
        let spin1z: f64 = parts[7].parse()?;
        let spin2z: f64 = parts[8].parse()?;

        // Skip BBH events
        if mass1 > 3.0 && mass2 > 3.0 {
            continue;
        }

        let binary_params = BinaryParams {
            mass_1_source: mass1,
            mass_2_source: mass2,
            radius_1: 12.0,
            radius_2: 12.0,
            chi_1: spin1z,
            chi_2: spin2z,
            tov_mass: 2.17,
            r_16: 12.0,
            ratio_zeta: 0.2,
            alpha: 1.0,
            ratio_epsilon: 0.1,
        };

        let gw_params = GwEventParams {
            inclination,
            distance,
            z: distance / 4500.0,
        };

        // Simulate multi-messenger event
        let mm_event =
            simulate_multimessenger_event(&binary_params, &gw_params, &grb_config, &mut rng);

        n_events += 1;

        // Simulated GPS time (sequential for demo)
        let gpstime = 1400000000.0 + (n_events as f64) * 3600.0;

        // Simplified GW SNR
        let gw_snr = 8.0 + (1.0 / distance * 100.0);
        // Use realistic FAR for significant GW detections
        let gw_far_per_year = 1e-8;

        // 1. Publish GW alert
        let gw_alert = GwAlert {
            simulation_id: n_events,
            gpstime,
            pipeline: "SGNL".to_string(),
            snr: gw_snr,
            far: gw_far_per_year,
            distance,
            mass1,
            mass2,
            has_em_counterpart: mm_event.has_grb()
                || mm_event.has_afterglow()
                || mm_event.has_kilonova(),
        };

        publish_json(&producer, "igwn.gwalert", &n_events.to_string(), &gw_alert).await?;
        n_gw_published += 1;

        info!(
            "📡 GW {} published: GPS={:.2}, SNR={:.1}, Distance={:.0} Mpc",
            n_events, gpstime, gw_snr, distance
        );

        // 2. Publish GRB if detected
        if mm_event.has_grb() {
            let time_offset = 0.5; // ~0.5s after GW
            let grb_alert = GrbAlert {
                simulation_id: n_events,
                detection_time: gpstime + time_offset,
                instrument: "Fermi GBM".to_string(),
                fluence: 1e-6,
                time_offset,
                on_axis: true, // If GRB detected, assume on-axis
            };

            publish_json(
                &producer,
                "gcn.notices.grb",
                &n_events.to_string(),
                &grb_alert,
            )
            .await?;
            n_grb_published += 1;

            info!("   🌟 GRB detected! Δt={:.2}s", time_offset);
        }

        // 3. Publish optical alert if detectable
        let has_optical = mm_event.has_afterglow() || mm_event.has_kilonova();
        let optical_magnitude = mm_event.afterglow.peak_magnitude;

        if has_optical {
            if let Some(mag) = optical_magnitude {
                if mag < args.limiting_magnitude {
                    let time_offset = 3600.0; // 1 hour after GW
                    let optical_alert = OpticalAlert {
                        simulation_id: n_events,
                        detection_time: gpstime + time_offset,
                        survey: "LSST".to_string(),
                        magnitude: mag,
                        mag_error: 0.1,
                        time_offset,
                        source_type: if mm_event.has_afterglow() {
                            "afterglow".to_string()
                        } else {
                            "kilonova".to_string()
                        },
                    };

                    publish_json(
                        &producer,
                        "optical.alerts",
                        &n_events.to_string(),
                        &optical_alert,
                    )
                    .await?;
                    n_optical_published += 1;

                    info!(
                        "   🔭 Optical detected! mag={:.1}, type={}",
                        mag, optical_alert.source_type
                    );
                }
            }
        }

        // 4. Calculate and publish joint FAR if multi-messenger
        if mm_event.has_grb() || (has_optical && optical_magnitude.is_some()) {
            let has_grb = mm_event.has_grb();
            let has_optical_detectable =
                has_optical && optical_magnitude.is_some_and(|m| m < args.limiting_magnitude);

            let skymap_area_90 = (distance / 100.0).powi(2) * 100.0; // Simplified

            let far_assoc = FarAssociation {
                gw_snr,
                gw_far_per_year,
                skymap_area_90,
                has_grb,
                grb_fluence: if has_grb { Some(1e-6) } else { None },
                grb_time_offset: if has_grb { Some(0.5) } else { None },
                has_optical: has_optical_detectable,
                optical_magnitude,
                optical_time_offset: if has_optical_detectable {
                    Some(3600.0)
                } else {
                    None
                },
            };

            let far_result = calculate_joint_far(&far_assoc, &far_config);
            let pastro = 1.0 / (1.0 + far_result.far_per_year * far_config.gw_observing_time);

            // Cap infinity/NaN values for JSON serialization
            let significance_sigma = if far_result.significance_sigma.is_infinite()
                || far_result.significance_sigma.is_nan()
            {
                1000.0 // Use a very large but finite value
            } else {
                far_result.significance_sigma
            };

            let correlation = MultiMessengerCorrelation {
                simulation_id: n_events,
                gw_snr,
                has_grb,
                has_optical: has_optical_detectable,
                optical_magnitude,
                joint_far_per_year: far_result.far_per_year,
                significance_sigma,
                pastro,
            };

            publish_json(
                &producer,
                "mm.correlations",
                &n_events.to_string(),
                &correlation,
            )
            .await?;
            n_correlations_published += 1;

            // Always show multi-messenger correlations
            let emoji = if far_result.significance_sigma > 5.0 {
                "🎯"
            } else if far_result.significance_sigma > 3.0 {
                "✨"
            } else {
                "💫"
            };

            info!(
                "   {} MMA CORRELATION: GW+{}{}",
                emoji,
                if has_grb { "GRB" } else { "" },
                if has_optical_detectable {
                    "+Optical"
                } else {
                    ""
                }
            );
            info!(
                "      FAR={:.2e}/yr, σ={:.2}, P_astro={:.1}%",
                far_result.far_per_year,
                far_result.significance_sigma,
                100.0 * pastro
            );
            if has_grb && has_optical_detectable {
                info!("      THREE-WAY CORRELATION (GW+GRB+Optical)");
            }
        }

        info!("");

        // Rate limiting
        sleep(event_interval).await;
    }

    let elapsed = start_time.elapsed();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║                        Summary                               ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");
    info!("Total events processed: {}", n_events);
    info!("  GW alerts published:       {}", n_gw_published);
    info!("  GRB alerts published:      {}", n_grb_published);
    info!("  Optical alerts published:  {}", n_optical_published);
    info!("  Correlations published:    {}", n_correlations_published);
    info!("");
    info!("Elapsed time: {:.1}s", elapsed.as_secs_f64());
    info!(
        "Actual rate: {:.2} Hz",
        n_events as f64 / elapsed.as_secs_f64()
    );

    Ok(())
}
