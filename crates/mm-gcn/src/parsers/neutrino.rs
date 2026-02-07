use mm_core::{Event, NeutrinoEvent, ParseError, SkyPosition};
use serde::Deserialize;

/// IceCube neutrino alert format
#[derive(Debug, Deserialize)]
struct IceCubeAlert {
    event_id: Option<String>,
    event_time: Option<f64>,
    ra: Option<f64>,
    dec: Option<f64>,
    error_radius: Option<f64>,
    energy: Option<f64>,
    significance: Option<f64>,
}

pub fn parse_icecube(payload: &str) -> Result<Event, ParseError> {
    let alert: IceCubeAlert = serde_json::from_str(payload)
        .map_err(|e| ParseError::JsonError(e.to_string()))?;

    let event_id = alert.event_id.unwrap_or_else(|| "unknown".to_string());
    let event_time = alert.event_time.ok_or(ParseError::MissingField("event_time"))?;

    let position = if let (Some(ra), Some(dec)) = (alert.ra, alert.dec) {
        let error_radius = alert.error_radius.unwrap_or(1.0);
        Some(SkyPosition::new(ra, dec, error_radius * 3600.0)) // Convert degrees to arcsec
    } else {
        None
    };

    Ok(Event::Neutrino(NeutrinoEvent {
        event_id,
        event_time,
        position,
        energy: alert.energy,
        significance: alert.significance.unwrap_or(0.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_icecube() {
        let payload = r#"{
            "event_id": "IC20240101A",
            "event_time": 1412546713.52,
            "ra": 123.456,
            "dec": 45.123,
            "error_radius": 0.5,
            "energy": 100.0,
            "significance": 5.2
        }"#;

        let event = parse_icecube(payload).unwrap();
        match event {
            Event::Neutrino(nu) => {
                assert_eq!(nu.event_id, "IC20240101A");
                assert!(nu.position.is_some());
                assert_eq!(nu.energy, Some(100.0));
            }
            _ => panic!("Expected Neutrino event"),
        }
    }
}
