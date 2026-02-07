use apache_avro::Reader;
use mm_core::{LightCurve, Photometry, SkyPosition};
use serde::Deserialize;
use std::error::Error;
use tracing::{debug, warn};

/// Parse BOOM alert from Avro payload
pub fn parse_boom_alert(payload: &[u8]) -> Result<BoomAlert, Box<dyn Error>> {
    let reader = Reader::new(payload)?;

    for value in reader {
        let value = value?;

        // Try to deserialize as BOOM alert structure
        if let apache_avro::types::Value::Record(fields) = value {
            return parse_avro_record(fields);
        }
    }

    Err("No valid Avro record found".into())
}

fn parse_avro_record(
    fields: Vec<(String, apache_avro::types::Value)>,
) -> Result<BoomAlert, Box<dyn Error>> {
    use apache_avro::types::Value;

    let mut object_id: Option<String> = None;
    let mut candid: Option<i64> = None;
    let mut ra: Option<f64> = None;
    let mut dec: Option<f64> = None;
    let mut jd: Option<f64> = None;
    let mut magpsf: Option<f32> = None;
    let mut sigmapsf: Option<f32> = None;
    let mut fid: Option<i32> = None;
    let mut drb: Option<f32> = None;
    let mut photometry: Vec<PhotometryData> = Vec::new();
    let mut classifications: Vec<Classification> = Vec::new();

    for (name, value) in fields {
        match name.as_str() {
            "objectId" => {
                if let Value::String(s) = value {
                    object_id = Some(s);
                }
            }
            "candid" => {
                if let Value::Long(l) = value {
                    candid = Some(l);
                }
            }
            "ra" => {
                if let Value::Double(d) = value {
                    ra = Some(d);
                }
            }
            "dec" => {
                if let Value::Double(d) = value {
                    dec = Some(d);
                }
            }
            "jd" => {
                if let Value::Double(d) = value {
                    jd = Some(d);
                }
            }
            "magpsf" => {
                if let Value::Float(f) = value {
                    magpsf = Some(f);
                }
            }
            "sigmapsf" => {
                if let Value::Float(f) = value {
                    sigmapsf = Some(f);
                }
            }
            "fid" => {
                if let Value::Int(i) = value {
                    fid = Some(i);
                }
            }
            "drb" => {
                if let Value::Float(f) = value {
                    drb = Some(f);
                }
            }
            "prv_candidates" => {
                if let Value::Array(arr) = value {
                    photometry = parse_photometry_array(arr);
                }
            }
            "classifications" => {
                if let Value::Record(class_fields) = value {
                    classifications = parse_classifications(class_fields);
                }
            }
            _ => {
                debug!("Ignoring field: {}", name);
            }
        }
    }

    Ok(BoomAlert {
        object_id: object_id.ok_or("Missing objectId")?,
        candid: candid.ok_or("Missing candid")?,
        ra: ra.ok_or("Missing ra")?,
        dec: dec.ok_or("Missing dec")?,
        jd: jd.ok_or("Missing jd")?,
        magpsf,
        sigmapsf,
        fid: fid.unwrap_or(1),
        drb: drb.unwrap_or(0.0),
        photometry,
        classifications,
    })
}

fn parse_photometry_array(arr: Vec<apache_avro::types::Value>) -> Vec<PhotometryData> {
    use apache_avro::types::Value;

    let mut result = Vec::new();

    for item in arr {
        if let Value::Record(fields) = item {
            let mut jd: Option<f64> = None;
            let mut magpsf: Option<f32> = None;
            let mut sigmapsf: Option<f32> = None;
            let mut fid: Option<i32> = None;

            for (name, value) in fields {
                match name.as_str() {
                    "jd" => {
                        if let Value::Double(d) = value {
                            jd = Some(d);
                        }
                    }
                    "magpsf" => {
                        if let Value::Float(f) = value {
                            magpsf = Some(f);
                        } else if let Value::Union(_idx, boxed) = value {
                            if let Value::Float(f) = *boxed {
                                magpsf = Some(f);
                            }
                        }
                    }
                    "sigmapsf" => {
                        if let Value::Float(f) = value {
                            sigmapsf = Some(f);
                        } else if let Value::Union(_idx, boxed) = value {
                            if let Value::Float(f) = *boxed {
                                sigmapsf = Some(f);
                            }
                        }
                    }
                    "fid" => {
                        if let Value::Int(i) = value {
                            fid = Some(i);
                        }
                    }
                    _ => {}
                }
            }

            if let (Some(jd), Some(fid)) = (jd, fid) {
                result.push(PhotometryData {
                    jd,
                    magpsf,
                    sigmapsf,
                    fid,
                });
            }
        }
    }

    result
}

fn parse_classifications(fields: Vec<(String, apache_avro::types::Value)>) -> Vec<Classification> {
    use apache_avro::types::Value;

    let mut result = Vec::new();

    for (classifier, value) in fields {
        if let Value::Float(score) = value {
            result.push(Classification {
                classifier,
                score: score as f64,
            });
        } else if let Value::Double(score) = value {
            result.push(Classification { classifier, score });
        }
    }

    result
}

/// BOOM alert structure
#[derive(Debug, Clone)]
pub struct BoomAlert {
    pub object_id: String,
    pub candid: i64,
    pub ra: f64,
    pub dec: f64,
    pub jd: f64,
    pub magpsf: Option<f32>,
    pub sigmapsf: Option<f32>,
    pub fid: i32,
    pub drb: f32,
    pub photometry: Vec<PhotometryData>,
    pub classifications: Vec<Classification>,
}

#[derive(Debug, Clone)]
pub struct PhotometryData {
    pub jd: f64,
    pub magpsf: Option<f32>,
    pub sigmapsf: Option<f32>,
    pub fid: i32,
}

#[derive(Debug, Clone)]
pub struct Classification {
    pub classifier: String,
    pub score: f64,
}

impl BoomAlert {
    /// Convert BOOM alert to LightCurve
    pub fn to_lightcurve(&self) -> LightCurve {
        let mut lc = LightCurve::new(self.object_id.clone());

        // Add current detection
        if let (Some(mag), Some(magerr)) = (self.magpsf, self.sigmapsf) {
            let flux = mag_to_flux(mag);
            let flux_err = mag_to_flux(mag + magerr) - flux;
            lc.add_measurement(Photometry::new(
                self.jd,
                flux,
                flux_err,
                fid_to_band(self.fid),
            ));
        }

        // Add previous photometry
        for phot in &self.photometry {
            if let (Some(mag), Some(magerr)) = (phot.magpsf, phot.sigmapsf) {
                let flux = mag_to_flux(mag);
                let flux_err = mag_to_flux(mag + magerr) - flux;
                lc.add_measurement(Photometry::new(
                    phot.jd,
                    flux,
                    flux_err,
                    fid_to_band(phot.fid),
                ));
            }
        }

        lc
    }

    /// Get sky position
    pub fn position(&self) -> SkyPosition {
        SkyPosition::new(self.ra, self.dec, 2.0) // ZTF: ~2 arcsec
    }
}

/// Convert ZTF magnitude to flux (nanoJansky)
fn mag_to_flux(mag: f32) -> f64 {
    let mag = mag as f64;
    3631.0 * 1e9 * 10_f64.powf(-mag / 2.5)
}

/// Convert ZTF filter ID to band name
fn fid_to_band(fid: i32) -> String {
    match fid {
        1 => "g".to_string(),
        2 => "r".to_string(),
        3 => "i".to_string(),
        _ => format!("fid{}", fid),
    }
}
