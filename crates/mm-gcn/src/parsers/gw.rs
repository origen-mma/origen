use mm_core::{Event, GWEvent, GpsTime, ParseError};
use serde::Deserialize;

/// igwn.gwalert JSON format
#[derive(Debug, Deserialize)]
struct GWAlertPayload {
    alert_type: String,
    superevent_id: String,
    #[allow(dead_code)]
    time_created: Option<String>,
    event: Option<EventInfo>,
}

#[derive(Debug, Deserialize)]
struct EventInfo {
    time: Option<f64>,
    instruments: Option<Vec<String>>,
    far: Option<f64>,
    // skymap will be added in Phase 2
}

pub fn parse_gw_alert(payload: &str) -> Result<Event, ParseError> {
    let alert: GWAlertPayload = serde_json::from_str(payload)
        .map_err(|e| ParseError::JsonError(e.to_string()))?;

    let event_info = alert.event.ok_or(ParseError::MissingField("event"))?;
    let gps_time = event_info.time.ok_or(ParseError::MissingField("event.time"))?;

    Ok(Event::GravitationalWave(GWEvent {
        superevent_id: alert.superevent_id,
        alert_type: alert.alert_type,
        gps_time: GpsTime::from_seconds(gps_time),
        instruments: event_info.instruments.unwrap_or_default(),
        far: event_info.far.unwrap_or(1.0),
        position: None, // Will parse skymap in Phase 2
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gw_alert() {
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
}
