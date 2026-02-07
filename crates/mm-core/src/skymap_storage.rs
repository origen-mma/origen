use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Skymap storage manager for saving and loading HEALPix FITS skymaps
pub struct SkymapStorage {
    storage_dir: PathBuf,
    client: reqwest::Client,
}

#[derive(Debug, Error)]
pub enum SkymapStorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("HTTP request failed after retries: {0}")]
    Http(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Download timeout")]
    Timeout,
}

impl SkymapStorage {
    /// Create new skymap storage with specified directory
    pub fn new<P: AsRef<Path>>(storage_dir: P) -> Result<Self, SkymapStorageError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();

        // Create storage directory if it doesn't exist
        if !storage_dir.exists() {
            fs::create_dir_all(&storage_dir)?;
        }

        // Create HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout
            .build()
            .map_err(|e| SkymapStorageError::Http(e.to_string()))?;

        Ok(Self {
            storage_dir,
            client,
        })
    }

    /// Download skymap from URL and save to storage
    pub async fn download_skymap(
        &self,
        url: &str,
        event_id: &str,
        instrument: &str,
    ) -> Result<PathBuf, SkymapStorageError> {
        // Create instrument-specific subdirectory
        let instrument_dir = self.storage_dir.join(instrument.to_lowercase());
        if !instrument_dir.exists() {
            fs::create_dir_all(&instrument_dir)?;
        }

        // Determine filename
        let filename = format!("{}.fits.gz", event_id);
        let filepath = instrument_dir.join(&filename);

        // Skip download if file already exists
        if filepath.exists() {
            tracing::info!("Skymap already exists: {:?}", filepath);
            return Ok(filepath);
        }

        // Download skymap with retry logic
        tracing::info!("Downloading skymap from: {}", url);

        let max_retries = 3;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            match self.download_with_retry(url, &filepath).await {
                Ok(_) => {
                    tracing::info!("Successfully downloaded skymap to: {:?}", filepath);
                    return Ok(filepath);
                }
                Err(e) => {
                    tracing::warn!("Download attempt {}/{} failed: {}", attempt, max_retries, e);
                    last_error = Some(e);

                    if attempt < max_retries {
                        // Wait before retry (exponential backoff)
                        let wait_secs = 2u64.pow(attempt - 1);
                        tracing::info!("Retrying in {} seconds...", wait_secs);
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SkymapStorageError::Http("Download failed after all retries".to_string())
        }))
    }

    /// Internal helper for single download attempt
    async fn download_with_retry(
        &self,
        url: &str,
        filepath: &Path,
    ) -> Result<(), SkymapStorageError> {
        // Send HTTP GET request
        let response = self.client.get(url).send().await?;

        // Check if successful
        if !response.status().is_success() {
            return Err(SkymapStorageError::Http(format!(
                "HTTP {} {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Get content length for progress tracking
        let total_size = response.content_length();
        if let Some(size) = total_size {
            tracing::info!("Downloading {} bytes", size);
        }

        // Download and write to file
        let bytes = response.bytes().await?;
        fs::write(filepath, &bytes)?;

        tracing::info!("Downloaded {} bytes to {:?}", bytes.len(), filepath);
        Ok(())
    }

    /// Get path to stored skymap
    pub fn get_skymap_path(&self, event_id: &str, instrument: &str) -> PathBuf {
        let instrument_dir = self.storage_dir.join(instrument.to_lowercase());
        let filename = format!("{}.fits.gz", event_id);
        instrument_dir.join(filename)
    }

    /// Check if skymap exists in storage
    pub fn has_skymap(&self, event_id: &str, instrument: &str) -> bool {
        self.get_skymap_path(event_id, instrument).exists()
    }

    /// List all stored skymaps
    pub fn list_skymaps(&self) -> Result<Vec<(String, String)>, SkymapStorageError> {
        let mut skymaps = Vec::new();

        if !self.storage_dir.exists() {
            return Ok(skymaps);
        }

        for entry in fs::read_dir(&self.storage_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let instrument = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                for skymap_entry in fs::read_dir(&path)? {
                    let skymap_entry = skymap_entry?;
                    let skymap_path = skymap_entry.path();

                    if let Some(filename) = skymap_path.file_stem() {
                        let event_id = filename.to_str().unwrap_or("unknown").to_string();
                        skymaps.push((instrument.clone(), event_id));
                    }
                }
            }
        }

        Ok(skymaps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_skymap_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SkymapStorage::new(temp_dir.path()).unwrap();

        assert!(temp_dir.path().exists());
    }

    #[test]
    fn test_skymap_path() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SkymapStorage::new(temp_dir.path()).unwrap();

        let path = storage.get_skymap_path("GRB240101A", "Fermi-GBM");
        assert!(path.ends_with("fermi-gbm/GRB240101A.fits.gz"));
    }

    #[test]
    fn test_has_skymap() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SkymapStorage::new(temp_dir.path()).unwrap();

        assert!(!storage.has_skymap("GRB240101A", "Fermi-GBM"));

        // Create a dummy skymap
        let skymap_dir = temp_dir.path().join("fermi-gbm");
        fs::create_dir_all(&skymap_dir).unwrap();
        fs::write(skymap_dir.join("GRB240101A.fits.gz"), b"test").unwrap();

        assert!(storage.has_skymap("GRB240101A", "Fermi-GBM"));
    }
}
