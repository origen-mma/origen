use mm_boom::BoomSimulator;
use mm_config::Config;
use mm_core::{
    estimate_explosion_time, Event, GWEvent, GammaRayEvent, GpsTime, MockSkymap, SkyPosition,
};
use mm_correlator::SupereventCorrelator;
use std::env;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("Starting Multi-Messenger Correlator Demo");

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

    // Load ZTF alerts first to get realistic time range
    let ztf_dir = &app_config.simulation.ztf_csv_dir;
    info!("Loading ZTF light curves from: {}", ztf_dir);

    // Create a mock GW skymap - typical neutron star merger at random sky position
    let gw_ra = 180.0;
    let gw_dec = 45.0;
    let skymap = MockSkymap::typical_ns_merger(gw_ra, gw_dec);

    info!(
        "Created mock GW skymap: center=(RA={:.1}°, Dec={:.1}°), 50% CR={:.1}°, 90% CR={:.1}°",
        skymap.center_ra, skymap.center_dec, skymap.radius_50, skymap.radius_90
    );

    // Get GW position before moving the skymap
    let gw_skymap_center = skymap.center();

    let mut simulator = BoomSimulator::from_directory_with_skymap(ztf_dir, 0, Some(skymap))?;

    info!("Loaded {} light curves", simulator.len());

    // Create correlator with RAVEN configuration
    let mut correlator = SupereventCorrelator::new_raven();

    // Get first light curve to determine realistic time range
    let first_lc = simulator.next().expect("Need at least one light curve");

    // Estimate explosion time from first light curve
    let estimated_t0 = estimate_explosion_time(first_lc).unwrap_or_else(|| {
        first_lc
            .measurements
            .first()
            .map(|m| m.to_gps_time())
            .unwrap_or(0.0)
    });

    let first_detection = first_lc
        .measurements
        .first()
        .map(|m| m.to_gps_time())
        .unwrap_or(0.0);

    info!("First light curve: {}", first_lc.object_id);
    info!(
        "  First detection: GPS {:.2} (MJD {:.2})",
        first_detection, first_lc.measurements[0].mjd
    );
    info!("  Estimated explosion time: GPS {:.2}", estimated_t0);
    info!(
        "  Rise time estimate: {:.2} hours",
        (first_detection - estimated_t0) / 3600.0
    );

    // Set GW trigger time to be near the estimated explosion time
    let gw_gps_time = estimated_t0 - 1800.0; // 30 minutes before estimated explosion

    info!("Setting GW trigger time to GPS {:.2}", gw_gps_time);
    info!("  (30 minutes before estimated explosion time for realistic correlation)");

    // For demo, add a synthetic GW event with realistic time and position
    let gw_event = GWEvent {
        superevent_id: "S240101a".to_string(),
        alert_type: "PRELIMINARY".to_string(),
        gps_time: GpsTime::from_seconds(gw_gps_time),
        instruments: vec!["H1".to_string(), "L1".to_string()],
        far: 1e-10,
        position: Some(gw_skymap_center.clone()), // Use actual GW position for spatial correlation
        skymap: None,
    };

    info!("Processing synthetic GW event: {}", gw_event.superevent_id);
    info!("  GW position: RA={:.2}°, Dec={:.2}°", gw_ra, gw_dec);
    let superevent_ids = correlator.process_gcn_event(Event::GravitationalWave(gw_event))?;
    info!("Created superevents: {:?}", superevent_ids);

    // Also add a simulated Fermi GRB 30 seconds after the GW
    let grb_trigger_time = gw_gps_time + 30.0;
    let grb_ra = gw_ra + 2.0; // 2 degrees offset
    let grb_dec = gw_dec + 1.0; // 1 degree offset
    let grb_event = GammaRayEvent {
        trigger_id: "GRB240101A".to_string(),
        instrument: "Fermi-GBM".to_string(),
        trigger_time: grb_trigger_time,
        position: Some(SkyPosition::new(grb_ra, grb_dec, 5.0 * 3600.0)),
        significance: 8.2,
        skymap_url: Some("https://example.com/grb_skymap.fits".to_string()),
        error_radius: Some(5.0),
    };

    info!("Processing synthetic Fermi GRB: {}", grb_event.trigger_id);
    info!(
        "  GRB position: RA={:.2}°, Dec={:.2}° (offset from GW)",
        grb_ra, grb_dec
    );
    let grb_superevent_ids = correlator.process_gcn_event(Event::GammaRay(grb_event))?;
    info!("GRB associated with superevents: {:?}", grb_superevent_ids);

    // Create new simulator with skymap for processing all light curves
    let skymap = MockSkymap::typical_ns_merger(gw_ra, gw_dec);
    let mut simulator = BoomSimulator::from_directory_with_skymap(ztf_dir, 0, Some(skymap))?;

    let mut matched_count = 0;
    let mut processed_count = 0;
    let mut spatial_offsets = Vec::new();

    simulator
        .stream(|lightcurve, position, explosion_time_gps| {
            processed_count += 1;

            // Process through correlator using estimated explosion time
            // Note: For now we still pass the full light curve, but the explosion_time_gps
            // gives us better timing information
            match correlator.process_optical_lightcurve(lightcurve, position) {
                Ok(matches) => {
                    if !matches.is_empty() {
                        matched_count += 1;

                        // Calculate spatial offset from GW center
                        let gw_center = MockSkymap::typical_ns_merger(gw_ra, gw_dec).center();
                        let spatial_offset = position.angular_separation(&gw_center);
                        spatial_offsets.push(spatial_offset);

                        let time_since_explosion = lightcurve
                            .measurements
                            .first()
                            .map(|m| m.to_gps_time() - explosion_time_gps)
                            .unwrap_or(0.0);

                        info!(
                            "Optical match! Object: {}, Matches: {:?}",
                            lightcurve.object_id, matches
                        );
                        info!(
                            "  Explosion t0: GPS {:.2}, First detection: {:.2} hrs after",
                            explosion_time_gps,
                            time_since_explosion / 3600.0
                        );
                        info!(
                            "  Position: RA={:.2}°, Dec={:.2}°, Offset from GW={:.2}°",
                            position.ra, position.dec, spatial_offset
                        );

                        // Print superevent details
                        for superevent_id in matches {
                            if let Some(superevent) = correlator.get_superevent(&superevent_id) {
                                info!(
                                    "  Superevent {}: {} optical candidates, classification: {:?}",
                                    superevent_id,
                                    superevent.optical_candidates.len(),
                                    superevent.classification
                                );
                            }
                        }
                    }

                    if processed_count % 100 == 0 {
                        info!("Processed {} optical alerts", processed_count);
                    }

                    Ok(())
                }
                Err(e) => {
                    error!("Error processing {}: {}", lightcurve.object_id, e);
                    Err(e.into())
                }
            }
        })
        .await?;

    // Print final statistics
    info!("\n=== Final Statistics ===");
    let stats = correlator.stats();
    info!("Total superevents: {}", stats.total_superevents);
    info!("GW-only: {}", stats.gw_only);
    info!("With optical: {}", stats.with_optical);
    info!("Optical alerts processed: {}", processed_count);
    info!("Optical matches found: {}", matched_count);

    // Analyze spatial distribution
    if !spatial_offsets.is_empty() {
        spatial_offsets.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_offset = spatial_offsets[spatial_offsets.len() / 2];
        let within_50 = spatial_offsets.iter().filter(|&&x| x <= 1.5).count();
        let within_90 = spatial_offsets.iter().filter(|&&x| x <= 3.0).count();

        info!("\n=== Spatial Distribution ===");
        info!("Median offset from GW center: {:.2}°", median_offset);
        info!(
            "Within 50% CR (1.5°): {}/{} ({:.1}%)",
            within_50,
            spatial_offsets.len(),
            100.0 * within_50 as f64 / spatial_offsets.len() as f64
        );
        info!(
            "Within 90% CR (3.0°): {}/{} ({:.1}%)",
            within_90,
            spatial_offsets.len(),
            100.0 * within_90 as f64 / spatial_offsets.len() as f64
        );
    }

    // Print sample multi-messenger superevents
    let mm_superevents = correlator.get_mm_superevents();
    if !mm_superevents.is_empty() {
        info!("\n=== Multi-Messenger Superevents (first 3) ===");
        for superevent in mm_superevents.iter().take(3) {
            info!("Superevent {}: ", superevent.id);
            if let Some(gw) = &superevent.gw_event {
                info!("  GW event: {}", gw.superevent_id);
            }
            info!("  t_0: {} (GPS)", superevent.t_0);
            info!("  Classification: {:?}", superevent.classification);

            // Show GRB candidates if any
            if !superevent.gamma_ray_candidates.is_empty() {
                info!(
                    "  Gamma-ray candidates: {}",
                    superevent.gamma_ray_candidates.len()
                );
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

            // Show first 5 candidates
            for (i, candidate) in superevent.optical_candidates.iter().take(5).enumerate() {
                info!("    {}. Object: {}", i + 1, candidate.object_id);
                info!(
                    "       Time offset: {:.2} s ({:.2} hrs)",
                    candidate.time_offset,
                    candidate.time_offset / 3600.0
                );
                info!("       Spatial offset: {:.2} deg", candidate.spatial_offset);
                info!("       SNR: {:.2}", candidate.significance);
                if let Some(far) = candidate.joint_far {
                    info!("       Joint FAR: {:.2e} /yr", far);
                }
            }
            if superevent.optical_candidates.len() > 5 {
                info!(
                    "    ... and {} more candidates",
                    superevent.optical_candidates.len() - 5
                );
            }
        }
    }

    Ok(())
}
