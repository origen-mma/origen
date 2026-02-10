use anyhow::Result;
use clap::Parser;
use mm_boom::parse_boom_alert;
use mm_config::Config;
use mm_core::lightcurve_fitting::gps_to_mjd;
use mm_core::{io::load_lightcurves_dir, LightCurve, MockSkymap, Photometry, SkyPosition};
use mm_correlator::{CorrelatorConfig, SupereventCorrelator};
use mm_gcn::AlertRouter;
use rand::SeedableRng;
use rdkafka::{
    client::{ClientContext, OAuthToken},
    config::RDKafkaLogLevel,
    consumer::{Consumer, ConsumerContext, StreamConsumer},
    ClientConfig, Message,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Custom Kafka context that handles OAUTHBEARER token refresh for GCN.
///
/// librdkafka's built-in OIDC token fetching is broken on macOS, so we
/// manually fetch the OAuth token via curl and provide it through the callback.
struct GcnContext {
    client_id: String,
    client_secret: String,
}

impl ClientContext for GcnContext {
    const ENABLE_REFRESH_OAUTH_TOKEN: bool = true;

    fn generate_oauth_token(
        &self,
        _oauthbearer_config: Option<&str>,
    ) -> Result<OAuthToken, Box<dyn Error>> {
        info!("Fetching OAuth token from auth.gcn.nasa.gov...");
        let output = std::process::Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                "https://auth.gcn.nasa.gov/oauth2/token",
                "-H",
                "Content-Type: application/x-www-form-urlencoded",
                "-d",
                &format!(
                    "grant_type=client_credentials&client_id={}&client_secret={}&scope={}",
                    self.client_id, self.client_secret, "gcn.nasa.gov/kafka-public-consumer"
                ),
            ])
            .output()
            .map_err(|e| format!("Failed to run curl: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("curl failed: {stderr}").into());
        }

        let body = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from token endpoint: {e}"))?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: u64,
        }

        let resp: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse token response: {e} (body: {body})"))?;

        let lifetime_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + (resp.expires_in as i64 * 1000);

        info!("OAuth token obtained (expires in {}s)", resp.expires_in);
        Ok(OAuthToken {
            token: resp.access_token,
            principal_name: "origin".to_string(),
            lifetime_ms,
        })
    }
}

impl ConsumerContext for GcnContext {}

#[derive(Parser)]
#[command(
    name = "gcn-correlator",
    about = "GW+GRB correlator using real GCN Kafka streams"
)]
struct Cli {
    /// Start consuming from earliest available offset (historical replay)
    #[arg(short = 'b', long)]
    from_beginning: bool,

    /// Stop after consuming for N seconds (default: run forever)
    #[arg(short, long)]
    duration: Option<u64>,

    /// Output JSON file for correlation results
    #[arg(short, long)]
    output: Option<String>,

    /// Extra logging
    #[arg(short, long)]
    verbose: bool,

    /// Path to config file
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    /// List available topics from the broker and exit
    #[arg(long)]
    list_topics: bool,

    /// Also consume BOOM optical transient alerts from kaboom.caltech.edu
    #[arg(long)]
    boom: bool,

    /// Use simulated optical light curves from CSV directory instead of BOOM
    /// (loads ZTF CSV files and time-shifts them to match GW events)
    #[arg(long)]
    simulate: bool,

    /// Maximum number of simulated light curves to inject per GW event (default: all)
    #[arg(long)]
    max_sim: Option<usize>,
}

#[derive(Serialize)]
struct RunOutput {
    run_config: RunConfig,
    stats: RunStats,
    correlations: Vec<CorrelationRecord>,
    optical_correlations: Vec<OpticalCorrelationRecord>,
}

#[derive(Serialize)]
struct RunConfig {
    from_beginning: bool,
    duration_s: Option<u64>,
    topics: Vec<String>,
}

#[derive(Serialize)]
struct RunStats {
    gw_events: usize,
    grb_events: usize,
    optical_events: usize,
    other_events: usize,
    parse_errors: usize,
    correlations: usize,
    optical_correlations: usize,
    runtime_s: f64,
}

#[derive(Serialize, Clone)]
struct CorrelationRecord {
    superevent_id: String,
    gw_superevent_id: String,
    gw_gps_time: f64,
    grb_trigger_id: String,
    grb_instrument: String,
    time_offset_s: f64,
    spatial_offset_deg: Option<f64>,
    skymap_probability: Option<f64>,
    in_90cr: Option<bool>,
}

#[derive(Serialize, Clone)]
struct OpticalCorrelationRecord {
    superevent_id: String,
    gw_superevent_id: String,
    object_id: String,
    time_offset_s: f64,
    spatial_offset_deg: Option<f64>,
    skymap_probability: Option<f64>,
    in_90cr: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        "gcn_correlator=debug,mm_gcn=debug,mm_correlator=debug,mm_core=info"
    } else {
        "gcn_correlator=info,mm_gcn=info,mm_correlator=warn"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("=== GCN Kafka GW+GRB Correlator ===");
    info!(
        "Mode: {}",
        if cli.from_beginning {
            "historical replay (from beginning)"
        } else {
            "live (latest only)"
        }
    );
    if let Some(dur) = cli.duration {
        info!("Duration: {} seconds", dur);
    }

    // Load configuration
    let config_path = env::var("MM_CONFIG_PATH").unwrap_or_else(|_| cli.config.clone());
    let app_config = match Config::from_file_with_env(&config_path) {
        Ok(cfg) => {
            info!("Configuration loaded from: {}", config_path);
            cfg
        }
        Err(e) => {
            warn!("Failed to load config ({}), using development defaults", e);
            Config::development()
        }
    };

    // GW+GRB topics (plus heartbeat for connectivity verification)
    // New JSON format: gcn.notices.fermi.gbm.alert, gcn.notices.swift.bat.guano
    // Classic VOEvent XML format: gcn.classic.voevent.FERMI_GBM_* (more historical data)
    let topics = vec![
        "gcn.heartbeat".to_string(),
        "igwn.gwalert".to_string(),
        "gcn.notices.fermi.gbm.alert".to_string(),
        "gcn.notices.swift.bat.guano".to_string(),
        // Classic VOEvent topics for Fermi GBM (flt/gnd/fin positions)
        "gcn.classic.voevent.FERMI_GBM_FLT_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_GND_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_FIN_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_SUBTHRESH".to_string(),
    ];

    // Configure GCN Kafka consumer with custom OAuth token callback.
    // We bypass gcn-kafka's set_gcn_auth() because librdkafka's built-in
    // OAUTHBEARER OIDC token fetching is broken on macOS.
    let context = GcnContext {
        client_id: app_config.gcn.client_id.clone(),
        client_secret: app_config.gcn.client_secret.clone(),
    };

    let mut config = ClientConfig::new();
    config.set("bootstrap.servers", "kafka.gcn.nasa.gov");
    config.set("security.protocol", "sasl_ssl");
    config.set("sasl.mechanisms", "OAUTHBEARER");
    // Use fresh group ID each run for historical replay (avoids stale offsets)
    let group_id = if cli.from_beginning {
        format!(
            "origin-replay-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        )
    } else {
        "origin-live".to_string()
    };
    config.set("group.id", &group_id);
    config.set("session.timeout.ms", "45000");
    config.set("enable.auto.commit", "false");
    config.set("auto.offset.reset", "earliest");
    if cli.verbose {
        config.set("debug", "broker,security");
    }
    info!("Consumer group: {}", group_id);

    info!(
        "Connecting to GCN Kafka (client_id={}...)",
        &app_config.gcn.client_id[..8.min(app_config.gcn.client_id.len())]
    );

    config.set_log_level(if cli.verbose {
        RDKafkaLogLevel::Debug
    } else {
        RDKafkaLogLevel::Warning
    });
    let consumer: StreamConsumer<GcnContext> = config.create_with_context(context)?;

    // --list-topics: subscribe to heartbeat to trigger OAuth, then list all topics
    if cli.list_topics {
        consumer.subscribe(&["gcn.heartbeat"])?;
        info!("Connecting (waiting for heartbeat to confirm auth)...");
        // Consume one message to ensure OAuth handshake completes
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            tokio::select! {
                msg = consumer.recv() => {
                    match msg {
                        Ok(_) => {
                            info!("Connected! Fetching topic list...");
                            break;
                        }
                        Err(_) => continue,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    error!("Timed out waiting for connection");
                    return Ok(());
                }
            }
        }
        match consumer.fetch_metadata(None, Duration::from_secs(15)) {
            Ok(metadata) => {
                let mut topic_names: Vec<&str> =
                    metadata.topics().iter().map(|t| t.name()).collect();
                topic_names.sort();
                info!("Available topics ({}):", topic_names.len());
                for name in &topic_names {
                    info!("  {}", name);
                }
                // Also filter for Fermi/Swift/GRB related
                let grb_topics: Vec<&&str> = topic_names
                    .iter()
                    .filter(|n| {
                        n.contains("fermi")
                            || n.contains("gbm")
                            || n.contains("swift")
                            || n.contains("bat")
                            || n.contains("grb")
                    })
                    .collect();
                if !grb_topics.is_empty() {
                    info!("GRB-related topics:");
                    for name in grb_topics {
                        info!("  {}", name);
                    }
                }
            }
            Err(e) => {
                error!("Failed to fetch metadata: {}", e);
            }
        }
        return Ok(());
    }

    let topic_refs: Vec<&str> = topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topic_refs)?;

    info!("Subscribed to {} GCN topics:", topics.len());
    for t in &topics {
        info!("  - {}", t);
    }
    info!("Waiting for alerts (first heartbeat confirms connectivity)...");

    // Validate --boom and --simulate are mutually exclusive
    if cli.boom && cli.simulate {
        anyhow::bail!("Cannot use --boom and --simulate together. Choose one optical source.");
    }

    // Load simulated light curves if --simulate is set
    let simulated_lightcurves: Vec<LightCurve> = if cli.simulate {
        let csv_dir = &app_config.simulation.ztf_csv_dir;
        info!("=== Simulation Mode ===");
        info!("Loading ZTF light curves from: {}", csv_dir);

        let lcs = load_lightcurves_dir(csv_dir)
            .map_err(|e| anyhow::anyhow!("Failed to load light curves from {}: {}", csv_dir, e))?;

        info!(
            "Loaded {} simulated light curves for injection after GW events",
            lcs.len()
        );
        lcs
    } else {
        Vec::new()
    };

    // Set up BOOM optical transient consumer if requested
    let (boom_tx, mut boom_rx) = mpsc::channel::<(LightCurve, SkyPosition, String)>(1000);

    if cli.boom {
        info!("=== BOOM Optical Transient Consumer ===");

        // BOOM broker ACLs require group_id to be prefixed with the SASL username
        let boom_group_id = format!("{}-gcn-correlator", app_config.boom.sasl_username);

        let boom_topics = app_config.boom.topics.clone();
        info!(
            "BOOM broker: {} (group: {})",
            app_config.boom.bootstrap_servers, boom_group_id
        );

        let boom_consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &app_config.boom.bootstrap_servers)
            .set("security.protocol", "SASL_PLAINTEXT")
            .set("sasl.mechanisms", "SCRAM-SHA-512")
            .set("sasl.username", &app_config.boom.sasl_username)
            .set("sasl.password", &app_config.boom.sasl_password)
            .set("group.id", &boom_group_id)
            .set("enable.auto.commit", "false")
            .set(
                "auto.offset.reset",
                if cli.from_beginning {
                    "earliest"
                } else {
                    "latest"
                },
            )
            .set("session.timeout.ms", "45000")
            .set_log_level(if cli.verbose {
                RDKafkaLogLevel::Debug
            } else {
                RDKafkaLogLevel::Warning
            })
            .create()?;

        let boom_topic_refs: Vec<&str> = boom_topics.iter().map(|s| s.as_str()).collect();
        boom_consumer.subscribe(&boom_topic_refs)?;

        info!("Subscribed to {} BOOM topics:", boom_topics.len());
        for t in &boom_topics {
            info!("  - {}", t);
        }

        // Spawn BOOM consumer task
        let verbose = cli.verbose;
        tokio::spawn(async move {
            let mut boom_count: usize = 0;
            let mut boom_errors: usize = 0;
            loop {
                match boom_consumer.recv().await {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            // Parse in a block so the non-Send Result doesn't span the await
                            let parsed = parse_boom_alert(payload)
                                .map(|alert| {
                                    let object_id = alert.object_id.clone();
                                    let lc = alert.to_lightcurve();
                                    let pos = alert.position();
                                    (lc, pos, object_id)
                                })
                                .map_err(|e| e.to_string());

                            match parsed {
                                Ok(data) => {
                                    boom_count += 1;

                                    if verbose && boom_count % 1000 == 0 {
                                        debug!(
                                            "BOOM progress: {} alerts parsed, {} errors",
                                            boom_count, boom_errors
                                        );
                                    }

                                    if boom_tx.send(data).await.is_err() {
                                        // Receiver dropped, exit
                                        break;
                                    }
                                }
                                Err(e) => {
                                    boom_errors += 1;
                                    if verbose && boom_errors <= 5 {
                                        debug!("BOOM parse error: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("BOOM Kafka error: {}", e);
                    }
                }
            }
            info!(
                "BOOM consumer task exiting: {} alerts, {} errors",
                boom_count, boom_errors
            );
        });
    }

    // Initialize correlator and router
    // Disable expensive GP-based light curve fitting; keep fast early linear rates
    let correlator_config = CorrelatorConfig::without_lc_filter();
    let mut correlator = SupereventCorrelator::new(correlator_config);
    let router = AlertRouter::new();

    // Stats
    let start = Instant::now();
    let mut gw_count: usize = 0;
    let mut grb_count: usize = 0;
    let mut optical_count: usize = 0;
    let mut other_count: usize = 0;
    let mut parse_errors: usize = 0;
    let mut total_messages: usize = 0;
    let mut correlation_records: Vec<CorrelationRecord> = Vec::new();
    let mut optical_correlation_records: Vec<OpticalCorrelationRecord> = Vec::new();

    // Duration timer
    let deadline = cli
        .duration
        .map(|d| tokio::time::Instant::now() + Duration::from_secs(d));

    // Main consumption loop with graceful shutdown
    loop {
        tokio::select! {
            msg_result = consumer.recv() => {
                match msg_result {
                    Err(err) => {
                        error!("Kafka receive error: {}", err);
                    }
                    Ok(msg) => {
                        let topic = msg.topic();

                        // Log heartbeats as connectivity proof
                        if topic == "gcn.heartbeat" {
                            if total_messages == 0 {
                                info!("Connection verified (heartbeat received)");
                            }
                            total_messages += 1;
                            continue;
                        }

                        let payload = match msg.payload_view::<str>() {
                            Some(Ok(p)) => p,
                            Some(Err(e)) => {
                                error!("Failed to decode message from {}: {}", topic, e);
                                continue;
                            }
                            None => continue,
                        };

                        total_messages += 1;

                        // Parse alert
                        let event = match router.route_and_parse(topic, payload) {
                            Ok(event) => event,
                            Err(e) => {
                                if cli.verbose {
                                    warn!("Parse error from {}: {}", topic, e);
                                }
                                parse_errors += 1;
                                continue;
                            }
                        };

                        // Track event type and extract GW info for simulation
                        let event_type = event.event_type();
                        let mut gw_info_for_sim: Option<(f64, Option<SkyPosition>)> = None;
                        match event_type {
                            mm_core::EventType::GravitationalWave => {
                                gw_count += 1;
                                if let mm_core::Event::GravitationalWave(ref gw) = event {
                                    let ts = gw.gps_time.seconds;
                                    info!("GW event: {} (GPS {:.2})", gw.superevent_id, ts);
                                    // Save GW info for simulation injection
                                    if cli.simulate {
                                        gw_info_for_sim = Some((ts, gw.position.clone()));
                                    }
                                }
                            }
                            mm_core::EventType::GammaRay => {
                                grb_count += 1;
                                if cli.verbose {
                                    if let mm_core::Event::GammaRay(ref grb) = event {
                                        info!("GRB event: {} from {} (GPS {:.2})",
                                            grb.trigger_id, grb.instrument, grb.trigger_time);
                                    }
                                }
                            }
                            _ => {
                                other_count += 1;
                                continue; // Skip non GW/GRB events
                            }
                        }

                        // Feed into correlator
                        match correlator.process_gcn_event(event) {
                            Ok(affected_ids) => {
                                for id in &affected_ids {
                                    if let Some(superevent) = correlator.get_superevent(id) {
                                        // Check if this superevent now has GW+GRB
                                        if superevent.gw_event.is_some()
                                            && !superevent.gamma_ray_candidates.is_empty()
                                        {
                                            let gw = superevent.gw_event.as_ref().unwrap();
                                            for grb in &superevent.gamma_ray_candidates {
                                                let record = CorrelationRecord {
                                                    superevent_id: superevent.id.clone(),
                                                    gw_superevent_id: gw.superevent_id.clone(),
                                                    gw_gps_time: gw.gps_time,
                                                    grb_trigger_id: grb.trigger_id.clone(),
                                                    grb_instrument: String::new(), // Not stored in candidate
                                                    time_offset_s: grb.time_offset,
                                                    spatial_offset_deg: grb.spatial_offset,
                                                    skymap_probability: grb.skymap_probability,
                                                    in_90cr: grb.in_90cr,
                                                };

                                                // Avoid duplicate records
                                                let is_new = !correlation_records.iter().any(|r| {
                                                    r.gw_superevent_id == record.gw_superevent_id
                                                        && r.grb_trigger_id == record.grb_trigger_id
                                                });

                                                if is_new {
                                                    info!(
                                                        "GW+GRB CORRELATION: {} + {} (dt={:.2}s, spatial={:?}deg)",
                                                        record.gw_superevent_id,
                                                        record.grb_trigger_id,
                                                        record.time_offset_s,
                                                        record.spatial_offset_deg,
                                                    );
                                                    correlation_records.push(record);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Correlator error: {}", e);
                            }
                        }

                        // Inject simulated light curves after GW event
                        if let Some((gw_gps_time, gw_position)) = gw_info_for_sim {
                            let gw_mjd = gps_to_mjd(gw_gps_time);
                            let n_available = simulated_lightcurves.len();
                            let n_sim = cli.max_sim.unwrap_or(n_available).min(n_available);

                            // Create MockSkymap centered on GW position for position sampling
                            let skymap = if let Some(ref pos) = gw_position {
                                MockSkymap::typical_ns_merger(pos.ra, pos.dec)
                            } else {
                                // No position info: use wide skymap at a default location
                                MockSkymap::poor_localization(180.0, 0.0)
                            };

                            let mut sim_rng = rand::rngs::StdRng::seed_from_u64(42);
                            let mut sim_injected = 0;
                            let mut sim_correlated = 0;

                            info!(
                                "Injecting {} simulated light curves for GW at GPS {:.2} (MJD {:.2})",
                                n_sim, gw_gps_time, gw_mjd
                            );

                            for orig_lc in simulated_lightcurves.iter().take(n_sim) {
                                if orig_lc.measurements.is_empty() {
                                    continue;
                                }

                                // Time-shift: align first detection to gw_mjd + 0.5 days
                                // The fitter will estimate t0 ≈ first_detection - ~1 day ≈ gw_mjd - 0.5
                                // which falls within the correlator's temporal window
                                let first_mjd = orig_lc.measurements.iter()
                                    .map(|m| m.mjd)
                                    .fold(f64::INFINITY, f64::min);
                                let mjd_shift = (gw_mjd + 0.5) - first_mjd;

                                let mut shifted_lc = LightCurve::new(
                                    format!("SIM_{}", orig_lc.object_id)
                                );
                                for m in &orig_lc.measurements {
                                    shifted_lc.add_measurement(Photometry {
                                        mjd: m.mjd + mjd_shift,
                                        flux: m.flux,
                                        flux_err: m.flux_err,
                                        filter: m.filter.clone(),
                                        is_upper_limit: m.is_upper_limit,
                                    });
                                }

                                // Sample position from GW skymap
                                let sim_pos = skymap.sample_position(&mut sim_rng);

                                // Feed into correlator
                                match correlator.process_optical_lightcurve(&shifted_lc, &sim_pos) {
                                    Ok(matched_ids) => {
                                        for id in &matched_ids {
                                            if let Some(superevent) = correlator.get_superevent(id) {
                                                if let Some(gw) = &superevent.gw_event {
                                                    for opt in &superevent.optical_candidates {
                                                        if opt.object_id == shifted_lc.object_id {
                                                            let record = OpticalCorrelationRecord {
                                                                superevent_id: superevent.id.clone(),
                                                                gw_superevent_id: gw.superevent_id.clone(),
                                                                object_id: shifted_lc.object_id.clone(),
                                                                time_offset_s: opt.time_offset,
                                                                spatial_offset_deg: Some(opt.spatial_offset),
                                                                skymap_probability: opt.skymap_probability,
                                                                in_90cr: opt.in_90cr,
                                                            };

                                                            let is_new = !optical_correlation_records.iter().any(|r| {
                                                                r.gw_superevent_id == record.gw_superevent_id
                                                                    && r.object_id == record.object_id
                                                            });

                                                            if is_new {
                                                                sim_correlated += 1;
                                                                info!(
                                                                    "GW+SIM CORRELATION: {} + {} (dt={:.2}s, sep={:?}deg, skymap_prob={:?}, in_90cr={:?})",
                                                                    record.gw_superevent_id,
                                                                    record.object_id,
                                                                    record.time_offset_s,
                                                                    record.spatial_offset_deg,
                                                                    record.skymap_probability,
                                                                    record.in_90cr,
                                                                );
                                                                optical_correlation_records.push(record);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if cli.verbose && sim_injected < 5 {
                                            debug!("Sim correlator error for {}: {}", shifted_lc.object_id, e);
                                        }
                                    }
                                }

                                sim_injected += 1;
                                optical_count += 1;

                                if sim_injected % 100 == 0 {
                                    info!(
                                        "Simulation progress: {}/{} injected, {} correlated | {:.0}s",
                                        sim_injected, n_sim, sim_correlated,
                                        start.elapsed().as_secs_f64()
                                    );
                                }
                            }

                            info!(
                                "Simulation injection complete: {}/{} injected, {} GW+optical correlations",
                                sim_injected, n_sim, sim_correlated
                            );
                        }

                        // Periodic stats
                        if total_messages % 100 == 0 {
                            info!(
                                "Progress: {} msgs ({} GW, {} GRB, {} optical, {} other, {} errors) | {} corr | {:.0}s",
                                total_messages, gw_count, grb_count, optical_count, other_count, parse_errors,
                                correlation_records.len() + optical_correlation_records.len(),
                                start.elapsed().as_secs_f64()
                            );
                        }
                    }
                }
            }
            // Receive parsed BOOM optical alerts from the background task
            boom_msg = boom_rx.recv() => {
                match boom_msg {
                    Some((lc, pos, object_id)) => {
                        optical_count += 1;
                        total_messages += 1;

                        if cli.verbose && optical_count <= 5 {
                            info!("Optical alert: {} (RA={:.4}, Dec={:.4}, {} measurements)",
                                object_id, pos.ra, pos.dec, lc.measurements.len());
                        }

                        // Skip heavy correlator processing if no GW events exist yet
                        // (avoids expensive GP fitting with zero chance of correlation)
                        if gw_count == 0 {
                            if optical_count % 1000 == 0 {
                                info!(
                                    "BOOM progress: {} optical alerts (skipping correlation, no GW events yet) | {:.0}s",
                                    optical_count, start.elapsed().as_secs_f64()
                                );
                            }
                            continue;
                        }

                        // Feed into correlator
                        match correlator.process_optical_lightcurve(&lc, &pos) {
                            Ok(matched_ids) => {
                                for id in &matched_ids {
                                    if let Some(superevent) = correlator.get_superevent(id) {
                                        if let Some(gw) = &superevent.gw_event {
                                            for opt in &superevent.optical_candidates {
                                                if opt.object_id == object_id {
                                                    let record = OpticalCorrelationRecord {
                                                        superevent_id: superevent.id.clone(),
                                                        gw_superevent_id: gw.superevent_id.clone(),
                                                        object_id: object_id.clone(),
                                                        time_offset_s: opt.time_offset,
                                                        spatial_offset_deg: Some(opt.spatial_offset),
                                                        skymap_probability: opt.skymap_probability,
                                                        in_90cr: opt.in_90cr,
                                                    };

                                                    let is_new = !optical_correlation_records.iter().any(|r| {
                                                        r.gw_superevent_id == record.gw_superevent_id
                                                            && r.object_id == record.object_id
                                                    });

                                                    if is_new {
                                                        info!(
                                                            "GW+OPTICAL CORRELATION: {} + {} (dt={:.2}s, skymap_prob={:?}, in_90cr={:?})",
                                                            record.gw_superevent_id,
                                                            record.object_id,
                                                            record.time_offset_s,
                                                            record.skymap_probability,
                                                            record.in_90cr,
                                                        );
                                                        optical_correlation_records.push(record);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                if cli.verbose {
                                    debug!("Optical correlator error for {}: {}", object_id, e);
                                }
                            }
                        }

                        // Periodic optical stats
                        if optical_count % 1000 == 0 {
                            info!(
                                "BOOM progress: {} optical alerts | {} GW+optical correlations | {:.0}s",
                                optical_count,
                                optical_correlation_records.len(),
                                start.elapsed().as_secs_f64()
                            );
                        }
                    }
                    None => {
                        // Channel closed, BOOM task exited
                        info!("BOOM consumer channel closed");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
                break;
            }
            _ = async {
                match deadline {
                    Some(dl) => tokio::time::sleep_until(dl).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                info!("Duration limit reached ({} seconds)", cli.duration.unwrap_or(0));
                break;
            }
        }
    }

    // Summary
    let runtime = start.elapsed().as_secs_f64();
    info!("========================================");
    info!("Run complete ({:.1}s)", runtime);
    info!("  GW events:       {}", gw_count);
    info!("  GRB events:      {}", grb_count);
    info!("  Optical events:  {}", optical_count);
    info!("  Other events:    {}", other_count);
    info!("  Parse errors:    {}", parse_errors);
    info!("  GW+GRB corr:    {}", correlation_records.len());
    info!("  GW+Optical corr: {}", optical_correlation_records.len());

    // List all GW+GRB correlations
    for (i, record) in correlation_records.iter().enumerate() {
        info!(
            "  [GRB {}] {} + {} | dt={:.2}s | spatial={:?}deg | skymap_prob={:?} | in_90cr={:?}",
            i + 1,
            record.gw_superevent_id,
            record.grb_trigger_id,
            record.time_offset_s,
            record.spatial_offset_deg,
            record.skymap_probability,
            record.in_90cr,
        );
    }

    // List all GW+Optical correlations
    for (i, record) in optical_correlation_records.iter().enumerate() {
        info!(
            "  [OPT {}] {} + {} | dt={:.2}s | skymap_prob={:?} | in_90cr={:?}",
            i + 1,
            record.gw_superevent_id,
            record.object_id,
            record.time_offset_s,
            record.skymap_probability,
            record.in_90cr,
        );
    }

    // Also summarize all superevents
    let all_superevents = correlator.get_all_superevents();
    info!("Total superevents: {}", all_superevents.len());
    let gw_grb = correlator.get_gw_grb_correlations();
    info!("Superevents with GW+GRB: {}", gw_grb.len());

    // Write output file if requested
    if let Some(output_path) = &cli.output {
        let output = RunOutput {
            run_config: RunConfig {
                from_beginning: cli.from_beginning,
                duration_s: cli.duration,
                topics,
            },
            stats: RunStats {
                gw_events: gw_count,
                grb_events: grb_count,
                optical_events: optical_count,
                other_events: other_count,
                parse_errors,
                correlations: correlation_records.len(),
                optical_correlations: optical_correlation_records.len(),
                runtime_s: runtime,
            },
            correlations: correlation_records,
            optical_correlations: optical_correlation_records,
        };

        let json = serde_json::to_string_pretty(&output)?;
        std::fs::write(output_path, &json)?;
        info!("Results written to: {}", output_path);
    }

    Ok(())
}
