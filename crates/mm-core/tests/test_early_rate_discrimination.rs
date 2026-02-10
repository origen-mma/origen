//! Population-level test: does the early linear rate discriminator
//! separate kilonovae from Type Ia supernovae?
//!
//! Generates realistic KN and SN Ia light curves sampled at ZTF cadence,
//! runs them through compute_early_rates + early_source_selection_score,
//! and reports discrimination statistics.

use mm_core::early_rates::{
    compute_early_rates, early_source_selection_score, EarlyRateConfig, EarlySelectionResult,
};
use mm_core::lightcurve::{LightCurve, Photometry};
use rand::Rng;
use rand::SeedableRng;
use std::io::Write;

/// Convert AB magnitude to flux in microJansky
fn mag_to_flux(mag: f64) -> f64 {
    10.0_f64.powf((23.9 - mag) / 2.5)
}

/// Generate a kilonova-like light curve (AT2017gfo template)
///
/// Physics: rapid rise (~0.5-1 day), peaks at ~17-21 mag,
/// decays >1 mag/day in first few days.
/// Reference: Villar et al. 2017 (AT2017gfo multi-band fits)
fn generate_kilonova_lc(
    rng: &mut impl Rng,
    id: &str,
    t0_mjd: f64,
    distance_mpc: f64,
) -> LightCurve {
    let mut lc = LightCurve::new(id.to_string());

    // KN parameters with scatter
    let peak_abs_mag = -16.0 + rng.gen_range(-1.0..1.0); // M ~ -15 to -17
    let dist_mod = 5.0 * (distance_mpc * 1e6 / 10.0).log10(); // distance modulus
    let peak_app_mag = peak_abs_mag + dist_mod;

    let t_rise = 0.3 + rng.gen_range(0.0..0.7); // 0.3-1.0 days to peak
    let decay_rate = 0.5 + rng.gen_range(0.0..1.5); // 0.5-2.0 mag/day decay

    // Sample at ZTF-like cadence: every 1-3 days with some jitter,
    // first detection 0.1-1.0 days after explosion
    let first_det_delay = 0.1 + rng.gen_range(0.0..0.9);
    let n_epochs: usize = rng.gen_range(3..8);

    for i in 0..n_epochs {
        let dt = first_det_delay + i as f64 * (1.0 + rng.gen_range(0.0..2.0));
        let mjd = t0_mjd + dt;

        // Simple piecewise model: linear rise then linear decay
        let phase = dt - first_det_delay; // time since first det
        let time_from_peak = phase - t_rise;

        let mag = if time_from_peak < 0.0 {
            // Rising phase: ~2 mag/day rise rate (bright = low mag)
            let rise_rate = 2.0 + rng.gen_range(-0.5..0.5);
            peak_app_mag + rise_rate * (-time_from_peak)
        } else {
            // Decay phase
            peak_app_mag + decay_rate * time_from_peak
        };

        // Only "detect" if brighter than ~21.5 mag (ZTF limit + margin)
        if mag > 22.0 {
            continue;
        }

        let flux = mag_to_flux(mag);
        let flux_err = flux * rng.gen_range(0.03..0.15); // 3-15% error

        // ZTF alternates bands but typically 2+ consecutive in same band;
        // use primary band for first few, then switch
        let band = if i < n_epochs / 2 + 1 { "r" } else { "g" };
        lc.add_measurement(Photometry::new(mjd, flux, flux_err, band.to_string()));
    }

    lc
}

/// Generate a Type Ia supernova-like light curve
///
/// Physics: ~15 day rise to peak, ~0.1 mag/day rise rate,
/// slow ~30 day decline at ~0.05 mag/day initially.
/// Reference: Nugent, Kim & Perlmutter 2002 template
fn generate_sn_ia_lc(rng: &mut impl Rng, id: &str, t0_mjd: f64, distance_mpc: f64) -> LightCurve {
    let mut lc = LightCurve::new(id.to_string());

    // SN Ia parameters
    let peak_abs_mag = -19.3 + rng.gen_range(-0.5..0.5); // M ~ -18.8 to -19.8
    let dist_mod = 5.0 * (distance_mpc * 1e6 / 10.0).log10();
    let peak_app_mag = peak_abs_mag + dist_mod;

    let t_rise = 15.0 + rng.gen_range(-3.0..3.0); // 12-18 days to peak
    let decay_rate = 0.05 + rng.gen_range(0.0..0.05); // 0.05-0.10 mag/day post-peak

    // Discovery during rise phase: 2-12 days after explosion
    let first_det_delay = 2.0 + rng.gen_range(0.0..10.0);
    let n_epochs: usize = rng.gen_range(3..8);

    for i in 0..n_epochs {
        let dt = first_det_delay + i as f64 * (1.0 + rng.gen_range(0.0..2.0));
        let mjd = t0_mjd + dt;

        let time_from_peak = dt - t_rise;

        let mag = if time_from_peak < 0.0 {
            // Rising: slow, ~0.1-0.3 mag/day
            // Total rise is ~2.5 mag over ~15 days
            let total_rise_mag = 2.5 + rng.gen_range(-0.5..0.5);
            peak_app_mag + total_rise_mag * (-time_from_peak / t_rise)
        } else {
            // Post-peak decline
            peak_app_mag + decay_rate * time_from_peak
        };

        if mag > 22.0 {
            continue;
        }

        let flux = mag_to_flux(mag);
        let flux_err = flux * rng.gen_range(0.03..0.15);
        let band = if i < n_epochs / 2 + 1 { "r" } else { "g" };
        lc.add_measurement(Photometry::new(mjd, flux, flux_err, band.to_string()));
    }

    lc
}

/// Generate a Type II supernova-like light curve (core-collapse)
///
/// Physics: ~7 day rise to peak, then plateau at ~-17 mag for ~80 days.
/// Rise rate ~0.3-0.7 mag/day (faster than Ia but slower than KN).
fn generate_sn_ii_lc(rng: &mut impl Rng, id: &str, t0_mjd: f64, distance_mpc: f64) -> LightCurve {
    let mut lc = LightCurve::new(id.to_string());

    let peak_abs_mag = -17.0 + rng.gen_range(-1.0..1.0);
    let dist_mod = 5.0 * (distance_mpc * 1e6 / 10.0).log10();
    let peak_app_mag = peak_abs_mag + dist_mod;

    let t_rise = 5.0 + rng.gen_range(0.0..5.0); // 5-10 days to peak
    let plateau_decay = 0.01 + rng.gen_range(0.0..0.02); // very slow on plateau

    let first_det_delay = 1.0 + rng.gen_range(0.0..5.0);
    let n_epochs: usize = rng.gen_range(3..8);

    for i in 0..n_epochs {
        let dt = first_det_delay + i as f64 * (1.0 + rng.gen_range(0.0..2.0));
        let mjd = t0_mjd + dt;

        let time_from_peak = dt - t_rise;

        let mag = if time_from_peak < 0.0 {
            let total_rise_mag = 2.0 + rng.gen_range(-0.5..0.5);
            peak_app_mag + total_rise_mag * (-time_from_peak / t_rise)
        } else {
            // Plateau: very slow decline
            peak_app_mag + plateau_decay * time_from_peak
        };

        if mag > 22.0 {
            continue;
        }

        let flux = mag_to_flux(mag);
        let flux_err = flux * rng.gen_range(0.03..0.15);
        let band = if i < n_epochs / 2 + 1 { "r" } else { "g" };
        lc.add_measurement(Photometry::new(mjd, flux, flux_err, band.to_string()));
    }

    lc
}

#[test]
fn test_population_discrimination() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let config = EarlyRateConfig::default();

    let n_per_class = 500;
    let t0 = 60000.0; // Arbitrary MJD

    // ── Generate populations ─────────────────────────────────────────

    // Kilonovae at 40-200 Mpc (BNS range)
    let kn_lcs: Vec<LightCurve> = (0..n_per_class)
        .map(|i| {
            let d = 40.0 + rng.gen_range(0.0..160.0);
            generate_kilonova_lc(&mut rng, &format!("KN{:04}", i), t0, d)
        })
        .collect();

    // Type Ia SNe at 50-300 Mpc
    let sn_ia_lcs: Vec<LightCurve> = (0..n_per_class)
        .map(|i| {
            let d = 50.0 + rng.gen_range(0.0..250.0);
            generate_sn_ia_lc(&mut rng, &format!("SNIa{:04}", i), t0, d)
        })
        .collect();

    // Type II SNe at 30-150 Mpc
    let sn_ii_lcs: Vec<LightCurve> = (0..n_per_class)
        .map(|i| {
            let d = 30.0 + rng.gen_range(0.0..120.0);
            generate_sn_ii_lc(&mut rng, &format!("SNII{:04}", i), t0, d)
        })
        .collect();

    // ── Run discriminator on each population ─────────────────────────

    struct ClassStats {
        name: &'static str,
        total: usize,
        computed: usize,       // early rates computed successfully
        pass_boosted: usize,   // multiplier < 1.0 (KN-like)
        pass_neutral: usize,   // multiplier == 1.0
        pass_penalized: usize, // multiplier > 1.0 (SN-like)
        rejected: usize,
        rise_rates: Vec<f64>,
        decay_rates: Vec<f64>,
        multipliers: Vec<f64>,
    }

    fn run_population(
        lcs: &[LightCurve],
        config: &EarlyRateConfig,
        name: &'static str,
    ) -> ClassStats {
        let mut stats = ClassStats {
            name,
            total: lcs.len(),
            computed: 0,
            pass_boosted: 0,
            pass_neutral: 0,
            pass_penalized: 0,
            rejected: 0,
            rise_rates: Vec::new(),
            decay_rates: Vec::new(),
            multipliers: Vec::new(),
        };

        for lc in lcs {
            if let Some(rates) = compute_early_rates(lc, config) {
                stats.computed += 1;
                stats.rise_rates.push(rates.rise_rate.abs());
                stats.decay_rates.push(rates.decay_rate.abs());

                match early_source_selection_score(&rates, config) {
                    EarlySelectionResult::Pass { far_multiplier } => {
                        stats.multipliers.push(far_multiplier);
                        if far_multiplier < 0.99 {
                            stats.pass_boosted += 1;
                        } else if far_multiplier > 1.01 {
                            stats.pass_penalized += 1;
                        } else {
                            stats.pass_neutral += 1;
                        }
                    }
                    EarlySelectionResult::Reject { .. } => {
                        stats.rejected += 1;
                    }
                }
            }
        }

        stats
    }

    let kn_stats = run_population(&kn_lcs, &config, "Kilonova");
    let sn_ia_stats = run_population(&sn_ia_lcs, &config, "SN Ia");
    let sn_ii_stats = run_population(&sn_ii_lcs, &config, "SN II");

    // ── Write data file for plotting ─────────────────────────────────
    let output_path = "/tmp/early_rate_discrimination.dat";
    if let Ok(mut f) = std::fs::File::create(output_path) {
        writeln!(f, "# class rise_rate decay_rate far_multiplier").unwrap();
        for (label, stats) in [
            ("KN", &kn_stats),
            ("SNIa", &sn_ia_stats),
            ("SNII", &sn_ii_stats),
        ] {
            for i in 0..stats.rise_rates.len() {
                let rise = stats.rise_rates[i];
                let decay = if i < stats.decay_rates.len() {
                    stats.decay_rates[i]
                } else {
                    f64::NAN
                };
                let mult = if i < stats.multipliers.len() {
                    stats.multipliers[i]
                } else {
                    f64::NAN
                };
                writeln!(f, "{} {:.6} {:.6} {:.6}", label, rise, decay, mult).unwrap();
            }
        }
        println!("Plot data written to: {}", output_path);
    }

    // ── Report results ───────────────────────────────────────────────

    fn percentile(v: &[f64], p: f64) -> f64 {
        let mut clean: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
        if clean.is_empty() {
            return f64::NAN;
        }
        clean.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((clean.len() as f64 - 1.0) * p).round() as usize;
        clean[idx.min(clean.len() - 1)]
    }

    fn mean(v: &[f64]) -> f64 {
        let clean: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
        if clean.is_empty() {
            return f64::NAN;
        }
        clean.iter().sum::<f64>() / clean.len() as f64
    }

    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║        EARLY RATE DISCRIMINATOR — POPULATION TEST              ║");
    println!(
        "║        {} KN, {} SN Ia, {} SN II (soft scoring mode)          ║",
        n_per_class, n_per_class, n_per_class
    );
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    for stats in [&kn_stats, &sn_ia_stats, &sn_ii_stats] {
        let rise = &stats.rise_rates;
        let decay = &stats.decay_rates;
        let mults = &stats.multipliers;

        println!(
            "┌─ {} ({} total, {} with rates computed) ─────────",
            stats.name, stats.total, stats.computed
        );
        println!(
            "│  Rise rate (mag/day):   median={:.3}, mean={:.3}, [p10={:.3}, p90={:.3}]",
            percentile(rise, 0.5),
            mean(rise),
            percentile(rise, 0.1),
            percentile(rise, 0.9),
        );
        println!(
            "│  Decay rate (mag/day):  median={:.3}, mean={:.3}, [p10={:.3}, p90={:.3}]",
            percentile(decay, 0.5),
            mean(decay),
            percentile(decay, 0.1),
            percentile(decay, 0.9),
        );
        println!(
            "│  FAR multiplier:        median={:.3}, mean={:.3}",
            percentile(mults, 0.5),
            mean(mults),
        );
        println!(
            "│  Boosted (KN-like):     {} ({:.1}%)",
            stats.pass_boosted,
            100.0 * stats.pass_boosted as f64 / stats.computed.max(1) as f64
        );
        println!(
            "│  Penalized (SN-like):   {} ({:.1}%)",
            stats.pass_penalized,
            100.0 * stats.pass_penalized as f64 / stats.computed.max(1) as f64
        );
        println!(
            "│  Neutral:               {} ({:.1}%)",
            stats.pass_neutral,
            100.0 * stats.pass_neutral as f64 / stats.computed.max(1) as f64
        );
        println!(
            "│  Rejected (hard cut):   {} ({:.1}%)",
            stats.rejected,
            100.0 * stats.rejected as f64 / stats.computed.max(1) as f64
        );
        println!("└──────────────────────────────────────────────────\n");
    }

    // ── Summary discrimination metrics ───────────────────────────────

    let kn_boost_frac = kn_stats.pass_boosted as f64 / kn_stats.computed.max(1) as f64;
    let sn_ia_penalized_frac =
        sn_ia_stats.pass_penalized as f64 / sn_ia_stats.computed.max(1) as f64;
    let sn_ii_penalized_frac =
        sn_ii_stats.pass_penalized as f64 / sn_ii_stats.computed.max(1) as f64;
    let sn_ia_boost_frac = sn_ia_stats.pass_boosted as f64 / sn_ia_stats.computed.max(1) as f64;

    println!("┌─ DISCRIMINATION SUMMARY ─────────────────────────────────────");
    println!(
        "│  KN correctly boosted:        {:.1}%",
        kn_boost_frac * 100.0
    );
    println!(
        "│  SN Ia correctly penalized:   {:.1}%",
        sn_ia_penalized_frac * 100.0
    );
    println!(
        "│  SN II correctly penalized:   {:.1}%",
        sn_ii_penalized_frac * 100.0
    );
    println!(
        "│  SN Ia falsely boosted:       {:.1}% (contamination)",
        sn_ia_boost_frac * 100.0
    );
    println!("│");
    println!(
        "│  Effective background reduction (SN Ia): {:.1}x",
        mean(&sn_ia_stats.multipliers)
    );
    println!(
        "│  Effective signal boost (KN):            {:.1}x",
        mean(&kn_stats.multipliers)
    );
    println!("└──────────────────────────────────────────────────────────────\n");

    // ── Now test with hard_cut mode ──────────────────────────────────

    let hard_config = EarlyRateConfig {
        hard_cut: true,
        ..EarlyRateConfig::default()
    };

    let kn_hard = run_population(&kn_lcs, &hard_config, "Kilonova");
    let sn_ia_hard = run_population(&sn_ia_lcs, &hard_config, "SN Ia");
    let sn_ii_hard = run_population(&sn_ii_lcs, &hard_config, "SN II");

    println!("┌─ HARD CUT MODE ──────────────────────────────────────────────");
    println!(
        "│  KN retained:          {}/{} ({:.1}%)",
        kn_hard.computed - kn_hard.rejected,
        kn_hard.computed,
        100.0 * (kn_hard.computed - kn_hard.rejected) as f64 / kn_hard.computed.max(1) as f64
    );
    println!(
        "│  SN Ia rejected:       {}/{} ({:.1}%)",
        sn_ia_hard.rejected,
        sn_ia_hard.computed,
        100.0 * sn_ia_hard.rejected as f64 / sn_ia_hard.computed.max(1) as f64
    );
    println!(
        "│  SN II rejected:       {}/{} ({:.1}%)",
        sn_ii_hard.rejected,
        sn_ii_hard.computed,
        100.0 * sn_ii_hard.rejected as f64 / sn_ii_hard.computed.max(1) as f64
    );
    println!(
        "│  KN false rejection:   {}/{} ({:.1}%)",
        kn_hard.rejected,
        kn_hard.computed,
        100.0 * kn_hard.rejected as f64 / kn_hard.computed.max(1) as f64
    );
    println!("└──────────────────────────────────────────────────────────────\n");

    // ── Assertions: basic sanity checks ──────────────────────────────

    // SN Ia should mostly be penalized (slow risers) — main value of the cut
    assert!(
        sn_ia_penalized_frac > 0.5,
        "At least 50% of SN Ia should be penalized, got {:.1}%",
        sn_ia_penalized_frac * 100.0
    );

    // SN II should also be penalized
    assert!(
        sn_ii_penalized_frac > 0.5,
        "At least 50% of SN II should be penalized, got {:.1}%",
        sn_ii_penalized_frac * 100.0
    );

    // SN Ia contamination in boosted category should be low
    assert!(
        sn_ia_boost_frac < 0.10,
        "SN Ia contamination in boosted should be <10%, got {:.1}%",
        sn_ia_boost_frac * 100.0
    );

    // Mean KN rise rate should be clearly higher than mean SN Ia rise rate
    let kn_mean_rise = mean(&kn_stats.rise_rates);
    let sn_ia_mean_rise = mean(&sn_ia_stats.rise_rates);
    assert!(
        kn_mean_rise > sn_ia_mean_rise * 1.5,
        "KN mean rise ({:.3}) should be >1.5x SN Ia mean rise ({:.3})",
        kn_mean_rise,
        sn_ia_mean_rise
    );

    // Mean FAR multiplier should be higher (more penalizing) for SNe than KN
    let kn_mean_mult = mean(&kn_stats.multipliers);
    let sn_ia_mean_mult = mean(&sn_ia_stats.multipliers);
    assert!(
        sn_ia_mean_mult > kn_mean_mult,
        "SN Ia mean FAR mult ({:.2}) should be > KN mean ({:.2})",
        sn_ia_mean_mult,
        kn_mean_mult
    );
}
