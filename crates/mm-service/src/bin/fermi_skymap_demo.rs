use mm_config::Config;
use mm_core::{Event, GammaRayEvent, GpsTime, MockSkymap, SkymapStorage, SkyPosition};
use mm_correlator::SupereventCorrelator;
use std::env;
use tracing::{error, info, warn};

/// Demo showing Fermi GBM skymap downloading and multi-messenger correlation
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("Starting Fermi GRB Skymap Demo");

    // Load configuration
    let config_path =
        env::var("MM_CONFIG_PATH").unwrap_or_else(|_| "config/config.toml".to_string());

    let app_config = match Config::from_file_with_env(&config_path) {
        Ok(cfg) => {
            info!("Configuration loaded from: {}", config_path);
            cfg
        }
        Err(e) => {
            warn!("Failed to load configuration: {}", e);
            warn!("Using development defaults");
            Config::development()
        }
    };

    // Create skymap storage
    let skymap_storage = SkymapStorage::new(&app_config.simulation.skymap_storage_dir)?;
    info!(
        "Skymap storage initialized at: {}",
        app_config.simulation.skymap_storage_dir
    );

    // Create correlator with RAVEN configuration
    let mut correlator = SupereventCorrelator::new_raven();

    // Simulate a GW event first (typical order: GW detected first, then GRB within ~seconds)
    let gw_gps_time = 1433156802.0; // GPS time
    let gw_ra = 180.0;
    let gw_dec = 45.0;
    let gw_skymap = MockSkymap::typical_ns_merger(gw_ra, gw_dec);

    info!(
        "Simulating GW event: GPS time={}, position=(RA={:.2}, Dec={:.2})",
        gw_gps_time, gw_ra, gw_dec
    );

    let gw_event = mm_core::GWEvent {
        superevent_id: "S240101a".to_string(),
        alert_type: "PRELIMINARY".to_string(),
        gps_time: GpsTime::from_seconds(gw_gps_time),
        instruments: vec!["H1".to_string(), "L1".to_string()],
        far: 1e-10,
        position: Some(gw_skymap.center()),
    };

    let gw_superevent_ids = correlator.process_gcn_event(Event::GravitationalWave(gw_event))?;
    info!("Created GW superevents: {:?}", gw_superevent_ids);

    // Now simulate a Fermi GRB detection 30 seconds later
    let grb_trigger_time = gw_gps_time + 30.0; // 30 seconds after GW
    let grb_ra = gw_ra + 2.0; // Slightly offset but within localization
    let grb_dec = gw_dec + 1.0;
    let grb_error_radius = 5.0; // degrees

    let fermi_event = GammaRayEvent {
        trigger_id: "GRB240101A".to_string(),
        instrument: "Fermi-GBM".to_string(),
        trigger_time: grb_trigger_time,
        position: Some(SkyPosition::new(grb_ra, grb_dec, grb_error_radius * 3600.0)),
        significance: 7.5,
        skymap_url: Some(
            "https://heasarc.gsfc.nasa.gov/FTP/fermi/data/gbm/triggers/2024/bn240101000/quicklook/glg_healpix_all_bn240101000.fit".to_string(),
        ),
        error_radius: Some(grb_error_radius),
    };

    info!(
        "Simulating Fermi GRB 30s after GW: {}, trigger_time={}, position=(RA={:.2}, Dec={:.2}), error={:.2}°",
        fermi_event.trigger_id,
        fermi_event.trigger_time,
        grb_ra,
        grb_dec,
        grb_error_radius
    );

    // Download and save skymap
    if let Some(ref skymap_url) = fermi_event.skymap_url {
        match skymap_storage
            .download_skymap(skymap_url, &fermi_event.trigger_id, &fermi_event.instrument)
            .await
        {
            Ok(skymap_path) => {
                info!("Fermi skymap saved to: {:?}", skymap_path);
            }
            Err(e) => {
                error!("Failed to download Fermi skymap: {}", e);
            }
        }
    }

    // Process Fermi GRB through correlator (should associate with GW superevent)
    info!("Processing Fermi GRB through correlator");
    let grb_superevent_ids = correlator.process_gcn_event(Event::GammaRay(fermi_event.clone()))?;
    info!("GRB associated with superevents: {:?}", grb_superevent_ids);

    // Print multi-messenger superevents
    let mm_superevents = correlator.get_mm_superevents();
    info!("\n=== Multi-Messenger Superevents ===");
    info!("Total multi-messenger superevents: {}", mm_superevents.len());

    for superevent in mm_superevents {
        info!("\nSuperevent {}: ", superevent.id);
        info!("  t_0: {} (GPS)", superevent.t_0);
        info!("  Classification: {:?}", superevent.classification);

        if let Some(gw) = &superevent.gw_event {
            info!("  GW event: {}", gw.superevent_id);
        }

        if !superevent.gamma_ray_candidates.is_empty() {
            info!("  Gamma-ray candidates:");
            for grb in &superevent.gamma_ray_candidates {
                info!(
                    "    - {}, time_offset={:.2}s",
                    grb.trigger_id, grb.time_offset
                );
                if let Some(spatial_offset) = grb.spatial_offset {
                    info!("      Spatial offset: {:.2}°", spatial_offset);
                }
            }
        }

        info!(
            "  Optical candidates: {}",
            superevent.optical_candidates.len()
        );
    }

    // List all stored skymaps
    info!("\n=== Stored Skymaps ===");
    let stored_skymaps = skymap_storage.list_skymaps()?;
    info!("Total skymaps in storage: {}", stored_skymaps.len());

    for (instrument, event_id) in stored_skymaps {
        info!("  {} - {}", instrument, event_id);
    }

    Ok(())
}
