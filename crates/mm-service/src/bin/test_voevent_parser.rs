use anyhow::Result;
use mm_simulation::VOEventParser;
use std::fs;
use tracing::info;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("=== VOEvent Parser Test ===\n");

    // Test with a sample XML file
    let xml_path = "/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices/0.xml";
    info!("Parsing: {}", xml_path);

    let xml_content = fs::read_to_string(xml_path)?;
    let grb_alert = VOEventParser::parse_string(&xml_content)?;

    info!("✅ Successfully parsed VOEvent\n");
    info!("GRB Alert Details:");
    info!("  Instrument: {}", grb_alert.instrument);
    info!("  Trigger ID: {}", grb_alert.trigger_id);
    info!("  Trigger Time: {}", grb_alert.trigger_time);
    info!(
        "  Position: (RA={:.4}°, Dec={:.4}°)",
        grb_alert.ra, grb_alert.dec
    );
    info!("  Error Radius: {:.2}°", grb_alert.error_radius);
    info!("  Packet Type: {:?}", grb_alert.packet_type);
    info!("  Duration: {:?} s", grb_alert.duration);
    info!("  Spectral Class: {:?}", grb_alert.spectral_class);
    info!("  HEALPix URL: {:?}", grb_alert.healpix_url);

    Ok(())
}
