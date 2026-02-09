//! REST API for serving multi-messenger event data and skymaps to Grafana

pub mod client;

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use actix_cors::Cors;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;

/// Multi-messenger event with skymap reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MMEvent {
    pub event_id: String,
    pub gpstime: f64,
    pub ra: f64,
    pub dec: f64,
    pub skymap_url: Option<String>,
    pub snr: f64,
    pub far: f64,
    
    // Associated counterparts
    pub grb_detections: Vec<GrbDetection>,
    pub optical_detections: Vec<OpticalDetection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrbDetection {
    pub detection_time: f64,
    pub ra: f64,
    pub dec: f64,
    pub instrument: String,
    pub fluence: f64,
    pub error_radius: f64, // degrees (90% containment)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalDetection {
    pub detection_time: f64,
    pub ra: f64,
    pub dec: f64,
    pub magnitude: f64,
    pub survey: String,
    pub transient_type: String,
}

/// Shared state for API
pub struct ApiState {
    pub events: Arc<Mutex<HashMap<String, MMEvent>>>,
    pub skymaps: Arc<Mutex<HashMap<String, Vec<u8>>>>,  // event_id -> FITS data
    pub skymap_dir: Option<PathBuf>,  // Optional directory for storing skymap files
}

/// Get all recent events
#[get("/api/events")]
async fn get_events(state: web::Data<ApiState>) -> impl Responder {
    let events = state.events.lock().unwrap();
    let event_list: Vec<_> = events.values().cloned().collect();
    HttpResponse::Ok().json(event_list)
}

/// Get specific event by ID
#[get("/api/events/{event_id}")]
async fn get_event(
    event_id: web::Path<String>,
    state: web::Data<ApiState>,
) -> impl Responder {
    let events = state.events.lock().unwrap();
    
    match events.get(event_id.as_str()) {
        Some(event) => HttpResponse::Ok().json(event),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Event not found"
        })),
    }
}

/// Health check endpoint
#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "mm-api"
    }))
}

/// Serve the skymap dashboard HTML
#[get("/")]
async fn serve_dashboard() -> impl Responder {
    let html = include_str!("../../../skymap_dashboard.html");
    HttpResponse::Ok()
        .content_type("text/html")
        .body(html)
}

/// Get skymap FITS file for an event
#[get("/api/skymaps/{event_id}")]
async fn get_skymap(
    event_id: web::Path<String>,
    state: web::Data<ApiState>,
) -> impl Responder {
    use tracing::{info, warn};

    // Try to load from disk if skymap_dir is configured
    if let Some(ref skymap_dir) = state.skymap_dir {
        info!("Skymap directory configured: {}", skymap_dir.display());
        // Map event_id like "G1" to file "0.fits" (G1 -> 0, G2 -> 1, etc.)
        if event_id.starts_with("G") {
            if let Ok(num) = event_id[1..].parse::<usize>() {
                let filename = format!("{}.fits", num - 1);  // G1 = 0.fits
                let filepath = skymap_dir.join(&filename);
                info!("Attempting to read skymap from: {}", filepath.display());

                match tokio::fs::read(&filepath).await {
                    Ok(fits_data) => {
                        info!("Successfully read skymap: {} bytes", fits_data.len());
                        return HttpResponse::Ok()
                            .content_type("application/fits")
                            .append_header(("Content-Disposition", format!("attachment; filename=\"{}.fits\"", event_id)))
                            .body(fits_data);
                    }
                    Err(e) => {
                        warn!("Failed to read skymap from {}: {}", filepath.display(), e);
                    }
                }
            }
        }
    } else {
        info!("No skymap directory configured");
    }

    // Fall back to memory storage
    let skymaps = state.skymaps.lock().unwrap();
    match skymaps.get(event_id.as_str()) {
        Some(fits_data) => HttpResponse::Ok()
            .content_type("application/fits")
            .append_header(("Content-Disposition", format!("attachment; filename=\"{}.fits\"", event_id)))
            .body(fits_data.clone()),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Skymap not found"
        })),
    }
}

/// Contour response structure
#[derive(Debug, Serialize)]
struct ContourResponse {
    level: f64,
    area_deg2: f64,
    pixels: Vec<ContourPixel>,
}

#[derive(Debug, Serialize)]
struct ContourPixel {
    ra: f64,
    dec: f64,
}

/// Get skymap credible region contours
#[get("/api/skymaps/{event_id}/contours")]
async fn get_skymap_contours(
    event_id: web::Path<String>,
    state: web::Data<ApiState>,
) -> impl Responder {
    use tracing::{info, warn};
    use mm_core::ParsedSkymap;

    // Try to load from disk if skymap_dir is configured
    if let Some(ref skymap_dir) = state.skymap_dir {
        if event_id.starts_with("G") {
            if let Ok(num) = event_id[1..].parse::<usize>() {
                let filename = format!("{}.fits", num - 1);
                let filepath = skymap_dir.join(&filename);

                info!("Extracting contours from: {}", filepath.display());

                // Parse in blocking task
                let filepath_clone = filepath.clone();
                match tokio::task::spawn_blocking(move || {
                    ParsedSkymap::from_fits(&filepath_clone)
                }).await {
                    Ok(Ok(skymap)) => {
                        info!("Parsed skymap: {} credible regions, NSIDE={}",
                              skymap.credible_regions.len(), skymap.nside);

                        // Convert credible regions to contour format
                        let contours = skymap_to_contours(&skymap);
                        return HttpResponse::Ok().json(contours);
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to parse skymap: {}", e);
                        return HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": format!("Failed to parse skymap: {}", e)
                        }));
                    }
                    Err(e) => {
                        warn!("Task error: {}", e);
                        return HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": format!("Task error: {}", e)
                        }));
                    }
                }
            }
        }
    }

    HttpResponse::NotFound().json(serde_json::json!({
        "error": "Skymap not found"
    }))
}

/// Convert ParsedSkymap to contour response format
fn skymap_to_contours(skymap: &mm_core::ParsedSkymap) -> Vec<ContourResponse> {
    use cdshealpix::nested::center;

    let depth = (skymap.nside as f64).log2() as u8;

    skymap.credible_regions.iter().map(|region| {
        // Sample evenly across ALL pixels to show the full banana shape
        // Take every Nth pixel instead of just the first N highest-probability ones
        let total_pixels = region.pixel_indices.len();
        let sample_rate = if region.level <= 0.5 {
            (total_pixels / 1500).max(1) // 50% region: ~1500 pixels
        } else {
            (total_pixels / 2000).max(1) // 90% region: ~2000 pixels
        };

        let pixels: Vec<ContourPixel> = region.pixel_indices.iter()
            .enumerate()
            .filter(|(i, _)| i % sample_rate == 0)
            .map(|(_, &idx)| {
                let hash = idx as u64;
                let (lon, lat) = center(depth, hash);
                ContourPixel {
                    ra: lon.to_degrees(),
                    dec: lat.to_degrees(),
                }
            })
            .collect();

        ContourResponse {
            level: region.level,
            area_deg2: region.area,
            pixels,
        }
    }).collect()
}

/// Get skymap as MOC FITS file (Aladin can load FITS directly)
#[get("/api/skymaps/{event_id}/moc")]
async fn get_skymap_moc(
    event_id: web::Path<String>,
    state: web::Data<ApiState>,
) -> impl Responder {
    use tracing::info;

    // Try to load from disk if skymap_dir is configured
    if let Some(ref skymap_dir) = state.skymap_dir {
        if event_id.starts_with("G") {
            if let Ok(num) = event_id[1..].parse::<usize>() {
                let filename = format!("{}.fits", num - 1);
                let filepath = skymap_dir.join(&filename);

                info!("Serving MOC FITS file: {}", filepath.display());

                // Serve the FITS file directly - Aladin can load it
                match tokio::fs::read(&filepath).await {
                    Ok(fits_data) => {
                        return HttpResponse::Ok()
                            .content_type("application/fits")
                            .append_header(("Content-Disposition", format!("attachment; filename=\"{}_moc.fits\"", event_id)))
                            .body(fits_data);
                    }
                    Err(e) => {
                        return HttpResponse::NotFound().json(serde_json::json!({
                            "error": format!("FITS file not found: {}", e)
                        }));
                    }
                }
            }
        }
    }

    HttpResponse::NotFound().json(serde_json::json!({
        "error": "Skymap not found"
    }))
}

/// Convert ParsedSkymap credible regions to MOC JSON format
/// Create or update an event
#[post("/api/events")]
async fn post_event(
    event: web::Json<MMEvent>,
    state: web::Data<ApiState>,
) -> impl Responder {
    let mut events = state.events.lock().unwrap();
    events.insert(event.event_id.clone(), event.into_inner());
    HttpResponse::Ok().json(serde_json::json!({
        "status": "created"
    }))
}

/// Add GRB detection to an event
#[post("/api/events/{event_id}/grb")]
async fn add_grb_detection(
    event_id: web::Path<String>,
    detection: web::Json<GrbDetection>,
    state: web::Data<ApiState>,
) -> impl Responder {
    let mut events = state.events.lock().unwrap();

    match events.get_mut(event_id.as_str()) {
        Some(event) => {
            event.grb_detections.push(detection.into_inner());
            HttpResponse::Ok().json(serde_json::json!({
                "status": "added"
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Event not found"
        })),
    }
}

/// Add optical detection to an event
#[post("/api/events/{event_id}/optical")]
async fn add_optical_detection(
    event_id: web::Path<String>,
    detection: web::Json<OpticalDetection>,
    state: web::Data<ApiState>,
) -> impl Responder {
    let mut events = state.events.lock().unwrap();

    match events.get_mut(event_id.as_str()) {
        Some(event) => {
            event.optical_detections.push(detection.into_inner());
            HttpResponse::Ok().json(serde_json::json!({
                "status": "added"
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Event not found"
        })),
    }
}

/// Upload skymap FITS data
#[post("/api/skymaps/{event_id}")]
async fn post_skymap(
    event_id: web::Path<String>,
    body: web::Bytes,
    state: web::Data<ApiState>,
) -> impl Responder {
    let mut skymaps = state.skymaps.lock().unwrap();
    skymaps.insert(event_id.to_string(), body.to_vec());
    HttpResponse::Ok().json(serde_json::json!({
        "status": "uploaded"
    }))
}

/// Start the API server
pub async fn run_server(bind_addr: &str, state: ApiState) -> std::io::Result<()> {
    let state_data = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
            )
            .app_data(state_data.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))  // 10MB payload limit for skymaps
            .service(serve_dashboard)
            .service(health)
            .service(get_events)
            .service(get_event)
            .service(post_event)
            .service(add_grb_detection)
            .service(add_optical_detection)
            .service(get_skymap)
            .service(get_skymap_contours)
            .service(post_skymap)
            .service(get_skymap_moc)
    })
    .bind(bind_addr)?
    .run()
    .await
}
