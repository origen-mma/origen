use anyhow::Result;
use clap::Parser;
use mm_correlator::CorrelatorConfig;
use mm_simulation::{
    far_campaign::{run_injection_campaign, CampaignConfig},
    optical_injection::{GwPopulationModel, SurveyModel},
    BackgroundOpticalConfig,
};
use tracing::info;

#[derive(Parser)]
#[command(
    name = "far-tuning-campaign",
    about = "Run a GW-optical injection campaign to calibrate joint FAR thresholds.\n\n\
             Injects kilonova signals into a realistic background of optical transients\n\
             and measures detection efficiency vs false positive rate."
)]
struct Cli {
    /// Number of signal injections
    #[arg(short, long, default_value = "100")]
    n_injections: usize,

    /// Survey: "ztf" or "lsst"
    #[arg(short, long, default_value = "ztf")]
    survey: String,

    /// BNS detection horizon (Mpc)
    #[arg(short, long, default_value = "190")]
    d_horizon: f64,

    /// Observing window per injection (days)
    #[arg(short, long, default_value = "14")]
    window_days: f64,

    /// Random seed for reproducibility
    #[arg(long, default_value = "42")]
    seed: u64,

    /// Output JSON file for full results
    #[arg(short, long)]
    output: Option<String>,

    /// Extra logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose {
        "far_tuning_campaign=debug,mm_simulation=debug,mm_correlator=debug"
    } else {
        "far_tuning_campaign=info,mm_simulation=info,mm_correlator=warn"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let survey = match cli.survey.as_str() {
        "lsst" => SurveyModel::lsst(),
        _ => SurveyModel::ztf(),
    };

    let bg_config = match cli.survey.as_str() {
        "lsst" => BackgroundOpticalConfig::lsst(),
        _ => BackgroundOpticalConfig::ztf(),
    };

    let mut gw_pop = match cli.survey.as_str() {
        "lsst" => GwPopulationModel::o5(),
        _ => GwPopulationModel::o4(),
    };
    gw_pop.d_horizon_mpc = cli.d_horizon;

    let config = CampaignConfig {
        n_injections: cli.n_injections,
        observing_window_days: cli.window_days,
        survey,
        gw_pop,
        background_config: bg_config,
        correlator_config: CorrelatorConfig::without_lc_filter(),
        seed: cli.seed,
        far_thresholds: log_spaced(1e-6, 10.0, 20),
    };

    info!(
        "=== FAR Tuning Campaign: {} injections, {} survey, d_horizon={:.0} Mpc ===",
        config.n_injections, config.survey.name, config.gw_pop.d_horizon_mpc
    );

    let results = run_injection_campaign(&config);

    // Print ROC table
    info!("=== ROC Curve ===");
    info!(
        "{:>12}  {:>10}  {:>10}  {:>8}  {:>8}",
        "FAR_thresh", "Efficiency", "FPR", "N_sig", "N_bg"
    );
    for rp in &results.roc_curve {
        info!(
            "{:>12.2e}  {:>10.3}  {:>10.5}  {:>8}  {:>8}",
            rp.far_threshold,
            rp.efficiency,
            rp.false_positive_rate,
            rp.n_signal_recovered,
            rp.n_background_false,
        );
    }

    // Print efficiency vs distance
    info!("=== Detection Efficiency vs Distance ===");
    for (d_max, eff) in &results.efficiency_vs_distance {
        info!("  d <= {:>5.0} Mpc: {:.1}%", d_max, eff * 100.0);
    }

    // Write JSON if requested
    if let Some(output_path) = &cli.output {
        let json = serde_json::to_string_pretty(&results)?;
        std::fs::write(output_path, &json)?;
        info!("Full results written to: {}", output_path);
    }

    Ok(())
}

fn log_spaced(min: f64, max: f64, n: usize) -> Vec<f64> {
    let log_min = min.log10();
    let log_max = max.log10();
    (0..n)
        .map(|i| 10f64.powf(log_min + (log_max - log_min) * i as f64 / (n - 1) as f64))
        .collect()
}
