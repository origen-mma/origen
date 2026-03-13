use anyhow::Result;
use chrono::{NaiveDate, Utc};
use clap::Parser;
use mm_boom::parse_boom_alert;
use mm_config::Config;
use mm_core::{Event, EventType, LightCurve, SkyPosition};
use mm_correlator::{
    daily_report::{self, AlertSource, DailyReport, EventTypeCounts, InventoryEntry},
    CorrelatorConfig,
};
use mm_gcn::AlertRouter;
use rdkafka::{
    client::{ClientContext, OAuthToken},
    config::RDKafkaLogLevel,
    consumer::{Consumer, ConsumerContext, StreamConsumer},
    ClientConfig, Message,
};
use serde::Deserialize;
use std::env;
use std::error::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// GCN OAuth context (same as gcn_correlator)
// ---------------------------------------------------------------------------

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

        let lifetime_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
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

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "daily-comparison",
    about = "Daily GCN vs BOOM comparison service — accumulates events per UTC day and produces cross-match, completeness, and RAVEN correlation reports"
)]
struct Cli {
    /// Start consuming from earliest available offset (historical replay)
    #[arg(short = 'b', long)]
    from_beginning: bool,

    /// Extra logging
    #[arg(short, long)]
    verbose: bool,

    /// Path to config file
    #[arg(short, long, default_value = "config/config.toml")]
    config: String,

    /// Output directory for daily JSON reports
    #[arg(long)]
    output_dir: Option<String>,

    /// Redis URL for report persistence (optional)
    #[arg(long)]
    redis_url: Option<String>,

    /// Spatial cross-match threshold (degrees)
    #[arg(long)]
    spatial_threshold: Option<f64>,

    /// Temporal cross-match threshold (seconds)
    #[arg(long)]
    temporal_threshold: Option<f64>,

    /// Exit after producing one daily report (for testing/cron usage)
    #[arg(long)]
    single_day: bool,

    /// Also consume BOOM optical transient alerts from kaboom.caltech.edu
    #[arg(long)]
    boom: bool,
}

// ---------------------------------------------------------------------------
// Day accumulator
// ---------------------------------------------------------------------------

struct ErrorCounts {
    gcn_parse: usize,
    boom_parse: usize,
    gcn_kafka: usize,
    boom_kafka: usize,
}

impl ErrorCounts {
    fn new() -> Self {
        Self {
            gcn_parse: 0,
            boom_parse: 0,
            gcn_kafka: 0,
            boom_kafka: 0,
        }
    }
}

struct DayAccumulator {
    current_date: NaiveDate,
    gcn_inventory: Vec<InventoryEntry>,
    boom_inventory: Vec<InventoryEntry>,
    gcn_events: Vec<Event>,
    boom_lightcurves: Vec<(LightCurve, SkyPosition, String)>,
    gcn_by_type: EventTypeCounts,
    boom_by_type: EventTypeCounts,
    errors: ErrorCounts,
    day_start: Instant,
}

impl DayAccumulator {
    fn new(date: NaiveDate) -> Self {
        Self {
            current_date: date,
            gcn_inventory: Vec::new(),
            boom_inventory: Vec::new(),
            gcn_events: Vec::new(),
            boom_lightcurves: Vec::new(),
            gcn_by_type: EventTypeCounts::default(),
            boom_by_type: EventTypeCounts::default(),
            errors: ErrorCounts::new(),
            day_start: Instant::now(),
        }
    }

    fn reset(&mut self, date: NaiveDate) {
        self.current_date = date;
        self.gcn_inventory.clear();
        self.boom_inventory.clear();
        self.gcn_events.clear();
        self.boom_lightcurves.clear();
        self.gcn_by_type = EventTypeCounts::default();
        self.boom_by_type = EventTypeCounts::default();
        self.errors = ErrorCounts::new();
        self.day_start = Instant::now();
    }
}

fn now_unix_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

// ---------------------------------------------------------------------------
// Report generation
// ---------------------------------------------------------------------------

fn generate_report(
    acc: &DayAccumulator,
    spatial_threshold: f64,
    temporal_threshold: f64,
) -> DailyReport {
    let cross_matches = daily_report::cross_match_events(
        &acc.gcn_inventory,
        &acc.boom_inventory,
        spatial_threshold,
        temporal_threshold,
    );

    let gcn_completeness =
        daily_report::compute_completeness(&acc.gcn_inventory, &cross_matches, AlertSource::Gcn);
    let boom_completeness =
        daily_report::compute_completeness(&acc.boom_inventory, &cross_matches, AlertSource::Boom);

    let (med_gcn, med_boom, gcn_first, boom_first) =
        daily_report::compute_latency_stats(&cross_matches);

    let correlator_config = CorrelatorConfig::without_lc_filter();
    let correlator_summary = daily_report::run_daily_correlation(
        &acc.gcn_events,
        &acc.boom_lightcurves,
        &correlator_config,
    );

    DailyReport {
        date: acc.current_date.format("%Y-%m-%d").to_string(),
        generated_at: Utc::now().to_rfc3339(),
        uptime_s: acc.day_start.elapsed().as_secs_f64(),
        gcn_event_count: acc.gcn_inventory.len(),
        boom_event_count: acc.boom_inventory.len(),
        gcn_by_type: acc.gcn_by_type.clone(),
        boom_by_type: acc.boom_by_type.clone(),
        cross_matches: cross_matches.clone(),
        total_cross_matches: cross_matches.len(),
        gcn_completeness,
        boom_completeness,
        median_latency_advantage_gcn_s: med_gcn,
        median_latency_advantage_boom_s: med_boom,
        gcn_first_count: gcn_first,
        boom_first_count: boom_first,
        correlator_summary,
        gcn_parse_errors: acc.errors.gcn_parse,
        boom_parse_errors: acc.errors.boom_parse,
        gcn_kafka_errors: acc.errors.gcn_kafka,
        boom_kafka_errors: acc.errors.boom_kafka,
    }
}

async fn emit_report(
    report: &DailyReport,
    output_dir: &str,
    redis_url: Option<&str>,
) -> Result<()> {
    // Write JSON file
    std::fs::create_dir_all(output_dir)?;
    let filename = format!("{}/daily_report_{}.json", output_dir, report.date);
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&filename, &json)?;
    info!("Report written to: {}", filename);

    // Store in Redis if configured
    if let Some(url) = redis_url {
        match store_report_redis(report, url).await {
            Ok(()) => info!("Report stored in Redis"),
            Err(e) => warn!("Failed to store report in Redis: {}", e),
        }
    }

    // Log summary
    info!("=== Daily Report: {} ===", report.date);
    info!(
        "  GCN events: {} | BOOM events: {}",
        report.gcn_event_count, report.boom_event_count
    );
    info!("  Cross-matches: {}", report.total_cross_matches);
    info!(
        "  GCN completeness: {:.1}% | BOOM completeness: {:.1}%",
        report.gcn_completeness.completeness_fraction * 100.0,
        report.boom_completeness.completeness_fraction * 100.0,
    );
    info!(
        "  First reporter: GCN={} BOOM={}",
        report.gcn_first_count, report.boom_first_count,
    );
    info!(
        "  RAVEN: {} superevents ({} GW+GRB, {} GW+optical, {} multi-messenger)",
        report.correlator_summary.total_superevents,
        report.correlator_summary.with_grb,
        report.correlator_summary.with_optical,
        report.correlator_summary.multi_messenger,
    );
    if !report.correlator_summary.significant_candidates.is_empty() {
        info!(
            "  Significant candidates: {:?}",
            report.correlator_summary.significant_candidates
        );
    }
    info!(
        "  Errors: GCN parse={} kafka={} | BOOM parse={} kafka={}",
        report.gcn_parse_errors,
        report.gcn_kafka_errors,
        report.boom_parse_errors,
        report.boom_kafka_errors,
    );

    Ok(())
}

async fn store_report_redis(report: &DailyReport, redis_url: &str) -> Result<()> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;

    let key = format!("daily_report:{}", report.date);
    let json = serde_json::to_string(report)?;

    // Store report with 7-day TTL
    redis::cmd("SET")
        .arg(&key)
        .arg(&json)
        .arg("EX")
        .arg(7 * 86400)
        .query_async::<()>(&mut conn)
        .await?;

    // Add to sorted set index (score = unix timestamp of report date)
    let date = NaiveDate::parse_from_str(&report.date, "%Y-%m-%d")?;
    let score = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64;
    redis::cmd("ZADD")
        .arg("daily_reports")
        .arg(score)
        .arg(&report.date)
        .query_async::<()>(&mut conn)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        "daily_comparison=debug,mm_gcn=debug,mm_correlator=debug,mm_core=info"
    } else {
        "daily_comparison=info,mm_gcn=info,mm_correlator=warn"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("=== Daily GCN vs BOOM Comparison Service ===");
    info!(
        "Mode: {}",
        if cli.single_day {
            "single-day (will exit after one report)"
        } else {
            "continuous"
        }
    );
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

    // Precedence: CLI flag > config file > default
    let dc = app_config.daily_comparison.as_ref();
    let spatial_threshold = cli
        .spatial_threshold
        .unwrap_or_else(|| dc.map_or(5.0, |c| c.spatial_threshold));
    let temporal_threshold = cli
        .temporal_threshold
        .unwrap_or_else(|| dc.map_or(86400.0, |c| c.temporal_threshold));
    let output_dir = cli.output_dir.clone().unwrap_or_else(|| {
        dc.map_or_else(
            || "./data/daily_reports".to_string(),
            |c| c.output_dir.clone(),
        )
    });
    let redis_url = cli
        .redis_url
        .clone()
        .or_else(|| dc.and_then(|c| c.redis_url.clone()));

    info!("Output directory: {}", output_dir);
    info!(
        "Thresholds: spatial={:.1}deg, temporal={:.0}s",
        spatial_threshold, temporal_threshold
    );

    // GCN topics — same as gcn-correlator
    let topics = vec![
        "gcn.heartbeat".to_string(),
        "igwn.gwalert".to_string(),
        "gcn.notices.fermi.gbm.alert".to_string(),
        "gcn.notices.swift.bat.guano".to_string(),
        "gcn.notices.einstein_probe.wxt.alert".to_string(),
        "gcn.notices.icecube.lvk_nu_track_search".to_string(),
        "gcn.notices.icecube.gold_bronze_track_alerts".to_string(),
        "gcn.classic.voevent.FERMI_GBM_FLT_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_GND_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_FIN_POS".to_string(),
        "gcn.classic.voevent.FERMI_GBM_SUBTHRESH".to_string(),
    ];

    // Configure GCN Kafka consumer with custom OAuth token callback
    let context = GcnContext {
        client_id: app_config.gcn.client_id.clone(),
        client_secret: app_config.gcn.client_secret.clone(),
    };

    let group_id = if cli.from_beginning {
        format!(
            "origin-daily-replay-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        )
    } else {
        "origin-daily-comparison".to_string()
    };

    let mut kafka_config = ClientConfig::new();
    kafka_config.set("bootstrap.servers", "kafka.gcn.nasa.gov");
    kafka_config.set("security.protocol", "sasl_ssl");
    kafka_config.set("sasl.mechanisms", "OAUTHBEARER");
    kafka_config.set("group.id", &group_id);
    kafka_config.set("session.timeout.ms", "45000");
    kafka_config.set("enable.auto.commit", "false");
    kafka_config.set("auto.offset.reset", "earliest");
    if cli.verbose {
        kafka_config.set("debug", "broker,security");
    }

    info!(
        "Connecting to GCN Kafka (client_id={}...)",
        &app_config.gcn.client_id[..8.min(app_config.gcn.client_id.len())]
    );
    kafka_config.set_log_level(if cli.verbose {
        RDKafkaLogLevel::Debug
    } else {
        RDKafkaLogLevel::Warning
    });
    let consumer: StreamConsumer<GcnContext> = kafka_config.create_with_context(context)?;

    let topic_refs: Vec<&str> = topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topic_refs)?;

    info!("Subscribed to {} GCN topics", topics.len());

    // Set up BOOM optical transient consumer if requested
    let (boom_tx, mut boom_rx) = mpsc::channel::<(LightCurve, SkyPosition, String)>(1000);

    if cli.boom {
        info!("=== BOOM Optical Transient Consumer ===");
        let boom_group_id = format!("{}-daily-comparison", app_config.boom.sasl_username);
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
        info!("Subscribed to {} BOOM topics", boom_topics.len());

        let verbose = cli.verbose;
        tokio::spawn(async move {
            let mut boom_count: usize = 0;
            let mut boom_errors: usize = 0;
            loop {
                match boom_consumer.recv().await {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
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

    // Initialize alert router
    let router = AlertRouter::new();

    // Day accumulator
    let today = Utc::now().date_naive();
    let mut acc = DayAccumulator::new(today);
    let mut total_messages: usize = 0;
    let mut reports_generated: usize = 0;

    info!(
        "Accumulating events for {} (waiting for alerts...)",
        acc.current_date
    );

    // Compute next midnight UTC + 5s buffer
    fn next_midnight_instant() -> tokio::time::Instant {
        let now = Utc::now();
        let tomorrow = (now.date_naive() + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 5)
            .unwrap()
            .and_utc();
        let secs_until = (tomorrow - now).num_seconds().max(1) as u64;
        tokio::time::Instant::now() + Duration::from_secs(secs_until)
    }

    let mut midnight_deadline = next_midnight_instant();

    // Main consumption loop
    loop {
        tokio::select! {
            // GCN Kafka messages
            msg_result = consumer.recv() => {
                match msg_result {
                    Err(err) => {
                        acc.errors.gcn_kafka += 1;
                        error!("GCN Kafka error: {}", err);
                    }
                    Ok(msg) => {
                        let topic = msg.topic();

                        // Skip heartbeats (just connectivity proof)
                        if topic == "gcn.heartbeat" {
                            if total_messages == 0 {
                                info!("GCN connection verified (heartbeat received)");
                            }
                            total_messages += 1;
                            continue;
                        }

                        let payload = match msg.payload_view::<str>() {
                            Some(Ok(p)) => p,
                            Some(Err(e)) => {
                                error!("Failed to decode GCN message from {}: {}", topic, e);
                                acc.errors.gcn_parse += 1;
                                continue;
                            }
                            None => continue,
                        };

                        total_messages += 1;

                        // Check day boundary
                        let now_date = Utc::now().date_naive();
                        if now_date != acc.current_date {
                            info!("Day boundary crossed: {} -> {}", acc.current_date, now_date);
                            let report = generate_report(&acc, spatial_threshold, temporal_threshold);
                            if let Err(e) = emit_report(&report, &output_dir, redis_url.as_deref()).await {
                                error!("Failed to emit report: {}", e);
                            }
                            reports_generated += 1;
                            if cli.single_day {
                                info!("Single-day mode: exiting after report #{}", reports_generated);
                                return Ok(());
                            }
                            acc.reset(now_date);
                            midnight_deadline = next_midnight_instant();
                        }

                        // Parse alert
                        let event = match router.route_and_parse(topic, payload) {
                            Ok(event) => event,
                            Err(e) => {
                                if cli.verbose {
                                    warn!("GCN parse error from {}: {}", topic, e);
                                }
                                acc.errors.gcn_parse += 1;
                                continue;
                            }
                        };

                        let event_type = event.event_type();
                        acc.gcn_by_type.increment(event_type);

                        // Build inventory entry
                        let entry = InventoryEntry {
                            event_id: event_id_from(&event),
                            source: AlertSource::Gcn,
                            event_type,
                            gps_time: event.timestamp().unwrap_or(0.0),
                            position: event.sky_position().cloned(),
                            received_at: now_unix_secs(),
                        };
                        acc.gcn_inventory.push(entry);

                        // Store raw event for RAVEN replay
                        acc.gcn_events.push(event);

                        if cli.verbose && total_messages % 100 == 0 {
                            info!(
                                "Progress: {} msgs | GCN: {} events | BOOM: {} events",
                                total_messages,
                                acc.gcn_inventory.len(),
                                acc.boom_inventory.len(),
                            );
                        }
                    }
                }
            }

            // BOOM optical alerts from background task
            boom_msg = boom_rx.recv() => {
                match boom_msg {
                    Some((lc, pos, object_id)) => {
                        total_messages += 1;

                        // Check day boundary
                        let now_date = Utc::now().date_naive();
                        if now_date != acc.current_date {
                            info!("Day boundary crossed: {} -> {}", acc.current_date, now_date);
                            let report = generate_report(&acc, spatial_threshold, temporal_threshold);
                            if let Err(e) = emit_report(&report, &output_dir, redis_url.as_deref()).await {
                                error!("Failed to emit report: {}", e);
                            }
                            reports_generated += 1;
                            if cli.single_day {
                                info!("Single-day mode: exiting after report #{}", reports_generated);
                                return Ok(());
                            }
                            acc.reset(now_date);
                            midnight_deadline = next_midnight_instant();
                        }

                        acc.boom_by_type.increment_optical();

                        let gps_time = lc.measurements.first()
                            .map(|m| m.to_gps_time())
                            .unwrap_or(0.0);
                        let entry = InventoryEntry {
                            event_id: object_id.clone(),
                            source: AlertSource::Boom,
                            event_type: EventType::Circular, // no Optical variant; Circular is inert in EventTypeCounts
                            gps_time,
                            position: Some(pos.clone()),
                            received_at: now_unix_secs(),
                        };
                        acc.boom_inventory.push(entry);
                        acc.boom_lightcurves.push((lc, pos, object_id));

                        if cli.verbose && acc.boom_inventory.len() % 1000 == 0 {
                            info!(
                                "BOOM progress: {} optical alerts accumulated",
                                acc.boom_inventory.len(),
                            );
                        }
                    }
                    None => {
                        info!("BOOM consumer channel closed");
                    }
                }
            }

            // Midnight UTC timer
            _ = tokio::time::sleep_until(midnight_deadline) => {
                info!("Midnight UTC timer fired for {}", acc.current_date);
                let report = generate_report(&acc, spatial_threshold, temporal_threshold);
                if let Err(e) = emit_report(&report, &output_dir, redis_url.as_deref()).await {
                    error!("Failed to emit report: {}", e);
                }
                reports_generated += 1;
                if cli.single_day {
                    info!("Single-day mode: exiting after report #{}", reports_generated);
                    return Ok(());
                }
                let new_date = Utc::now().date_naive();
                acc.reset(new_date);
                midnight_deadline = next_midnight_instant();
                info!("Starting new day: {}", acc.current_date);
            }

            // Graceful shutdown
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, generating final report before shutdown...");
                let report = generate_report(&acc, spatial_threshold, temporal_threshold);
                if let Err(e) = emit_report(&report, &output_dir, redis_url.as_deref()).await {
                    error!("Failed to emit final report: {}", e);
                }
                info!("Shutting down. Total reports generated: {}", reports_generated + 1);
                break;
            }
        }
    }

    Ok(())
}

/// Extract a human-readable event ID from a parsed Event
fn event_id_from(event: &Event) -> String {
    match event {
        Event::GravitationalWave(gw) => gw.superevent_id.clone(),
        Event::GammaRay(grb) => grb.trigger_id.clone(),
        Event::XRay(xray) => xray.event_id.clone(),
        Event::Neutrino(nu) => nu.event_id.clone(),
        Event::Circular { text } => {
            let prefix: String = text.chars().take(40).collect();
            format!("circular-{}", prefix)
        }
    }
}
