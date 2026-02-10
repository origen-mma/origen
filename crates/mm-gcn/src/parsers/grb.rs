use mm_core::{Event, GammaRayEvent, ParseError, SkyPosition};
use serde::Deserialize;
use serde_json::Value;

/// Swift BAT GUANO alert format (simplified JSON for now)
/// Full VOEvent XML parsing will be added later
#[derive(Debug, Deserialize)]
struct SwiftBatAlert {
    #[serde(rename = "TrigID")]
    trigger_id: Option<String>,
    #[serde(rename = "TriggerTime")]
    trigger_time: Option<f64>,
    #[serde(rename = "RA")]
    ra: Option<f64>,
    #[serde(rename = "Dec")]
    dec: Option<f64>,
    #[serde(rename = "Error")]
    error: Option<f64>,
    #[serde(rename = "Significance")]
    significance: Option<f64>,
}

pub fn parse_swift_bat(payload: &str) -> Result<Event, ParseError> {
    // Try JSON first
    if let Ok(alert) = serde_json::from_str::<SwiftBatAlert>(payload) {
        let trigger_id = alert.trigger_id.unwrap_or_else(|| "unknown".to_string());
        let trigger_time = alert.trigger_time.unwrap_or(0.0);

        let position =
            if let (Some(ra), Some(dec), Some(error)) = (alert.ra, alert.dec, alert.error) {
                Some(SkyPosition::new(ra, dec, error * 3600.0)) // Convert degrees to arcsec
            } else {
                None
            };

        return Ok(Event::GammaRay(GammaRayEvent {
            trigger_id,
            instrument: "Swift-BAT".to_string(),
            trigger_time,
            position,
            significance: alert.significance.unwrap_or(0.0),
            skymap_url: None,
            error_radius: alert.error,
        }));
    }

    // TODO: Add VOEvent XML parsing for full support
    // For now, return a placeholder event
    tracing::warn!("Swift BAT VOEvent XML parsing not yet implemented, using placeholder");
    Ok(Event::GammaRay(GammaRayEvent {
        trigger_id: "placeholder".to_string(),
        instrument: "Swift-BAT".to_string(),
        trigger_time: 0.0,
        position: None,
        significance: 0.0,
        skymap_url: None,
        error_radius: None,
    }))
}

/// Fermi GBM alert formats
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FermiGbmAlert {
    #[serde(rename = "trigger_id")]
    trigger_id: Option<String>,
    #[serde(rename = "trigger_time")]
    trigger_time: Option<f64>,
    #[serde(rename = "ra")]
    ra: Option<f64>,
    #[serde(rename = "dec")]
    dec: Option<f64>,
    #[serde(rename = "error_radius")]
    error_radius: Option<f64>,
    #[serde(rename = "reliability")]
    reliability: Option<f64>,
    #[serde(rename = "most_likely_source")]
    most_likely_source: Option<String>,
}

/// Parse Fermi GBM flight position alert
pub fn parse_fermi_gbm_flt_pos(payload: &str) -> Result<Event, ParseError> {
    parse_fermi_gbm(payload, "Fermi-GBM-FLT")
}

/// Parse Fermi GBM ground position alert
pub fn parse_fermi_gbm_gnd_pos(payload: &str) -> Result<Event, ParseError> {
    parse_fermi_gbm(payload, "Fermi-GBM-GND")
}

/// Parse Fermi GBM final position alert
pub fn parse_fermi_gbm_fin_pos(payload: &str) -> Result<Event, ParseError> {
    parse_fermi_gbm(payload, "Fermi-GBM-FIN")
}

/// Generic Fermi GBM parser
pub fn parse_fermi_gbm(payload: &str, instrument: &str) -> Result<Event, ParseError> {
    // Try to parse as JSON
    let json: Value = serde_json::from_str(payload)
        .map_err(|e| ParseError::JsonError(format!("Failed to parse Fermi JSON: {}", e)))?;

    // Extract trigger ID
    let trigger_id = json["trigger_id"]
        .as_str()
        .or_else(|| json["triggerID"].as_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract trigger time (MET - Mission Elapsed Time)
    let trigger_time_met = json["trigger_time"]
        .as_f64()
        .or_else(|| json["triggerTime"].as_f64())
        .unwrap_or(0.0);

    // Convert Fermi MET to GPS time
    // Fermi MET epoch is 2001-01-01 00:00:00 UTC
    // GPS epoch is 1980-01-06 00:00:00 UTC
    // Difference is ~662860800 seconds
    let trigger_time = if trigger_time_met > 0.0 {
        trigger_time_met + 662860800.0
    } else {
        0.0
    };

    // Extract position
    let ra = json["ra"].as_f64();
    let dec = json["dec"].as_f64();
    let error_radius = json["error_radius"]
        .as_f64()
        .or_else(|| json["errorRadius"].as_f64());

    let position = if let (Some(ra), Some(dec)) = (ra, dec) {
        let error_arcsec = error_radius.unwrap_or(1.0) * 3600.0; // Convert degrees to arcsec
        Some(SkyPosition::new(ra, dec, error_arcsec))
    } else {
        None
    };

    // Extract reliability/significance
    let significance = json["reliability"]
        .as_f64()
        .or_else(|| json["most_likely_prob"].as_f64())
        .unwrap_or(0.0);

    // Extract skymap URL if present
    let skymap_url = json["skymap_url"]
        .as_str()
        .or_else(|| json["skymap"].as_str())
        .map(|s| s.to_string());

    tracing::info!(
        "Parsed {} GRB: trigger_id={}, time={}, position={:?}, skymap={:?}",
        instrument,
        trigger_id,
        trigger_time,
        position,
        skymap_url
    );

    Ok(Event::GammaRay(GammaRayEvent {
        trigger_id,
        instrument: instrument.to_string(),
        trigger_time,
        position,
        significance,
        skymap_url,
        error_radius,
    }))
}

/// Parse Fermi GBM classic VOEvent XML alerts
/// These come from topics like gcn.classic.voevent.FERMI_GBM_FLT_POS
pub fn parse_fermi_voevent(payload: &str, topic: &str) -> Result<Event, ParseError> {
    // Determine instrument suffix from topic name
    let instrument = if topic.contains("FLT_POS") {
        "Fermi-GBM-FLT"
    } else if topic.contains("GND_POS") {
        "Fermi-GBM-GND"
    } else if topic.contains("FIN_POS") {
        "Fermi-GBM-FIN"
    } else if topic.contains("SUBTHRESH") {
        "Fermi-GBM-SUBTHRESH"
    } else {
        "Fermi-GBM-VOEvent"
    };

    // Classic VOEvent payloads are XML. Try to extract key fields with simple string parsing.
    // A full XML parser (quick-xml) could be added later if needed.

    // Extract trigger ID from <Param name="TrigID" value="..." />
    let trigger_id = extract_voevent_param(payload, "TrigID")
        .or_else(|| extract_voevent_param(payload, "Trigger_Number"))
        .unwrap_or_else(|| "unknown".to_string());

    // Extract RA/Dec from <C1> and <C2> inside <Position2D>
    // Or from Param elements
    let ra = extract_voevent_param(payload, "RA")
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| extract_voevent_c1(payload));
    let dec = extract_voevent_param(payload, "Dec")
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| extract_voevent_c2(payload));
    let error_radius =
        extract_voevent_param(payload, "Error2Radius").and_then(|s| s.parse::<f64>().ok());

    let position = if let (Some(ra), Some(dec)) = (ra, dec) {
        let error_arcsec = error_radius.unwrap_or(1.0) * 3600.0;
        Some(SkyPosition::new(ra, dec, error_arcsec))
    } else {
        None
    };

    // Extract trigger time from ISOTime or Param
    let trigger_time = extract_voevent_isotime(payload)
        .and_then(|s| mm_core::GpsTime::from_iso8601(&s).ok().map(|t| t.seconds))
        .unwrap_or(0.0);

    tracing::info!(
        "Parsed {} VOEvent: trigger_id={}, time={}, position={:?}",
        instrument,
        trigger_id,
        trigger_time,
        position
    );

    Ok(Event::GammaRay(GammaRayEvent {
        trigger_id,
        instrument: instrument.to_string(),
        trigger_time,
        position,
        significance: 0.0,
        skymap_url: None,
        error_radius,
    }))
}

/// Extract a named parameter value from VOEvent XML
fn extract_voevent_param(xml: &str, name: &str) -> Option<String> {
    // Match patterns like: <Param name="TrigID" value="123456" />
    // or: <Param value="123456" name="TrigID" />
    let name_pattern = format!("name=\"{}\"", name);
    for line in xml.lines() {
        let line = line.trim();
        if line.contains(&name_pattern) && line.contains("value=\"") {
            if let Some(start) = line.find("value=\"") {
                let rest = &line[start + 7..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

/// Extract C1 (RA) from VOEvent Position2D
fn extract_voevent_c1(xml: &str) -> Option<f64> {
    extract_xml_element(xml, "C1")
}

/// Extract C2 (Dec) from VOEvent Position2D
fn extract_voevent_c2(xml: &str) -> Option<f64> {
    extract_xml_element(xml, "C2")
}

/// Extract ISOTime from VOEvent
fn extract_voevent_isotime(xml: &str) -> Option<String> {
    // Look for <ISOTime>2026-01-15T12:34:56</ISOTime>
    if let Some(start) = xml.find("<ISOTime>") {
        let rest = &xml[start + 9..];
        if let Some(end) = rest.find("</ISOTime>") {
            let time_str = rest[..end].trim().to_string();
            // Ensure it ends with Z for RFC 3339 compliance
            if time_str.ends_with('Z') {
                return Some(time_str);
            } else {
                return Some(format!("{}Z", time_str));
            }
        }
    }
    None
}

/// Extract a simple XML element's text content and parse as f64
fn extract_xml_element(xml: &str, tag: &str) -> Option<f64> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xml.find(&open) {
        let rest = &xml[start + open.len()..];
        if let Some(end) = rest.find(&close) {
            return rest[..end].trim().parse::<f64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_swift_bat_json() {
        let payload = r#"{
            "TrigID": "12345",
            "TriggerTime": 1412546713.52,
            "RA": 123.456,
            "Dec": 45.123,
            "Error": 0.05,
            "Significance": 8.5
        }"#;

        let event = parse_swift_bat(payload).unwrap();
        match event {
            Event::GammaRay(grb) => {
                assert_eq!(grb.trigger_id, "12345");
                assert_eq!(grb.instrument, "Swift-BAT");
                assert!(grb.position.is_some());
            }
            _ => panic!("Expected GammaRay event"),
        }
    }
}
