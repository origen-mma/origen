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
use mm_api::client::ApiClient;
use mm_core::ParsedSkymap;
use mm_simulation::{
    background_grbs::{generate_background_grbs, BackgroundGrbConfig},
    background_optical::{generate_background_optical, BackgroundOpticalConfig},
    calculate_joint_far, simulate_multimessenger_event, BinaryParams, FarAssociation,
    GrbSimulationConfig, GwEventParams, JointFarConfig, VOEventParser,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
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

    /// Simulate background transients (GRBs and optical)
    #[arg(long, default_value = "false")]
    simulate_background: bool,

    /// Background simulation time window (days)
    #[arg(long, default_value = "365.0")]
    background_duration_days: f64,

    /// API server URL for publishing events
    #[arg(long, default_value = "http://localhost:8080")]
    api_url: String,

    /// Enable API publishing
    #[arg(long, default_value = "false")]
    publish_to_api: bool,

    /// Path to GRB VOEvent XML directory (for realistic localizations)
    #[arg(long)]
    grb_xml_dir: Option<PathBuf>,

    /// Force at least one event to have all components (GW + GRB + optical)
    #[arg(long, default_value = "false")]
    force_multimessenger: bool,
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
    error_radius: f64, // degrees (90% containment)
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

#[derive(Debug, Default)]
struct BackgroundRejectionStats {
    // GRB background
    total_background_grbs: usize,
    grb_temporal_coincidences: usize,
    grb_spatial_coincidences: usize,

    // Optical background
    total_background_optical: usize,
    optical_temporal_coincidences: usize,
    optical_spatial_coincidences: usize,
    shock_cooling_spatial: usize,
    sne_ia_spatial: usize,
}

/// Realistic GRB localization from VOEvent XML
#[derive(Debug, Clone)]
struct GrbLocalizationTemplate {
    instrument: String,
    error_radius: f64, // degrees (90% containment)
    trigger_id: String,
}

/// Load GRB VOEvent XMLs and filter out 1.0° defaults
fn load_grb_localizations(grb_xml_dir: &PathBuf) -> Result<Vec<GrbLocalizationTemplate>> {
    let mut localizations = Vec::new();

    // Read all XML files
    let entries = std::fs::read_dir(grb_xml_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .xml files
        if path.extension().and_then(|s| s.to_str()) != Some("xml") {
            continue;
        }

        // Parse VOEvent
        let xml_content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let alert = match VOEventParser::parse_string(&xml_content) {
            Ok(a) => a,
            Err(_) => continue,
        };

        // Filter out 1.0° error radius (suspected default values)
        if (alert.error_radius - 1.0).abs() < 0.01 {
            continue;
        }

        // Also filter out unrealistically large (>30°) or small (<0.001°) error radii
        if alert.error_radius > 30.0 || alert.error_radius < 0.001 {
            continue;
        }

        localizations.push(GrbLocalizationTemplate {
            instrument: alert.instrument,
            error_radius: alert.error_radius,
            trigger_id: alert.trigger_id,
        });
    }

    if localizations.is_empty() {
        anyhow::bail!("No valid GRB localizations found");
    }

    Ok(localizations)
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

    // Initialize API client if enabled
    let api_client = if args.publish_to_api {
        let client = ApiClient::new(&args.api_url);
        match client.health_check().await {
            Ok(true) => {
                info!("✅ Connected to API server at {}", args.api_url);
                Some(client)
            }
            _ => {
                warn!(
                    "⚠️  API server at {} is not responding, continuing without API",
                    args.api_url
                );
                None
            }
        }
    } else {
        None
    };

    // Initialize RNG
    let mut rng = StdRng::seed_from_u64(args.seed);

    // Load GRB VOEvent XMLs for realistic localizations
    let grb_localizations = if let Some(ref grb_xml_dir) = args.grb_xml_dir {
        info!("Loading GRB VOEvent XMLs from {}...", grb_xml_dir.display());
        match load_grb_localizations(grb_xml_dir) {
            Ok(alerts) => {
                info!(
                    "✅ Loaded {} GRB localizations (filtered out 1.0° defaults)",
                    alerts.len()
                );
                Some(alerts)
            }
            Err(e) => {
                warn!(
                    "⚠️  Failed to load GRB XMLs: {}, using default localizations",
                    e
                );
                None
            }
        }
    } else {
        info!("No GRB XML directory specified, using default localizations");
        None
    };

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

    // Generate background transients if enabled
    let background_grbs = if args.simulate_background {
        info!("🎲 Generating background GRBs...");
        let o4_start_gps = 1369094418.0;
        let o4_duration = args.background_duration_days * 86400.0;
        let o4_end_gps = o4_start_gps + o4_duration;

        let bg_config = BackgroundGrbConfig::combined(); // Swift + Fermi
        let grbs = generate_background_grbs(&bg_config, o4_start_gps, o4_end_gps, &mut rng);
        info!("  Generated {} background GRBs", grbs.len());
        grbs
    } else {
        Vec::new()
    };

    let background_optical = if args.simulate_background {
        info!("🎲 Generating background optical transients...");
        let o4_start_gps = 1369094418.0;
        let o4_duration = args.background_duration_days * 86400.0;
        let o4_end_gps = o4_start_gps + o4_duration;

        let optical_config = BackgroundOpticalConfig::ztf(); // ZTF survey
        let transients =
            generate_background_optical(&optical_config, o4_start_gps, o4_end_gps, &mut rng);
        info!(
            "  Generated {} background optical transients",
            transients.len()
        );
        transients
    } else {
        Vec::new()
    };

    // Statistics
    let mut n_events = 0;
    let mut n_gw_published = 0;
    let mut n_grb_published = 0;
    let mut n_optical_published = 0;
    let mut n_correlations_published = 0;
    let mut bg_stats = BackgroundRejectionStats::default();
    let mut has_full_multimessenger = false; // Track if we've had GW+GRB+optical

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

        // Try to load skymap FITS file (n_events starts at 1, but skymap files start at 0)
        let skymap_path = args
            .bgp_path
            .join("allsky")
            .join(format!("{}.fits", n_events - 1));

        // Extract position from skymap (used for both API and later GRB/optical detections)
        let (ra, dec) = if skymap_path.exists() {
            match ParsedSkymap::from_fits(&skymap_path) {
                Ok(skymap) => (skymap.max_prob_position.ra, skymap.max_prob_position.dec),
                Err(e) => {
                    warn!(
                        "Failed to parse skymap for position {}: {}",
                        skymap_path.display(),
                        e
                    );
                    // Fallback to dummy position
                    (
                        (n_events as f64 * 37.5) % 360.0,
                        ((n_events as f64 * 23.1) % 180.0) - 90.0,
                    )
                }
            }
        } else {
            // Fallback to dummy position if no skymap
            (
                (n_events as f64 * 37.5) % 360.0,
                ((n_events as f64 * 23.1) % 180.0) - 90.0,
            )
        };

        // Publish to API if enabled
        if let Some(ref client) = api_client {
            let event_id = format!("G{}", n_events);

            let skymap_data = if skymap_path.exists() {
                match std::fs::read(&skymap_path) {
                    Ok(data) => {
                        info!(
                            "📊 Loaded skymap for event {}: {} bytes",
                            event_id,
                            data.len()
                        );
                        Some(data)
                    }
                    Err(e) => {
                        warn!("Failed to read skymap {}: {}", skymap_path.display(), e);
                        None
                    }
                }
            } else {
                None
            };

            if let Err(e) = client
                .publish_gw_event(
                    &event_id,
                    gpstime,
                    ra,
                    dec,
                    gw_snr as f64,
                    gw_far_per_year,
                    skymap_data,
                )
                .await
            {
                warn!("Failed to publish event {} to API: {}", event_id, e);
            }
        }

        info!(
            "📡 GW {} published: GPS={:.2}, SNR={:.1}, Distance={:.0} Mpc",
            n_events, gpstime, gw_snr, distance
        );

        // Background rejection analysis (if enabled)
        if args.simulate_background && !background_grbs.is_empty() {
            bg_stats.total_background_grbs = background_grbs.len();
            bg_stats.total_background_optical = background_optical.len();

            // GRB temporal window: ±5 seconds
            let grb_time_window = 5.0;
            let grb_temporal: Vec<_> = background_grbs
                .iter()
                .filter(|grb| (grb.gps_time - gpstime).abs() <= grb_time_window)
                .collect();

            bg_stats.grb_temporal_coincidences += grb_temporal.len();

            // GRB spatial window: simplified 100 sq deg skymap
            let skymap_area_90 = (distance / 100.0).powi(2) * 100.0;
            let full_sky = 41253.0;
            let spatial_prob = skymap_area_90 / full_sky;

            for _ in &grb_temporal {
                // Probabilistic spatial check (simplified)
                if rand::random::<f64>() < spatial_prob {
                    bg_stats.grb_spatial_coincidences += 1;
                }
            }

            // Optical temporal window: 14 days
            let optical_time_window = 14.0 * 86400.0;
            let optical_temporal: Vec<_> = background_optical
                .iter()
                .filter(|opt| {
                    let dt = opt.discovery_gps_time - gpstime;
                    dt >= 0.0 && dt <= optical_time_window
                })
                .collect();

            bg_stats.optical_temporal_coincidences += optical_temporal.len();

            // Optical spatial window: check if in GW skymap
            use mm_simulation::background_optical::OpticalTransientType;
            for opt in &optical_temporal {
                if rand::random::<f64>() < spatial_prob {
                    bg_stats.optical_spatial_coincidences += 1;

                    match opt.transient_type {
                        OpticalTransientType::ShockCooling => {
                            bg_stats.shock_cooling_spatial += 1;
                        }
                        OpticalTransientType::TypeIaSN => {
                            bg_stats.sne_ia_spatial += 1;
                        }
                    }
                }
            }
        }

        // Check if we should force multi-messenger for this event
        // (last event and we haven't had a full multi-messenger yet)
        let is_last_event = args.max_events > 0 && n_events >= args.max_events;
        let force_mm_this_event =
            args.force_multimessenger && !has_full_multimessenger && is_last_event;

        if force_mm_this_event {
            info!("🎯 FORCING MULTI-MESSENGER EVENT (GW+GRB+Optical) for demonstration");
        }

        // 2. Publish GRB if detected (or forced)
        let has_grb = mm_event.has_grb() || force_mm_this_event;
        if has_grb {
            let time_offset = 0.5; // ~0.5s after GW

            // Sample realistic GRB localization and generate position with error
            use mm_simulation::{add_localization_error, GrbInstrument};

            let (grb_instrument_name, grb_inst_enum) = if let Some(ref locs) = grb_localizations {
                let idx = rng.gen_range(0..locs.len());
                let loc = &locs[idx];
                let inst_str = &loc.instrument;

                // Parse instrument string to enum
                let inst_enum = if inst_str.contains("Swift") {
                    GrbInstrument::SwiftBAT
                } else if inst_str.contains("Fermi") {
                    GrbInstrument::FermiGBM
                } else if inst_str.contains("Einstein") {
                    GrbInstrument::EinsteinProbeWXT
                } else {
                    GrbInstrument::FermiGBM // default
                };

                (inst_str.clone(), inst_enum)
            } else {
                ("Fermi GBM".to_string(), GrbInstrument::FermiGBM)
            };

            // Generate realistic GRB position with localization error
            // Use true GW position (ra, dec) and add instrument-specific error
            let grb_loc = add_localization_error(ra, dec, grb_inst_enum, &mut rng);
            let (grb_error_radius, grb_ra, grb_dec) =
                (grb_loc.error_radius, grb_loc.obs_ra, grb_loc.obs_dec);

            let grb_alert = GrbAlert {
                simulation_id: n_events,
                detection_time: gpstime + time_offset,
                instrument: grb_instrument_name.clone(),
                fluence: 1e-6,
                time_offset,
                on_axis: true, // If GRB detected, assume on-axis
                error_radius: grb_error_radius,
            };

            publish_json(
                &producer,
                "gcn.notices.grb",
                &n_events.to_string(),
                &grb_alert,
            )
            .await?;
            n_grb_published += 1;

            // Publish GRB to API
            if let Some(ref client) = api_client {
                let event_id = format!("G{}", n_events);
                // GRB uses observed position with realistic localization error

                if let Err(e) = client
                    .add_grb_detection(
                        &event_id,
                        gpstime + time_offset,
                        grb_ra,  // Use GRB observed position with error
                        grb_dec, // Use GRB observed position with error
                        &grb_alert.instrument,
                        grb_alert.fluence,
                        grb_alert.error_radius,
                    )
                    .await
                {
                    warn!("Failed to add GRB detection to API: {}", e);
                }
            }

            info!("   🌟 GRB detected! Δt={:.2}s", time_offset);
        }

        // 3. Publish optical alert if detectable (or forced)
        let has_optical =
            mm_event.has_afterglow() || mm_event.has_kilonova() || force_mm_this_event;
        let optical_magnitude =
            if force_mm_this_event && mm_event.afterglow.peak_magnitude.is_none() {
                // Force a bright optical counterpart
                Some(20.0)
            } else {
                mm_event.afterglow.peak_magnitude
            };

        if has_optical {
            if let Some(mag) = optical_magnitude {
                if mag < args.limiting_magnitude || force_mm_this_event {
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

                    // Publish optical to API
                    if let Some(ref client) = api_client {
                        let event_id = format!("G{}", n_events);
                        // Optical transient should be at same position as GW event
                        // (kilonova/afterglow from same source)
                        // Use ra/dec from skymap extraction above

                        if let Err(e) = client
                            .add_optical_detection(
                                &event_id,
                                gpstime + time_offset,
                                ra, // Use skymap position
                                dec,
                                mag,
                                &optical_alert.survey,
                                &optical_alert.source_type,
                            )
                            .await
                        {
                            warn!("Failed to add optical detection to API: {}", e);
                        }
                    }

                    info!(
                        "   🔭 Optical detected! mag={:.1}, type={}",
                        mag, optical_alert.source_type
                    );
                }
            }
        }

        // 4. Calculate and publish joint FAR if multi-messenger
        if has_grb || (has_optical && optical_magnitude.is_some()) {
            // Use has_grb from earlier (includes forced MM)
            let has_optical_detectable = has_optical
                && (optical_magnitude.is_some_and(|m| m < args.limiting_magnitude)
                    || force_mm_this_event);

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
                has_full_multimessenger = true;
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

    // Background rejection statistics
    if args.simulate_background {
        info!("");
        info!("╔══════════════════════════════════════════════════════════════╗");
        info!("║              Background Rejection Analysis                   ║");
        info!("╚══════════════════════════════════════════════════════════════╝");
        info!("");
        info!("Background GRBs:");
        info!(
            "  Total generated:           {}",
            bg_stats.total_background_grbs
        );
        info!(
            "  Temporal coincidences:     {} ({:.2}%)",
            bg_stats.grb_temporal_coincidences,
            100.0 * bg_stats.grb_temporal_coincidences as f64
                / bg_stats.total_background_grbs.max(1) as f64
        );
        info!(
            "  Spatial+temporal:          {} ({:.2}%)",
            bg_stats.grb_spatial_coincidences,
            100.0 * bg_stats.grb_spatial_coincidences as f64
                / bg_stats.total_background_grbs.max(1) as f64
        );

        let grb_temporal_rejection = 1.0
            - (bg_stats.grb_temporal_coincidences as f64
                / bg_stats.total_background_grbs.max(1) as f64);
        let grb_total_rejection = 1.0
            - (bg_stats.grb_spatial_coincidences as f64
                / bg_stats.total_background_grbs.max(1) as f64);

        info!(
            "  Temporal rejection:        {:.2}%",
            100.0 * grb_temporal_rejection
        );
        info!(
            "  Total rejection:           {:.4}%",
            100.0 * grb_total_rejection
        );

        info!("");
        info!("Background Optical Transients:");
        info!(
            "  Total generated:           {}",
            bg_stats.total_background_optical
        );
        info!(
            "  Temporal coincidences:     {} ({:.2}%)",
            bg_stats.optical_temporal_coincidences,
            100.0 * bg_stats.optical_temporal_coincidences as f64
                / bg_stats.total_background_optical.max(1) as f64
        );
        info!(
            "  Spatial+temporal:          {} ({:.2}%)",
            bg_stats.optical_spatial_coincidences,
            100.0 * bg_stats.optical_spatial_coincidences as f64
                / bg_stats.total_background_optical.max(1) as f64
        );
        info!(
            "    └─ Shock cooling:        {}",
            bg_stats.shock_cooling_spatial
        );
        info!("    └─ SNe Ia:               {}", bg_stats.sne_ia_spatial);

        let optical_temporal_rejection = 1.0
            - (bg_stats.optical_temporal_coincidences as f64
                / bg_stats.total_background_optical.max(1) as f64);
        let optical_total_rejection = 1.0
            - (bg_stats.optical_spatial_coincidences as f64
                / bg_stats.total_background_optical.max(1) as f64);

        info!(
            "  Temporal rejection:        {:.2}%",
            100.0 * optical_temporal_rejection
        );
        info!(
            "  Total rejection:           {:.4}%",
            100.0 * optical_total_rejection
        );

        info!("");
        info!("Conclusion:");
        info!(
            "  ✅ Temporal cuts reject {:.2}% of background GRBs",
            100.0 * grb_temporal_rejection
        );
        info!(
            "  ✅ Spatial cuts further improve to {:.4}% rejection",
            100.0 * grb_total_rejection
        );
        info!(
            "  ✅ Temporal cuts reject {:.2}% of background optical",
            100.0 * optical_temporal_rejection
        );
        info!(
            "  ✅ Spatial cuts further improve to {:.4}% rejection",
            100.0 * optical_total_rejection
        );
        info!("  🎯 Time + spatial cuts are EXTREMELY effective!");
    }

    Ok(())
}
