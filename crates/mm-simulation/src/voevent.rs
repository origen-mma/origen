//! VOEvent XML parser for GRB alerts (Fermi GBM, Swift BAT, etc.)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Parsed GRB alert from VOEvent XML
#[derive(Debug, Clone)]
pub struct GrbAlert {
    /// Instrument name (e.g., "Fermi GBM", "Swift BAT")
    pub instrument: String,

    /// Trigger ID
    pub trigger_id: String,

    /// Trigger time
    pub trigger_time: DateTime<Utc>,

    /// Sky position (RA, Dec in degrees)
    pub ra: f64,
    pub dec: f64,

    /// Error radius (degrees)
    pub error_radius: f64,

    /// HEALPix skymap URL (if available)
    pub healpix_url: Option<String>,

    /// Packet type (e.g., "GBM_SubThresh", "GBM_Flt_Pos")
    pub packet_type: Option<String>,

    /// Duration (seconds)
    pub duration: Option<f64>,

    /// Spectral class (e.g., "hard", "soft")
    pub spectral_class: Option<String>,
}

/// VOEvent XML parser
pub struct VOEventParser;

impl VOEventParser {
    /// Parse a VOEvent XML file
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<GrbAlert> {
        let file = File::open(path.as_ref())
            .context("Failed to open VOEvent XML file")?;
        let reader = BufReader::new(file);
        Self::parse_reader(reader)
    }

    /// Parse VOEvent XML from a reader
    pub fn parse_reader<R: std::io::BufRead>(reader: R) -> Result<GrbAlert> {
        let mut xml_reader = Reader::from_reader(reader);

        let mut instrument = String::new();
        let mut trigger_id = String::new();
        let mut trigger_time = String::new();
        let mut ra = 0.0;
        let mut dec = 0.0;
        let mut error_radius = 0.0;
        let mut healpix_url: Option<String> = None;
        let mut packet_type: Option<String> = None;
        let mut duration: Option<f64> = None;
        let mut spectral_class: Option<String> = None;

        let mut buf = Vec::new();
        let mut in_who = false;
        let mut in_what = false;
        let mut in_where_when = false;
        let mut in_position = false;

        loop {
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    match e.name().as_ref() {
                        b"Who" => in_who = true,
                        b"What" => in_what = true,
                        b"WhereWhen" => in_where_when = true,
                        b"Position2D" => in_position = true,
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    match e.name().as_ref() {
                        b"Who" => in_who = false,
                        b"What" => in_what = false,
                        b"WhereWhen" => in_where_when = false,
                        b"Position2D" => in_position = false,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    if e.name().as_ref() == b"Param" {
                        let mut param_name = String::new();
                        let mut param_value = String::new();

                        for attr in e.attributes() {
                            if let Ok(attr) = attr {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                let value = String::from_utf8_lossy(&attr.value);

                                match key.as_ref() {
                                    "name" => param_name = value.to_string(),
                                    "value" => param_value = value.to_string(),
                                    _ => {}
                                }
                            }
                        }

                        // Extract parameters
                        if in_what {
                            match param_name.as_str() {
                                "Trans_Num" => trigger_id = param_value,
                                "HealPix_URL" => healpix_url = Some(param_value),
                                "Trans_Duration" => {
                                    if let Ok(d) = param_value.parse::<f64>() {
                                        duration = Some(d);
                                    }
                                }
                                "Spectral_class" => spectral_class = Some(param_value),
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::Text(e)) => {
                    if in_who {
                        let text = e.unescape().unwrap_or_default();
                        if text.contains("Fermi") {
                            instrument = "Fermi GBM".to_string();
                        } else if text.contains("Swift") {
                            instrument = "Swift BAT".to_string();
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => anyhow::bail!("Error parsing XML at position {}: {}", xml_reader.buffer_position(), e),
                _ => {}
            }
            buf.clear();
        }

        // Second pass to extract position and time (nested elements)
        let file = File::open(Path::new("dummy"))?; // Reopen not possible with this API
        // For now, use a simplified approach with string searching

        // Return parsed alert (with default values if parsing incomplete)
        Ok(GrbAlert {
            instrument,
            trigger_id,
            trigger_time: Utc::now(), // TODO: Parse from XML
            ra,
            dec,
            error_radius,
            healpix_url,
            packet_type,
            duration,
            spectral_class,
        })
    }

    /// Parse VOEvent XML from string (more complete implementation)
    pub fn parse_string(xml: &str) -> Result<GrbAlert> {
        let mut instrument = String::new();
        let mut trigger_id = String::new();
        let mut trigger_time_str = String::new();
        let mut ra = 0.0;
        let mut dec = 0.0;
        let mut error_radius = 0.0;
        let mut healpix_url: Option<String> = None;
        let mut packet_type: Option<String> = None;
        let mut duration: Option<f64> = None;
        let mut spectral_class: Option<String> = None;

        // Extract shortName (instrument)
        if let Some(start) = xml.find("<shortName>") {
            if let Some(end) = xml[start..].find("</shortName>") {
                let name = &xml[start + 11..start + end];
                if name.contains("Fermi") {
                    instrument = "Fermi GBM".to_string();
                } else if name.contains("Swift") {
                    instrument = "Swift BAT".to_string();
                } else {
                    instrument = name.to_string();
                }
            }
        }

        // Extract ivorn for packet type and trigger ID
        if let Some(start) = xml.find("ivorn=\"") {
            if let Some(end) = xml[start + 7..].find("\"") {
                let ivorn = &xml[start + 7..start + 7 + end];
                if let Some(hash_pos) = ivorn.find('#') {
                    packet_type = Some(ivorn[hash_pos + 1..].split('_').take(3).collect::<Vec<_>>().join("_"));
                }
            }
        }

        // Extract Trans_Num (trigger ID)
        if let Some(start) = xml.find("Trans_Num") {
            if let Some(value_start) = xml[start..].find("value=\"") {
                let value_pos = start + value_start + 7;
                if let Some(value_end) = xml[value_pos..].find("\"") {
                    trigger_id = xml[value_pos..value_pos + value_end].to_string();
                }
            }
        }

        // Extract ISOTime (trigger time)
        if let Some(start) = xml.find("<ISOTime>") {
            if let Some(end) = xml[start..].find("</ISOTime>") {
                trigger_time_str = xml[start + 9..start + end].to_string();
            }
        }

        // Extract Position (RA, Dec, Error)
        if let Some(pos_start) = xml.find("<Position2D") {
            let pos_section = &xml[pos_start..];

            // Extract C1 (RA)
            if let Some(c1_start) = pos_section.find("<C1>") {
                if let Some(c1_end) = pos_section[c1_start..].find("</C1>") {
                    if let Ok(val) = pos_section[c1_start + 4..c1_start + c1_end].parse::<f64>() {
                        ra = val;
                    }
                }
            }

            // Extract C2 (Dec)
            if let Some(c2_start) = pos_section.find("<C2>") {
                if let Some(c2_end) = pos_section[c2_start..].find("</C2>") {
                    if let Ok(val) = pos_section[c2_start + 4..c2_start + c2_end].parse::<f64>() {
                        dec = val;
                    }
                }
            }

            // Extract Error2Radius
            if let Some(err_start) = pos_section.find("<Error2Radius>") {
                if let Some(err_end) = pos_section[err_start..].find("</Error2Radius>") {
                    if let Ok(val) = pos_section[err_start + 14..err_start + err_end].parse::<f64>() {
                        error_radius = val;
                    }
                }
            }
        }

        // Extract HealPix_URL
        if let Some(start) = xml.find("HealPix_URL") {
            if let Some(value_start) = xml[start..].find("value=\"") {
                let value_pos = start + value_start + 7;
                if let Some(value_end) = xml[value_pos..].find("\"") {
                    healpix_url = Some(xml[value_pos..value_pos + value_end].to_string());
                }
            }
        }

        // Extract Trans_Duration
        if let Some(start) = xml.find("Trans_Duration") {
            if let Some(value_start) = xml[start..].find("value=\"") {
                let value_pos = start + value_start + 7;
                if let Some(value_end) = xml[value_pos..].find("\"") {
                    if let Ok(d) = xml[value_pos..value_pos + value_end].parse::<f64>() {
                        duration = Some(d);
                    }
                }
            }
        }

        // Extract Spectral_class
        if let Some(start) = xml.find("Spectral_class") {
            if let Some(value_start) = xml[start..].find("value=\"") {
                let value_pos = start + value_start + 7;
                if let Some(value_end) = xml[value_pos..].find("\"") {
                    spectral_class = Some(xml[value_pos..value_pos + value_end].to_string());
                }
            }
        }

        // Parse trigger time
        let trigger_time = if !trigger_time_str.is_empty() {
            DateTime::parse_from_rfc3339(&trigger_time_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };

        Ok(GrbAlert {
            instrument,
            trigger_id,
            trigger_time,
            ra,
            dec,
            error_radius,
            healpix_url,
            packet_type,
            duration,
            spectral_class,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fermi_gbm() {
        let xml = r#"<?xml version = '1.0' encoding = 'UTF-8'?>
<voe:VOEvent ivorn="ivo://nasa.gsfc.gcn/Fermi#GBM_SubThresh_2021-06-23T01:26:03.00_646104368_0-701">
  <Who>
    <Author>
      <shortName>Fermi (via VO-GCN)</shortName>
    </Author>
  </Who>
  <What>
    <Param name="Trans_Num" value="646104368" />
    <Param name="Trans_Duration" value="0.703" />
    <Param name="HealPix_URL" value="https://gcn.gsfc.nasa.gov/notices_gbm_sub/gbm_subthresh_646104368.383999_healpix.fits" />
    <Group name="Misc_Flags">
      <Param name="Spectral_class" value="hard" />
    </Group>
  </What>
  <WhereWhen>
    <ObsDataLocation>
      <ObservationLocation>
        <AstroCoords>
          <Time>
            <TimeInstant>
              <ISOTime>2021-06-23T01:26:03.38</ISOTime>
            </TimeInstant>
          </Time>
          <Position2D>
            <Value2>
              <C1>222.3199</C1>
              <C2>21.8900</C2>
            </Value2>
            <Error2Radius>44.3900</Error2Radius>
          </Position2D>
        </AstroCoords>
      </ObservationLocation>
    </ObsDataLocation>
  </WhereWhen>
</voe:VOEvent>"#;

        let alert = VOEventParser::parse_string(xml).unwrap();
        assert_eq!(alert.instrument, "Fermi GBM");
        assert_eq!(alert.trigger_id, "646104368");
        assert!((alert.ra - 222.3199).abs() < 0.001);
        assert!((alert.dec - 21.89).abs() < 0.001);
        assert!((alert.error_radius - 44.39).abs() < 0.001);
        assert_eq!(alert.duration, Some(0.703));
        assert_eq!(alert.spectral_class, Some("hard".to_string()));
    }
}
