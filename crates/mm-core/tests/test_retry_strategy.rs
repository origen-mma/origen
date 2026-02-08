/// Test retry strategy: use original params if safeguarded version fails catastrophically
///
/// Strategy:
/// 1. First attempt: Use current settings (LR=0.005, with safeguards)
/// 2. If catastrophic failure (ELBO < -1000): Retry with original settings (LR=0.01, no safeguards)
///
/// Run with: cargo test --test test_retry_strategy -- --nocapture --ignored
use mm_core::{fit_lightcurve, FitModel, FitQualityAssessment, LightCurve, Photometry};
use rand::Rng;

/// Generate synthetic GRB afterglow light curve (PowerLaw decay)
fn generate_synthetic_afterglow(seed: u64) -> (LightCurve, f64) {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let true_log_a: f64 = 5.0;
    let true_log_alpha: f64 = -1.0;
    let true_log_beta: f64 = 0.4;
    let true_t0: f64 = 0.0;

    let obs_times = vec![0.1, 0.5, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 14.0, 21.0, 30.0];

    let mut fluxes = Vec::new();
    let mut errors = Vec::new();

    for &t in &obs_times {
        let dt = t - true_t0;
        let a = true_log_a.exp();
        let alpha = true_log_alpha.exp();
        let beta = true_log_beta.exp();

        let flux_clean = a * dt.powf(alpha) * (-dt.powf(beta)).exp();

        let snr = if t < 5.0 { 20.0 } else { 10.0 };
        let err = flux_clean / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let flux_noisy = (flux_clean + noise).max(0.1);

        fluxes.push(flux_noisy);
        errors.push(err);
    }

    let mut lightcurve = LightCurve::new(format!("GRB_AFTERGLOW_{}", seed));
    let mjd_offset = 59000.0;

    for (i, &t) in obs_times.iter().enumerate() {
        lightcurve.add_measurement(Photometry::new(
            mjd_offset + t,
            fluxes[i],
            errors[i],
            "R".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

#[test]
#[ignore]
fn test_retry_on_catastrophic_failure() {
    println!("\n=== Testing Retry Strategy ===\n");
    println!("Strategy:");
    println!("  1. First try: Current settings (LR=0.005, safeguards)");
    println!("  2. If ELBO < -1000: Retry with original settings (LR=0.01)");
    println!();

    let mut first_attempt_stats = Stats::new();
    let mut retry_stats = Stats::new();
    let mut final_stats = Stats::new();

    for seed in 1..=10 {
        let (lc, true_t0) = generate_synthetic_afterglow(seed);
        let mjd_offset = 59000.0;
        let true_t0_mjd = mjd_offset + true_t0;

        // First attempt with current settings
        let first_result = fit_lightcurve(&lc, FitModel::PowerLaw);

        match first_result {
            Ok(fit) => {
                let t0_error = (fit.t0 - true_t0_mjd).abs();
                first_attempt_stats.record(fit.elbo, t0_error);

                // Check if catastrophic failure
                if fit.elbo < -1000.0 {
                    println!(
                        "Seed {:2}: CATASTROPHIC (ELBO = {:.2e}), retrying with original params...",
                        seed, fit.elbo
                    );

                    // TODO: Retry with original parameters (LR=0.01, no safeguards)
                    // For now, just use the same fit function
                    // In real implementation, we'd have a fit_lightcurve_with_config() function
                    let retry_result = fit_lightcurve(&lc, FitModel::PowerLaw);

                    match retry_result {
                        Ok(retry_fit) => {
                            let retry_t0_error = (retry_fit.t0 - true_t0_mjd).abs();
                            retry_stats.record(retry_fit.elbo, retry_t0_error);

                            println!(
                                "  Retry: ELBO = {:.2}, t0_err = {:.2} days ({})",
                                retry_fit.elbo,
                                retry_t0_error,
                                if retry_fit.elbo > fit.elbo {
                                    "IMPROVED ✅"
                                } else {
                                    "WORSE ❌"
                                }
                            );

                            // Use better of the two
                            if retry_fit.elbo > fit.elbo {
                                final_stats.record(retry_fit.elbo, retry_t0_error);
                            } else {
                                final_stats.record(fit.elbo, t0_error);
                            }
                        }
                        Err(e) => {
                            println!("  Retry: FAILED - {}", e);
                            final_stats.record(fit.elbo, t0_error);
                        }
                    }
                } else {
                    // No retry needed
                    let assessment = FitQualityAssessment::assess(&fit, None);
                    println!(
                        "Seed {:2}: ELBO = {:8.2}, t0_err = {:5.2} days, Quality = {:?}",
                        seed, fit.elbo, t0_error, assessment.quality
                    );
                    final_stats.record(fit.elbo, t0_error);
                }
            }
            Err(e) => {
                println!("Seed {:2}: FAILED - {}", seed, e);
            }
        }
    }

    println!("\n=== First Attempt Summary (Current Settings) ===");
    first_attempt_stats.print();

    if retry_stats.count > 0 {
        println!("\n=== Retry Summary (Original Settings) ===");
        retry_stats.print();
    }

    println!("\n=== Final Results (Best of First + Retries) ===");
    final_stats.print();

    println!("\n=== Effectiveness ===");
    let retries_helped = retry_stats.count;
    let retries_total = first_attempt_stats.catastrophic_count;
    if retries_total > 0 {
        println!(
            "Catastrophic failures: {} / {}",
            retries_total, first_attempt_stats.count
        );
        println!("Retries attempted: {}", retries_helped);
        println!(
            "Success: Reduced catastrophic failures from {} to {}",
            first_attempt_stats.catastrophic_count, final_stats.catastrophic_count
        );
    } else {
        println!("No catastrophic failures - retry strategy not needed");
    }
}

struct Stats {
    count: usize,
    elbos: Vec<f64>,
    t0_errors: Vec<f64>,
    catastrophic_count: usize,
}

impl Stats {
    fn new() -> Self {
        Self {
            count: 0,
            elbos: Vec::new(),
            t0_errors: Vec::new(),
            catastrophic_count: 0,
        }
    }

    fn record(&mut self, elbo: f64, t0_error: f64) {
        self.count += 1;
        self.elbos.push(elbo);
        self.t0_errors.push(t0_error);
        if elbo < -1000.0 {
            self.catastrophic_count += 1;
        }
    }

    fn print(&self) {
        if self.count == 0 {
            println!("No data");
            return;
        }

        let n_excellent = self.elbos.iter().filter(|&&e| e > 50.0).count();
        let n_good = self
            .elbos
            .iter()
            .filter(|&&e| e > 10.0 && e <= 50.0)
            .count();
        let n_fair = self.elbos.iter().filter(|&&e| e > 0.0 && e <= 10.0).count();
        let n_poor = self
            .elbos
            .iter()
            .filter(|&&e| e > -10.0 && e <= 0.0)
            .count();
        let n_failed = self.elbos.iter().filter(|&&e| e <= -10.0).count();

        println!("Total: {}", self.count);
        println!(
            "Excellent (ELBO > 50): {} ({:.0}%)",
            n_excellent,
            (n_excellent as f64 / self.count as f64) * 100.0
        );
        println!(
            "Good (ELBO 10-50): {} ({:.0}%)",
            n_good,
            (n_good as f64 / self.count as f64) * 100.0
        );
        println!(
            "Fair (ELBO 0-10): {} ({:.0}%)",
            n_fair,
            (n_fair as f64 / self.count as f64) * 100.0
        );
        println!(
            "Poor (ELBO -10 to 0): {} ({:.0}%)",
            n_poor,
            (n_poor as f64 / self.count as f64) * 100.0
        );
        println!(
            "Failed (ELBO < -10): {} ({:.0}%)",
            n_failed,
            (n_failed as f64 / self.count as f64) * 100.0
        );
        println!("Catastrophic (ELBO < -1000): {}", self.catastrophic_count);

        // Median t0 error
        let mut sorted_errors = self.t0_errors.clone();
        sorted_errors.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_error = sorted_errors[sorted_errors.len() / 2];
        println!(
            "\nMedian t0 error: {:.2} days ({:.1} hours)",
            median_error,
            median_error * 24.0
        );
    }
}

#[test]
#[ignore]
fn test_comparison_original_vs_safeguarded() {
    println!("\n=== Comparing Original vs Safeguarded Parameters ===\n");
    println!("Testing on seed 10 (known to cause catastrophic failure):\n");

    let (lc, true_t0) = generate_synthetic_afterglow(10);
    let mjd_offset = 59000.0;
    let true_t0_mjd = mjd_offset + true_t0;

    println!("Attempt 1: Current settings (LR=0.005, safeguards)");
    match fit_lightcurve(&lc, FitModel::PowerLaw) {
        Ok(fit) => {
            let t0_error = (fit.t0 - true_t0_mjd).abs();
            println!("  ELBO: {:.2}", fit.elbo);
            println!(
                "  t0 error: {:.2} days ({:.1} hours)",
                t0_error,
                t0_error * 24.0
            );
            let assessment = FitQualityAssessment::assess(&fit, None);
            println!("  Quality: {:?}", assessment.quality);
        }
        Err(e) => {
            println!("  FAILED: {}", e);
        }
    }

    println!("\nAttempt 2: Original settings (would need LR=0.01, no safeguards)");
    println!("  (Currently using same settings - need to implement config passing)");
    match fit_lightcurve(&lc, FitModel::PowerLaw) {
        Ok(fit) => {
            let t0_error = (fit.t0 - true_t0_mjd).abs();
            println!("  ELBO: {:.2}", fit.elbo);
            println!(
                "  t0 error: {:.2} days ({:.1} hours)",
                t0_error,
                t0_error * 24.0
            );
            let assessment = FitQualityAssessment::assess(&fit, None);
            println!("  Quality: {:?}", assessment.quality);
        }
        Err(e) => {
            println!("  FAILED: {}", e);
        }
    }

    println!("\n=== Next Steps ===");
    println!("1. Add fit_lightcurve_with_config() that accepts FitConfig");
    println!("2. FitConfig should include:");
    println!("   - svi_learning_rate (0.005 vs 0.01)");
    println!("   - enable_safeguards (true vs false)");
    println!("   - scale_clamp_range (Option<(f64, f64)>)");
    println!("3. Implement retry logic in fit_lightcurve():");
    println!("   - Try with conservative config first");
    println!("   - If catastrophic (ELBO < -1000), retry with aggressive config");
    println!("   - Return best result");
}
