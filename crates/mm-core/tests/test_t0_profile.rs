/// Test profile likelihood approach for t0 estimation
///
/// Run with: cargo test --test test_t0_profile -- --nocapture --ignored
use mm_core::{
    fit_lightcurve, fit_lightcurve_profile_t0, svi_models, FitConfig, FitModel,
    FitQualityAssessment, LightCurve, Photometry,
};
use rand::Rng;

fn generate_synthetic_kilonova(seed: u64) -> (LightCurve, f64) {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let true_t0: f64 = 0.0;
    let true_params = vec![-2.0, -1.0, 0.5, true_t0, -3.0];

    let obs_times_detections = vec![
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0,
    ];
    let obs_times_nondetections = vec![-3.0, -2.0, -1.0, -0.5];

    let mut all_obs_times = obs_times_nondetections.clone();
    all_obs_times.extend_from_slice(&obs_times_detections);

    let clean_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &true_params,
        &all_obs_times,
    );

    let scale_factor = 200.0;
    let mut lightcurve = LightCurve::new(format!("SEED_{}", seed));
    let mjd_offset = 60000.0;

    let n_nondet = obs_times_nondetections.len();
    let limiting_flux = 15.0;

    #[allow(clippy::needless_range_loop)]
    for i in 0..n_nondet {
        lightcurve.add_measurement(Photometry::new_upper_limit(
            mjd_offset + all_obs_times[i],
            limiting_flux,
            "r".to_string(),
        ));
    }

    for i in n_nondet..all_obs_times.len() {
        let flux = clean_fluxes[i] * scale_factor;
        let snr = 20.0;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let scaled_flux = (flux + noise).max(0.1);

        lightcurve.add_measurement(Photometry::new(
            mjd_offset + all_obs_times[i],
            scaled_flux,
            err,
            "r".to_string(),
        ));
    }

    (lightcurve, true_t0)
}

#[test]
#[ignore]
fn test_profile_vs_joint_optimization() {
    println!("\n=== Profile Likelihood vs Joint Optimization ===\n");

    let (lc, true_t0) = generate_synthetic_kilonova(1);
    let mjd_offset = 60000.0;
    let true_t0_mjd = mjd_offset + true_t0;

    // Method 1: Standard joint optimization
    println!("Method 1: Joint Optimization (all parameters together)");
    let joint_result = fit_lightcurve(&lc, FitModel::MetzgerKN).unwrap();
    let joint_t0_err = (joint_result.t0 - true_t0_mjd).abs();

    println!(
        "  t0 = {:.3} MJD (error: {:.2} days = {:.1} hours)",
        joint_result.t0,
        joint_t0_err,
        joint_t0_err * 24.0
    );
    println!(
        "  t0_err = ±{:.2} days (±{:.1} hours)",
        joint_result.t0_err,
        joint_result.t0_err * 24.0
    );
    println!("  ELBO = {:.2}", joint_result.elbo);

    let joint_quality = FitQualityAssessment::assess(&joint_result, None);
    println!("  Quality: {:?}", joint_quality.quality);

    // Method 2: Profile likelihood
    println!("\nMethod 2: Profile Likelihood (grid search over t0)");
    let config = FitConfig::default();
    let profile_result = fit_lightcurve_profile_t0(&lc, FitModel::MetzgerKN, &config).unwrap();
    let profile_t0_err = (profile_result.t0 - true_t0_mjd).abs();

    println!(
        "  t0 = {:.3} MJD (error: {:.2} days = {:.1} hours)",
        profile_result.t0,
        profile_t0_err,
        profile_t0_err * 24.0
    );
    println!(
        "  t0_err = ±{:.2} days (±{:.1} hours)",
        profile_result.t0_err,
        profile_result.t0_err * 24.0
    );
    println!("  ELBO = {:.2}", profile_result.elbo);

    let profile_quality = FitQualityAssessment::assess(&profile_result, None);
    println!("  Quality: {:?}", profile_quality.quality);

    // Comparison
    println!("\n=== Comparison ===");
    println!("t0 error:");
    println!(
        "  Joint: {:.2} days ({:.1} hours)",
        joint_t0_err,
        joint_t0_err * 24.0
    );
    println!(
        "  Profile: {:.2} days ({:.1} hours)",
        profile_t0_err,
        profile_t0_err * 24.0
    );
    println!("\nt0 uncertainty:");
    println!("  Joint: ±{:.2} days", joint_result.t0_err);
    println!("  Profile: ±{:.2} days", profile_result.t0_err);
    println!("\nELBO:");
    println!("  Joint: {:.2}", joint_result.elbo);
    println!("  Profile: {:.2}", profile_result.elbo);

    if profile_t0_err < joint_t0_err {
        println!("\n✅ Profile likelihood achieved better t0 estimate!");
    } else {
        println!("\n⚠️  Joint optimization was better for this case");
    }
}

#[test]
#[ignore]
fn test_profile_likelihood_multiple_seeds() {
    println!("\n=== Profile Likelihood: Multiple Seeds ===\n");

    let mut joint_stats = Stats::new();
    let mut profile_stats = Stats::new();

    for seed in 1..=5 {
        println!("Seed {}:", seed);

        let (lc, true_t0) = generate_synthetic_kilonova(seed);
        let mjd_offset = 60000.0;
        let true_t0_mjd = mjd_offset + true_t0;

        // Joint optimization
        match fit_lightcurve(&lc, FitModel::MetzgerKN) {
            Ok(joint_result) => {
                let t0_err = (joint_result.t0 - true_t0_mjd).abs();
                joint_stats.record(joint_result.elbo, t0_err);
                print!(
                    "  Joint: ELBO = {:7.2}, t0_err = {:.2} days",
                    joint_result.elbo, t0_err
                );
            }
            Err(e) => {
                print!("  Joint: FAILED - {}", e);
            }
        }

        // Profile likelihood
        let config = FitConfig::default();
        match fit_lightcurve_profile_t0(&lc, FitModel::MetzgerKN, &config) {
            Ok(profile_result) => {
                let t0_err = (profile_result.t0 - true_t0_mjd).abs();
                profile_stats.record(profile_result.elbo, t0_err);
                println!(
                    ", Profile: ELBO = {:7.2}, t0_err = {:.2} days",
                    profile_result.elbo, t0_err
                );
            }
            Err(e) => {
                println!(", Profile: FAILED - {}", e);
            }
        }
    }

    println!("\n=== Summary ===");
    println!("\nJoint Optimization:");
    joint_stats.print();

    println!("\nProfile Likelihood:");
    profile_stats.print();

    println!("\n=== Improvement ===");
    if profile_stats.median_t0_error() < joint_stats.median_t0_error() {
        let improvement = (joint_stats.median_t0_error() - profile_stats.median_t0_error())
            / joint_stats.median_t0_error()
            * 100.0;
        println!(
            "Profile likelihood reduced median t0 error by {:.1}%",
            improvement
        );
    }
}

struct Stats {
    elbos: Vec<f64>,
    t0_errors: Vec<f64>,
}

impl Stats {
    fn new() -> Self {
        Self {
            elbos: Vec::new(),
            t0_errors: Vec::new(),
        }
    }

    fn record(&mut self, elbo: f64, t0_error: f64) {
        self.elbos.push(elbo);
        self.t0_errors.push(t0_error);
    }

    fn median_t0_error(&self) -> f64 {
        if self.t0_errors.is_empty() {
            return f64::NAN;
        }
        let mut sorted = self.t0_errors.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    }

    fn print(&self) {
        if self.elbos.is_empty() {
            println!("  No data");
            return;
        }

        let median_elbo = {
            let mut sorted = self.elbos.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted[sorted.len() / 2]
        };

        let median_t0_err = self.median_t0_error();

        println!("  Trials: {}", self.elbos.len());
        println!("  Median ELBO: {:.2}", median_elbo);
        println!(
            "  Median t0 error: {:.2} days ({:.1} hours)",
            median_t0_err,
            median_t0_err * 24.0
        );

        let n_good = self.elbos.iter().filter(|&&e| e > 10.0).count();
        println!(
            "  Good fits (ELBO > 10): {} / {} ({:.0}%)",
            n_good,
            self.elbos.len(),
            (n_good as f64 / self.elbos.len() as f64) * 100.0
        );
    }
}
