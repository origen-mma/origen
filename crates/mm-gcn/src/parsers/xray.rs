use mm_core::{Event, ParseError, SkyPosition, XRayEvent};
use serde::Deserialize;

/// Einstein Probe WXT alert format
#[derive(Debug, Deserialize)]
struct EinsteinProbeAlert {
    event_id: Option<String>,
    trigger_time: Option<f64>,
    ra: Option<f64>,
    dec: Option<f64>,
    error_radius: Option<f64>,
    flux: Option<f64>,
}

pub fn parse_einstein_probe(payload: &str) -> Result<Event, ParseError> {
    let alert: EinsteinProbeAlert = serde_json::from_str(payload)
        .map_err(|e| ParseError::JsonError(e.to_string()))?;

    let event_id = alert.event_id.unwrap_or_else(|| "unknown".to_string());
    let trigger_time = alert.trigger_time.ok_or(ParseError::MissingField("trigger_time"))?;

    let position = if let (Some(ra), Some(dec)) = (alert.ra, alert.dec) {
        let error_radius = alert.error_radius.unwrap_or(10.0);
        Some(SkyPosition::new(ra, dec, error_radius * 3600.0)) // Convert degrees to arcsec
    } else {
        None
    };

    Ok(Event::XRay(XRayEvent {
        event_id,
        trigger_time,
        position,
        flux: alert.flux,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_einstein_probe() {
        let payload = r#"{
            "event_id": "EP20240101a",
            "trigger_time": 1412546713.52,
            "ra": 123.456,
            "dec": 45.123,
            "error_radius": 0.1,
            "flux": 1.5e-10
        }"#;

        let event = parse_einstein_probe(payload).unwrap();
        match event {
            Event::XRay(xray) => {
                assert_eq!(xray.event_id, "EP20240101a");
                assert!(xray.position.is_some());
                assert!(xray.flux.is_some());
            }
            _ => panic!("Expected XRay event"),
        }
    }
}
