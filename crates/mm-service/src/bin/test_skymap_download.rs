use mm_core::SkymapStorage;
use tracing::info;

/// Quick test to verify HTTP download works with a real small file
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("Testing skymap HTTP download...");

    // Create temporary storage
    let storage = SkymapStorage::new("./test_downloads")?;

    // Test with a small file from httpbin (returns JSON, but that's fine for testing)
    let test_url = "https://httpbin.org/bytes/1024";

    info!("Downloading test file from: {}", test_url);

    match storage.download_skymap(test_url, "test_event", "test-instrument").await {
        Ok(path) => {
            info!("✓ Download successful!");
            info!("  Saved to: {:?}", path);

            // Check file size
            let metadata = std::fs::metadata(&path)?;
            info!("  File size: {} bytes", metadata.len());

            // Cleanup
            std::fs::remove_dir_all("./test_downloads")?;
            info!("✓ Test passed!");
            Ok(())
        }
        Err(e) => {
            info!("✗ Download failed: {}", e);
            Err(e.into())
        }
    }
}
