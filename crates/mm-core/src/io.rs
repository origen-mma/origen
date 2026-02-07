use crate::{LightCurve, Photometry};
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct CsvRow {
    mjd: f64,
    flux: f64,
    flux_err: f64,
    filter: String,
}

/// Load a light curve from CSV file
/// Format: mjd,flux,flux_err,filter
pub fn load_lightcurve_csv<P: AsRef<Path>>(
    path: P,
) -> Result<LightCurve, Box<dyn Error>> {
    let file = File::open(&path)?;
    let mut rdr = csv::Reader::from_reader(file);

    // Extract object ID from filename (e.g., "ZTF25aaaalin.csv" -> "ZTF25aaaalin")
    let filename = path
        .as_ref()
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut lc = LightCurve::new(filename.to_string());

    for result in rdr.deserialize() {
        let row: CsvRow = result?;
        lc.add_measurement(Photometry::new(
            row.mjd,
            row.flux,
            row.flux_err,
            row.filter,
        ));
    }

    lc.sort_by_time();
    Ok(lc)
}

/// Load all light curves from a directory
pub fn load_lightcurves_dir<P: AsRef<Path>>(
    dir: P,
) -> Result<Vec<LightCurve>, Box<dyn Error>> {
    let mut lightcurves = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("csv") {
            match load_lightcurve_csv(&path) {
                Ok(lc) => lightcurves.push(lc),
                Err(e) => eprintln!("Error loading {:?}: {}", path, e),
            }
        }
    }

    Ok(lightcurves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_csv() {
        // Create temp CSV file
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "mjd,flux,flux_err,filter").unwrap();
        writeln!(file, "60675.1,100.0,5.0,g").unwrap();
        writeln!(file, "60675.2,150.0,10.0,r").unwrap();
        writeln!(file, "60675.3,120.0,8.0,g").unwrap();

        let lc = load_lightcurve_csv(file.path()).unwrap();
        assert_eq!(lc.measurements.len(), 3);
        assert_eq!(lc.filter_band("g").len(), 2);
    }
}
