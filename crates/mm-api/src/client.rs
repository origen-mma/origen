//! HTTP client for publishing events to the mm-api server

use anyhow::Result;
use reqwest::Client;
use serde_json::json;

pub struct ApiClient {
    client: Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// Publish a GW event with skymap
    pub async fn publish_gw_event(
        &self,
        event_id: &str,
        gpstime: f64,
        ra: f64,
        dec: f64,
        snr: f64,
        far: f64,
        skymap_data: Option<Vec<u8>>,
    ) -> Result<()> {
        let event = json!({
            "event_id": event_id,
            "gpstime": gpstime,
            "ra": ra,
            "dec": dec,
            "snr": snr,
            "far": far,
            "skymap_url": skymap_data.as_ref().map(|_| format!("/api/skymaps/{}", event_id)),
            "grb_detections": [],
            "optical_detections": [],
        });

        // Publish event metadata
        let url = format!("{}/api/events", self.base_url);
        self.client
            .post(&url)
            .json(&event)
            .send()
            .await?;

        // Publish skymap if provided
        if let Some(data) = skymap_data {
            let url = format!("{}/api/skymaps/{}", self.base_url, event_id);
            self.client
                .post(&url)
                .header("Content-Type", "application/fits")
                .body(data)
                .send()
                .await?;
        }

        Ok(())
    }

    /// Add GRB detection to an event
    pub async fn add_grb_detection(
        &self,
        event_id: &str,
        detection_time: f64,
        ra: f64,
        dec: f64,
        instrument: &str,
        fluence: f64,
        error_radius: f64,
    ) -> Result<()> {
        let detection = json!({
            "detection_time": detection_time,
            "ra": ra,
            "dec": dec,
            "instrument": instrument,
            "fluence": fluence,
            "error_radius": error_radius,
        });

        let url = format!("{}/api/events/{}/grb", self.base_url, event_id);
        self.client
            .post(&url)
            .json(&detection)
            .send()
            .await?;

        Ok(())
    }

    /// Add optical detection to an event
    pub async fn add_optical_detection(
        &self,
        event_id: &str,
        detection_time: f64,
        ra: f64,
        dec: f64,
        magnitude: f64,
        survey: &str,
        transient_type: &str,
    ) -> Result<()> {
        let detection = json!({
            "detection_time": detection_time,
            "ra": ra,
            "dec": dec,
            "magnitude": magnitude,
            "survey": survey,
            "transient_type": transient_type,
        });

        let url = format!("{}/api/events/{}/optical", self.base_url, event_id);
        self.client
            .post(&url)
            .json(&detection)
            .send()
            .await?;

        Ok(())
    }

    /// Check if server is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}
