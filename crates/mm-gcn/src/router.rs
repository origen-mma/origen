use crate::parsers::{grb, gw, neutrino, xray};
use mm_core::{Event, ParseError};

pub struct AlertRouter;

impl AlertRouter {
    pub fn new() -> Self {
        Self
    }

    /// Route alert to appropriate parser based on topic
    pub fn route_and_parse(&self, topic: &str, payload: &str) -> Result<Event, ParseError> {
        match topic {
            "igwn.gwalert" => gw::parse_gw_alert(payload),
            "gcn.notices.swift.bat.guano" => grb::parse_swift_bat(payload),
            // New-format single Fermi GBM topic (replaces flt_pos/gnd_pos/fin_pos)
            "gcn.notices.fermi.gbm.alert" => grb::parse_fermi_gbm(payload, "Fermi-GBM"),
            // Legacy topic names kept for compatibility with simulations
            "gcn.notices.fermi.gbm.flt_pos" => grb::parse_fermi_gbm_flt_pos(payload),
            "gcn.notices.fermi.gbm.gnd_pos" => grb::parse_fermi_gbm_gnd_pos(payload),
            "gcn.notices.fermi.gbm.fin_pos" => grb::parse_fermi_gbm_fin_pos(payload),
            // Classic VOEvent XML topics
            t if t.starts_with("gcn.classic.voevent.FERMI_GBM") => {
                grb::parse_fermi_voevent(payload, t)
            }
            "gcn.notices.einstein_probe.wxt.alert" => xray::parse_einstein_probe(payload),
            t if t.starts_with("gcn.notices.icecube") => neutrino::parse_icecube(payload),
            "gcn.circulars" => {
                // Human-readable circular, just log for now
                Ok(Event::Circular {
                    text: payload.to_string(),
                })
            }
            _ => Err(ParseError::UnknownTopic(topic.to_string())),
        }
    }
}

impl Default for AlertRouter {
    fn default() -> Self {
        Self::new()
    }
}
