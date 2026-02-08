use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct CorrelationMessage {
    simulation_id: usize,
    gw_snr: f64,
    has_grb: bool,
    has_optical: bool,
    optical_magnitude: Option<f64>,
    joint_far_per_year: f64,
    significance_sigma: f64,
    pastro: f64,
}

#[derive(Debug, Clone, Default)]
struct Metrics {
    total_correlations: u64,
    total_with_grb: u64,
    total_with_optical: u64,
    total_three_way: u64,
    last_gw_snr: f64,
    last_joint_far: f64,
    last_significance: f64,
    last_pastro: f64,
    last_optical_mag: f64,
    min_far: f64,
    max_significance: f64,
    avg_significance: f64,
}

impl Metrics {
    fn update(&mut self, msg: &CorrelationMessage) {
        let n = self.total_correlations as f64;

        // Count association types
        self.total_correlations += 1;
        if msg.has_grb {
            self.total_with_grb += 1;
        }
        if msg.has_optical {
            self.total_with_optical += 1;
        }
        if msg.has_grb && msg.has_optical {
            self.total_three_way += 1;
        }

        // Running average for significance
        self.avg_significance = (self.avg_significance * n + msg.significance_sigma) / (n + 1.0);

        // Track extremes
        if self.total_correlations == 1 || msg.joint_far_per_year < self.min_far {
            self.min_far = msg.joint_far_per_year;
        }
        if msg.significance_sigma > self.max_significance {
            self.max_significance = msg.significance_sigma;
        }

        // Last values
        self.last_gw_snr = msg.gw_snr;
        self.last_joint_far = msg.joint_far_per_year;
        self.last_significance = msg.significance_sigma;
        self.last_pastro = msg.pastro;
        self.last_optical_mag = msg.optical_magnitude.unwrap_or(99.0);
    }

    fn to_prometheus(&self) -> String {
        format!(
            "# HELP mm_correlations_total Total number of multi-messenger correlations\n\
             # TYPE mm_correlations_total counter\n\
             mm_correlations_total {}\n\
             \n\
             # HELP mm_correlations_with_grb Total correlations with GRB detection\n\
             # TYPE mm_correlations_with_grb counter\n\
             mm_correlations_with_grb {}\n\
             \n\
             # HELP mm_correlations_with_optical Total correlations with optical detection\n\
             # TYPE mm_correlations_with_optical counter\n\
             mm_correlations_with_optical {}\n\
             \n\
             # HELP mm_correlations_three_way Total three-way correlations (GW+GRB+Optical)\n\
             # TYPE mm_correlations_three_way counter\n\
             mm_correlations_three_way {}\n\
             \n\
             # HELP mm_last_gw_snr Last gravitational wave SNR\n\
             # TYPE mm_last_gw_snr gauge\n\
             mm_last_gw_snr {}\n\
             \n\
             # HELP mm_last_joint_far_per_year Last joint false alarm rate per year\n\
             # TYPE mm_last_joint_far_per_year gauge\n\
             mm_last_joint_far_per_year {}\n\
             \n\
             # HELP mm_last_significance_sigma Last significance in Gaussian sigma\n\
             # TYPE mm_last_significance_sigma gauge\n\
             mm_last_significance_sigma {}\n\
             \n\
             # HELP mm_last_pastro Last astrophysical probability (0-1)\n\
             # TYPE mm_last_pastro gauge\n\
             mm_last_pastro {}\n\
             \n\
             # HELP mm_last_optical_magnitude Last optical magnitude (mag)\n\
             # TYPE mm_last_optical_magnitude gauge\n\
             mm_last_optical_magnitude {}\n\
             \n\
             # HELP mm_min_joint_far Minimum joint FAR observed (most significant)\n\
             # TYPE mm_min_joint_far gauge\n\
             mm_min_joint_far {}\n\
             \n\
             # HELP mm_max_significance Maximum significance observed (sigma)\n\
             # TYPE mm_max_significance gauge\n\
             mm_max_significance {}\n\
             \n\
             # HELP mm_avg_significance Average significance across all correlations (sigma)\n\
             # TYPE mm_avg_significance gauge\n\
             mm_avg_significance {}\n",
            self.total_correlations,
            self.total_with_grb,
            self.total_with_optical,
            self.total_three_way,
            self.last_gw_snr,
            self.last_joint_far,
            self.last_significance,
            self.last_pastro,
            self.last_optical_mag,
            self.min_far,
            self.max_significance,
            self.avg_significance,
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("=== Multi-Messenger Correlation Metrics Exporter ===\n");

    // Shared metrics
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let metrics_clone = metrics.clone();

    // Spawn Kafka consumer task
    tokio::spawn(async move {
        if let Err(e) = consume_correlations(metrics_clone).await {
            warn!("Kafka consumer error: {}", e);
        }
    });

    // Start Prometheus HTTP server
    let listener = TcpListener::bind("0.0.0.0:9091").await?;
    info!("📊 Prometheus metrics available at http://localhost:9091/metrics\n");

    loop {
        let (mut socket, _) = listener.accept().await?;
        let metrics = metrics.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];

            // Read the HTTP request (async)
            match socket.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    // Generate metrics
                    let body = {
                        let m = metrics.lock().unwrap();
                        m.to_prometheus()
                    };

                    // Send proper HTTP response
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                         Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                         Content-Length: {}\r\n\
                         Connection: close\r\n\
                         \r\n\
                         {}",
                        body.len(),
                        body
                    );

                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                }
                _ => {}
            }
        });
    }
}

async fn consume_correlations(metrics: Arc<Mutex<Metrics>>) -> Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", "correlation-exporter")
        .set("bootstrap.servers", "localhost:9092")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&["mm.correlations"])?;
    info!("📡 Subscribed to mm.correlations topic");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(Ok(payload)) = msg.payload_view::<str>() {
                    match serde_json::from_str::<CorrelationMessage>(payload) {
                        Ok(correlation) => {
                            let mut m = metrics.lock().unwrap();
                            m.update(&correlation);
                            info!(
                                "📈 Updated metrics: {} total correlations",
                                m.total_correlations
                            );
                        }
                        Err(e) => {
                            warn!("Failed to parse correlation: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Kafka error: {}", e);
            }
        }
    }
}
