use gcn_kafka::GcnClientConfig;
use mm_config::Config;
use mm_gcn::AlertRouter;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    ClientConfig, Message,
};
use std::env;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("mm_service=info,mm_gcn=info,mm_core=info")
        .init();

    info!("Starting GCN Kafka consumer...");

    // Load configuration
    let config_path =
        env::var("MM_CONFIG_PATH").unwrap_or_else(|_| "config/config.toml".to_string());

    info!("Loading configuration from: {}", config_path);

    let app_config = match Config::from_file_with_env(&config_path) {
        Ok(cfg) => {
            info!("Configuration loaded successfully");
            cfg
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            warn!("Using development defaults (credentials need to be set!)");
            Config::development()
        }
    };

    // Configure GCN Kafka client
    let mut config = ClientConfig::new();
    config.set_gcn_auth(
        &app_config.gcn.client_id,
        &app_config.gcn.client_secret,
        None,
    );

    // Create consumer
    let consumer: StreamConsumer = config.create()?;

    // Subscribe to topics from config
    let topics: Vec<&str> = app_config.gcn.topics.iter().map(|s| s.as_str()).collect();

    consumer.subscribe(&topics)?;
    info!("Subscribed to {} GCN topics", topics.len());
    info!("Topics: {}", topics.join(", "));
    info!("Waiting for alerts...");

    // Initialize alert router
    let router = AlertRouter::new();

    // Main consumption loop
    loop {
        match consumer.recv().await {
            Err(err) => {
                error!("Kafka receive error: {}", err);
            }
            Ok(msg) => {
                let topic = msg.topic();

                // Skip heartbeat messages (just log them)
                if topic == "gcn.heartbeat" {
                    info!("Received heartbeat");
                    continue;
                }

                // Get payload as string
                if let Some(result) = msg.payload_view::<str>() {
                    match result {
                        Err(err) => {
                            error!("Failed to decode message from {}: {}", topic, err);
                        }
                        Ok(payload) => {
                            info!("========================================");
                            info!("Received alert from topic: {}", topic);

                            // Route to appropriate parser
                            match router.route_and_parse(topic, payload) {
                                Ok(event) => {
                                    info!("Successfully parsed event");
                                    info!("Event type: {:?}", event.event_type());

                                    if let Some(ts) = event.timestamp() {
                                        info!("Timestamp: {}", ts);
                                    }

                                    if let Some(pos) = event.sky_position() {
                                        info!(
                                            "Position: RA={:.3}, Dec={:.3}, Error={:.1}\"",
                                            pos.ra, pos.dec, pos.uncertainty
                                        );
                                    }

                                    // Pretty-print the event
                                    let json = serde_json::to_string_pretty(&event).unwrap();
                                    info!("Parsed event:\n{}", json);

                                    // Phase 1: Just log to stdout
                                    // Phase 3: Pass to correlator
                                }
                                Err(e) => {
                                    error!("Failed to parse alert from {}: {}", topic, e);
                                    // Log raw payload for debugging
                                    if payload.len() < 1000 {
                                        error!("Raw payload: {}", payload);
                                    } else {
                                        error!("Raw payload (truncated): {}...", &payload[..1000]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
