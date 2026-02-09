//! Multi-Messenger API Server
//!
//! Serves event data and skymaps for Grafana visualization
//!
//! Usage:
//! ```bash
//! cargo run --bin mm-api-server --release -- --bind 0.0.0.0:8080
//! ```

use anyhow::Result;
use clap::Parser;
use mm_api::{ApiState, run_server};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{info, Level};

#[derive(Parser)]
#[command(name = "mm-api-server")]
#[command(about = "Multi-Messenger API server for Grafana integration")]
struct Args {
    /// Bind address
    #[arg(long, default_value = "0.0.0.0:8080")]
    bind: String,

    /// Optional directory for storing skymap files
    #[arg(long)]
    skymap_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let args = Args::parse();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║        Multi-Messenger API Server for Grafana                ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");
    info!("Binding to: {}", args.bind);
    info!("");
    info!("Endpoints:");
    info!("  GET /health                    - Health check");
    info!("  GET /api/events                - List all events");
    info!("  GET /api/events/{{id}}          - Get specific event");
    info!("  GET /api/skymaps/{{id}}         - Get skymap FITS file");
    info!("  GET /api/skymaps/{{id}}/moc     - Get skymap MOC format");
    info!("");

    // Initialize shared state
    let state = ApiState {
        events: Arc::new(Mutex::new(HashMap::new())),
        skymaps: Arc::new(Mutex::new(HashMap::new())),
        skymap_dir: args.skymap_dir.map(|s| s.into()),
    };

    // Start server
    info!("🚀 Starting server...");
    run_server(&args.bind, state).await?;

    Ok(())
}
