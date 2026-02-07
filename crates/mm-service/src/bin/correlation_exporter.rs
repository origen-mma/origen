use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
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
    timestamp: f64,
}

#[derive(Debug, Clone, Default)]
struct Metrics {
    total_correlations: u64,
    last_overlap_area: f64,
    last_gw_area: f64,
    last_grb_area: f64,
    last_overlap_frac_gw: f64,
    last_overlap_frac_grb: f64,
    last_time_offset: f64,
    avg_overlap_area: f64,
    avg_gw_area: f64,
    avg_grb_area: f64,
}

impl Metrics {
    fn update(&mut self, msg: &CorrelationMessage) {
        let n = self.total_correlations as f64;

        // Running average
        self.avg_overlap_area = (self.avg_overlap_area * n + msg.overlap_area) / (n + 1.0);
        self.avg_gw_area = (self.avg_gw_area * n + msg.gw_90cr_area) / (n + 1.0);
        self.avg_grb_area = (self.avg_grb_area * n + msg.grb_90cr_area) / (n + 1.0);

        // Last values
        self.total_correlations += 1;
        self.last_overlap_area = msg.overlap_area;
        self.last_gw_area = msg.gw_90cr_area;
        self.last_grb_area = msg.grb_90cr_area;
        self.last_overlap_frac_gw = msg.overlap_fraction_gw;
        self.last_overlap_frac_grb = msg.overlap_fraction_grb;
        self.last_time_offset = msg.time_offset;
    }

    fn to_prometheus(&self) -> String {
        format!(
            "# HELP mm_correlations_total Total number of multi-messenger correlations\n\
             # TYPE mm_correlations_total counter\n\
             mm_correlations_total {}\n\
             \n\
             # HELP mm_last_overlap_area_sq_deg Last correlation overlap area in square degrees\n\
             # TYPE mm_last_overlap_area_sq_deg gauge\n\
             mm_last_overlap_area_sq_deg {}\n\
             \n\
             # HELP mm_last_gw_area_sq_deg Last GW 90% credible region area\n\
             # TYPE mm_last_gw_area_sq_deg gauge\n\
             mm_last_gw_area_sq_deg {}\n\
             \n\
             # HELP mm_last_grb_area_sq_deg Last GRB 90% credible region area\n\
             # TYPE mm_last_grb_area_sq_deg gauge\n\
             mm_last_grb_area_sq_deg {}\n\
             \n\
             # HELP mm_last_overlap_fraction_gw Last overlap as fraction of GW area\n\
             # TYPE mm_last_overlap_fraction_gw gauge\n\
             mm_last_overlap_fraction_gw {}\n\
             \n\
             # HELP mm_last_overlap_fraction_grb Last overlap as fraction of GRB area\n\
             # TYPE mm_last_overlap_fraction_grb gauge\n\
             mm_last_overlap_fraction_grb {}\n\
             \n\
             # HELP mm_last_time_offset_seconds Last time offset between GW and GRB\n\
             # TYPE mm_last_time_offset_seconds gauge\n\
             mm_last_time_offset_seconds {}\n\
             \n\
             # HELP mm_avg_overlap_area_sq_deg Average overlap area across all correlations\n\
             # TYPE mm_avg_overlap_area_sq_deg gauge\n\
             mm_avg_overlap_area_sq_deg {}\n\
             \n\
             # HELP mm_avg_gw_area_sq_deg Average GW area across all correlations\n\
             # TYPE mm_avg_gw_area_sq_deg gauge\n\
             mm_avg_gw_area_sq_deg {}\n\
             \n\
             # HELP mm_avg_grb_area_sq_deg Average GRB area across all correlations\n\
             # TYPE mm_avg_grb_area_sq_deg gauge\n\
             mm_avg_grb_area_sq_deg {}\n",
            self.total_correlations,
            self.last_overlap_area,
            self.last_gw_area,
            self.last_grb_area,
            self.last_overlap_frac_gw,
            self.last_overlap_frac_grb,
            self.last_time_offset,
            self.avg_overlap_area,
            self.avg_gw_area,
            self.avg_grb_area,
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
                            info!("📈 Updated metrics: {} total correlations", m.total_correlations);
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
