//! GCN Circular generation for simulated GRBs
//!
//! Generates realistic GCN Circulars similar to those published for real GRB detections.
//! Format follows the standard GCN Circular format used by the community.

use crate::grb_localization::{GrbInstrument, GrbLocalization};
use crate::grb_simulation::SimulatedGrb;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// GCN Circular for a GRB detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcnCircular {
    /// GCN Circular number
    pub number: u32,

    /// Circular title
    pub title: String,

    /// Author and affiliations
    pub author: String,

    /// Circular body text
    pub body: String,

    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl GcnCircular {
    /// Generate a Fermi GBM detection circular
    pub fn fermi_gbm_detection(
        grb_name: &str,
        trigger_time: f64, // GPS time
        localization: &GrbLocalization,
        grb: &SimulatedGrb,
        circular_number: u32,
    ) -> Self {
        let timestamp = Utc::now();

        let title = format!("GRB {}: Fermi GBM detection", grb_name);

        let author = "E. Burns (LSU) and the Fermi GBM Team".to_string();

        let trigger_time_utc = gps_to_utc(trigger_time);

        // Format fluence with appropriate units and precision
        let fluence_cgs = grb.fluence.unwrap_or(0.0);
        let fluence_str = format!("{:.2e}", fluence_cgs);

        let body = format!(
            r#"At {:02}:{:02}:{:02.2} UT on {}, the Fermi Gamma-ray Burst Monitor
(GBM) triggered and located GRB {} (trigger {:.0}).

The on-ground calculated location, using the Fermi GBM trigger data, is RA = {:.2},
Dec = {:.2} (J2000 degrees, equivalent to J2000 {:02}h {:02}m, {:+.1}d), with a
statistical uncertainty of {:.1} degrees (radius, 1-sigma containment, statistical only;
there is additionally a systematic error which we have characterized as a core-plus-tail
model, with 90% of GRBs having a 3.7 deg error and a small tail ranging from 3.7-10 deg).

The angle from the Fermi LAT boresight at the GBM trigger time is {} degrees.

The GBM light curve shows {} with a duration (T90) of about {:.2} s
(50-300 keV). The time-averaged spectrum from T0-{:.1}s to T0+{:.1}s is best fit by
a power law function with an exponential high-energy cutoff. The power law index is
{:.2} +/- 0.05 and the cutoff energy, parameterized as Epeak, is {:.0} +/- 50 keV.

The event fluence (10-1000 keV) in this time interval is {} +/- 20% erg/cm^2.
The 1-sec peak photon flux measured starting from T0+0.0 s in the 10-1000 keV band
is {:.1} +/- 0.5 ph/s/cm^2.

The spectral analysis results presented above are preliminary; final results will be
published in the GBM GRB Catalog."#,
            trigger_time_utc.hour(),
            trigger_time_utc.minute(),
            trigger_time_utc.second() as f64 + (trigger_time_utc.nanosecond() as f64 / 1e9),
            trigger_time_utc.format("%d %B %Y"),
            grb_name,
            trigger_time,
            localization.obs_ra,
            localization.obs_dec,
            (localization.obs_ra / 15.0) as u32,
            ((localization.obs_ra / 15.0).fract() * 60.0) as u32,
            localization.obs_dec,
            localization.error_radius,
            format!("{:.0}", 60.0 + (trigger_time % 60.0)), // Mock angle from boresight
            if grb.t90_obs.unwrap_or(0.0) < 2.0 {
                "a single pulse"
            } else {
                "multiple overlapping pulses"
            },
            grb.t90_obs.unwrap_or(0.0),
            grb.t90_obs.unwrap_or(0.0) / 4.0,
            grb.t90_obs.unwrap_or(0.0) * 3.0 / 4.0,
            -1.5, // Typical photon index
            grb.e_peak_obs.unwrap_or(200.0),
            fluence_str,
            (fluence_cgs * 1e8) * 2.0, // Convert to photons/cm²/s (rough estimate)
        );

        Self {
            number: circular_number,
            title,
            author,
            body,
            timestamp,
        }
    }

    /// Generate a Swift BAT detection circular
    pub fn swift_bat_detection(
        grb_name: &str,
        trigger_time: f64,
        localization: &GrbLocalization,
        grb: &SimulatedGrb,
        circular_number: u32,
    ) -> Self {
        let timestamp = Utc::now();

        let title = format!("GRB {}: Swift detection of a burst", grb_name);

        let author = "K. L. Page (U Leicester) and A. Y. Lien (GSFC/UMBC) report on behalf of the Swift Team".to_string();

        let trigger_time_utc = gps_to_utc(trigger_time);

        // Convert error radius to arcminutes
        let error_arcmin = localization.error_radius * 60.0;

        let body = format!(
            r#"At {:02}:{:02}:{:02} UT on {}, the Swift Burst Alert Telescope (BAT)
triggered and located GRB {} (trigger={:.0}). Swift slewed immediately to the burst.
The BAT on-board calculated location is RA, Dec {:.4}, {:+.4} which is
   RA(J2000) = {:02}h {:02}m {:.2}s
   Dec(J2000) = {:+03}d {:02}' {:.1}"
with an uncertainty of {:.0} arcmin (radius, 90% containment, including systematic
uncertainty). The BAT light curve showed a single-peaked structure with a duration of
about {:.1} sec. The peak count rate was ~10000 counts/sec (15-350 keV), at ~{:.1} sec
after the trigger.

The XRT began observing the field at {:02}:{:02}:{:02}.0 UT, {:.1} seconds after the
BAT trigger. Using promptly downlinked data we find a bright, uncatalogued X-ray source
located at RA, Dec {:.5}, {:+.5} which is equivalent to:
   RA(J2000) = {:02}h {:02}m {:.2}s
   Dec(J2000) = {:+03}d {:02}' {:.1}"
with an uncertainty of {:.1} arcseconds (radius, 90% containment). This location is
{:.1} arcminutes from the BAT onboard position, within the BAT error circle. This
position may be improved as more data are received; the latest position is available
at https://www.swift.ac.uk/sper.

A power-law fit to a spectrum formed from promptly downlinked event data gives a column
density in excess of the Galactic value (3.2 x 10^20 cm^-2, Willingale et al. 2013),
with an excess column of 5 ( +2.5 / -2.0 ) x 10^21 cm^-2 (90% confidence).

Burst Advocate for this burst is K. L. Page (klp5 AT leicester.ac.uk).
Please contact the BA by email if you require additional information regarding Swift
followup of this burst."#,
            trigger_time_utc.hour(),
            trigger_time_utc.minute(),
            trigger_time_utc.second(),
            trigger_time_utc.format("%d %B %Y"),
            grb_name,
            trigger_time,
            localization.obs_ra,
            localization.obs_dec,
            (localization.obs_ra / 15.0) as u32,
            ((localization.obs_ra / 15.0).fract() * 60.0) as u32,
            (((localization.obs_ra / 15.0).fract() * 60.0).fract() * 60.0),
            localization.obs_dec as i32,
            (localization.obs_dec.abs().fract() * 60.0) as u32,
            ((localization.obs_dec.abs().fract() * 60.0).fract() * 60.0),
            error_arcmin,
            grb.t90_obs.unwrap_or(1.0),
            grb.t90_obs.unwrap_or(1.0) / 2.0,
            trigger_time_utc.hour(),
            trigger_time_utc.minute(),
            trigger_time_utc.second() + 80,
            80.0,                 // XRT slew time
            localization.true_ra, // XRT gets more accurate position
            localization.true_dec,
            (localization.true_ra / 15.0) as u32,
            ((localization.true_ra / 15.0).fract() * 60.0) as u32,
            (((localization.true_ra / 15.0).fract() * 60.0).fract() * 60.0),
            localization.true_dec as i32,
            (localization.true_dec.abs().fract() * 60.0) as u32,
            ((localization.true_dec.abs().fract() * 60.0).fract() * 60.0),
            error_arcmin / 10.0,                  // XRT has better localization
            localization.position_error() * 60.0, // Distance from BAT position
        );

        Self {
            number: circular_number,
            title,
            author,
            body,
            timestamp,
        }
    }

    /// Generate a GW-GRB coincidence circular
    pub fn gw_grb_coincidence(
        grb_name: &str,
        gw_event_name: &str,
        time_offset: f64, // GRB time - GW time (seconds)
        spatial_consistent: bool,
        circular_number: u32,
    ) -> Self {
        let timestamp = Utc::now();

        let title = format!(
            "LIGO/Virgo/KAGRA {} and GRB {}: potential association",
            gw_event_name, grb_name
        );

        let author = "LIGO Scientific Collaboration and Virgo Collaboration".to_string();

        let spatial_statement = if spatial_consistent {
            "The GRB localization is consistent with the LIGO/Virgo/KAGRA skymap (XX% credible region)."
        } else {
            "The GRB localization is not within the high-probability region of the LIGO/Virgo/KAGRA skymap, but given the large error regions, the association cannot be ruled out."
        };

        let time_offset_abs = time_offset.abs();

        let body = format!(
            r#"The LIGO Scientific Collaboration, the Virgo Collaboration, and the KAGRA Collaboration
report:

We have conducted a search for gravitational-wave counterparts to GRB {} in LIGO and
Virgo data around the reported burst time {}. The GRB was detected by [INSTRUMENT]
(GCN XXXXX).

Analysis of LIGO and Virgo data reveals a gravitational-wave candidate, {},
consistent with the time of the GRB. The time difference between the GRB trigger and
the gravitational-wave candidate is {:.2} seconds (GRB {}  GW).
{}

The gravitational-wave data are consistent with a binary neutron star merger. The
estimated distance to the source is XXX +/- YYY Mpc.

Further analysis is ongoing. Additional observations are encouraged to search for an
electromagnetic counterpart.

A skymap is available at:
https://gracedb.ligo.org/superevents/{}/view/

This is a preliminary result. More detailed analysis, including refined parameter
estimation, will be provided in subsequent circulars."#,
            grb_name,
            gps_to_utc(0.0).format("%Y-%m-%d %H:%M:%S UTC"), // Placeholder
            gw_event_name,
            time_offset_abs,
            if time_offset > 0.0 { "after" } else { "before" },
            spatial_statement,
            gw_event_name,
        );

        Self {
            number: circular_number,
            title,
            author,
            body,
            timestamp,
        }
    }

    /// Format as plain text (standard GCN Circular format)
    pub fn to_text(&self) -> String {
        format!(
            "TITLE:   GCN CIRCULAR\nNUMBER:  {}\nSUBJECT: {}\nDATE:    {} \nFROM:    {}\n\n{}",
            self.number,
            self.title,
            self.timestamp.format("%y/%m/%d %H:%M:%S GMT"),
            self.author,
            self.body
        )
    }
}

/// Convert GPS time to UTC DateTime
/// GPS epoch: 1980-01-06 00:00:00 UTC
/// Leap seconds: GPS is ahead of UTC by ~18 seconds (as of 2025)
fn gps_to_utc(gps_time: f64) -> DateTime<Utc> {
    const GPS_EPOCH: i64 = 315964800; // Unix timestamp of GPS epoch
    const LEAP_SECONDS: i64 = 18; // GPS-UTC offset (approximate)

    let unix_time = GPS_EPOCH + gps_time as i64 - LEAP_SECONDS;
    DateTime::from_timestamp(unix_time, 0).unwrap_or_else(|| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grb_localization::{add_localization_error, GrbInstrument};
    use crate::grb_simulation::{simulate_grb_counterpart, GrbSimulationConfig, GwEventParams};
    use rand::SeedableRng;

    #[test]
    fn test_fermi_circular_generation() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Simulate a GRB
        let gw_params = GwEventParams {
            inclination: 0.2,
            distance: 100.0,
            z: 0.02,
        };

        let config = GrbSimulationConfig::default();
        let grb = simulate_grb_counterpart(&gw_params, &config, &mut rng);

        if !grb.visible {
            return; // Skip if GRB not visible
        }

        // Add localization error
        let localization = add_localization_error(180.0, 30.0, GrbInstrument::FermiGBM, &mut rng);

        // Generate circular
        let circular =
            GcnCircular::fermi_gbm_detection("240101A", 1262304000.0, &localization, &grb, 35000);

        let text = circular.to_text();

        println!("{}", text);

        // Check that key information is present
        assert!(text.contains("Fermi GBM"));
        assert!(text.contains("GRB 240101A"));
        assert!(text.contains("RA ="));
        assert!(text.contains("Dec ="));
        assert!(text.contains("T90"));
    }

    #[test]
    fn test_swift_circular_generation() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);

        let gw_params = GwEventParams {
            inclination: 0.15,
            distance: 40.0,
            z: 0.01,
        };

        let config = GrbSimulationConfig::default();
        let grb = simulate_grb_counterpart(&gw_params, &config, &mut rng);

        if !grb.visible {
            return;
        }

        let localization = add_localization_error(45.0, -20.0, GrbInstrument::SwiftBAT, &mut rng);

        let circular =
            GcnCircular::swift_bat_detection("240615B", 1277856000.0, &localization, &grb, 35001);

        let text = circular.to_text();

        println!("{}", text);

        assert!(text.contains("Swift"));
        assert!(text.contains("BAT"));
        assert!(text.contains("XRT"));
        assert!(text.contains("GRB 240615B"));
    }

    #[test]
    fn test_coincidence_circular() {
        let circular = GcnCircular::gw_grb_coincidence("170817A", "S170817a", 1.74, true, 24228);

        let text = circular.to_text();

        println!("{}", text);

        assert!(text.contains("S170817a"));
        assert!(text.contains("GRB 170817A"));
        assert!(text.contains("1.74 seconds"));
        assert!(text.contains("consistent"));
    }
}
