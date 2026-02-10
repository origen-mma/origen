use mm_core::{Event, GWEvent, GpsTime, ParseError};
use serde_json::Value;

pub fn parse_gw_alert(payload: &str) -> Result<Event, ParseError> {
    let json: Value =
        serde_json::from_str(payload).map_err(|e| ParseError::JsonError(e.to_string()))?;

    let alert_type = json["alert_type"].as_str().unwrap_or("UNKNOWN").to_string();

    let superevent_id = json["superevent_id"]
        .as_str()
        .ok_or(ParseError::MissingField("superevent_id"))?
        .to_string();

    // Retractions have event: null — skip them
    let event = match json.get("event") {
        Some(v) if !v.is_null() => v,
        _ => return Err(ParseError::MissingField("event")),
    };

    // Parse event time: real GCN sends ISO 8601 strings, simulations send GPS floats
    let gps_time = if let Some(t) = event["time"].as_f64() {
        GpsTime::from_seconds(t)
    } else if let Some(t) = event["time"].as_str() {
        GpsTime::from_iso8601(t)
            .map_err(|e| ParseError::JsonError(format!("Failed to parse time '{}': {}", t, e)))?
    } else {
        return Err(ParseError::MissingField("event.time"));
    };

    // Extract instruments
    let instruments = event["instruments"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let far = event["far"].as_f64().unwrap_or(1.0);

    Ok(Event::GravitationalWave(GWEvent {
        superevent_id,
        alert_type,
        gps_time,
        instruments,
        far,
        position: None,
        skymap: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gw_alert_gps_float() {
        let payload = r#"{
            "alert_type": "PRELIMINARY",
            "superevent_id": "S240101a",
            "event": {
                "time": 1412546713.52,
                "instruments": ["H1", "L1", "V1"],
                "far": 1e-10
            }
        }"#;

        let event = parse_gw_alert(payload).unwrap();
        match event {
            Event::GravitationalWave(gw) => {
                assert_eq!(gw.superevent_id, "S240101a");
                assert_eq!(gw.instruments, vec!["H1", "L1", "V1"]);
            }
            _ => panic!("Expected GravitationalWave event"),
        }
    }

    #[test]
    fn test_parse_gw_alert_iso8601() {
        let payload = r#"{
            "alert_type": "PRELIMINARY",
            "superevent_id": "MS260109h",
            "time_created": "2026-01-09T08:08:17Z",
            "event": {
                "time": "2026-01-09T08:02:45.420Z",
                "instruments": ["H1", "L1"],
                "far": 9.11e-14,
                "classification": {"BNS": 0.9999}
            }
        }"#;

        let event = parse_gw_alert(payload).unwrap();
        match event {
            Event::GravitationalWave(gw) => {
                assert_eq!(gw.superevent_id, "MS260109h");
                assert_eq!(gw.alert_type, "PRELIMINARY");
                assert_eq!(gw.instruments, vec!["H1", "L1"]);
                // GPS time should be reasonable (year 2026)
                assert!(gw.gps_time.seconds > 1.4e9);
            }
            _ => panic!("Expected GravitationalWave event"),
        }
    }

    #[test]
    fn test_parse_gw_alert_retraction() {
        let payload = r#"{
            "alert_type": "RETRACTION",
            "superevent_id": "MS260109h",
            "event": null
        }"#;

        let result = parse_gw_alert(payload);
        assert!(result.is_err());
    }
}
