/// Generate plots showing light curve fits for documentation
///
/// Run with: cargo test --test plot_lightcurve_fits -- --nocapture --ignored
///
/// This generates PNG plots in docs/plots/ showing the SVI fits on real ZTF data
use mm_core::{fit_lightcurve, load_lightcurve_csv, svi_models::eval_model_batch, FitModel};
use plotters::prelude::*;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/lightcurves_csv")
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs/plots");

    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore] // Run explicitly with --ignored
fn generate_all_plots() {
    println!("Generating light curve fit plots...");

    // Plot examples with different models
    let examples = vec![
        ("ZTF25aaaalin.csv", FitModel::Bazin, "Supernova-like"),
        ("ZTF25aaaawig.csv", FitModel::PowerLaw, "Fast transient"),
        ("ZTF25aaabnwi.csv", FitModel::MetzgerKN, "Kilonova candidate"),
    ];

    for (filename, model, description) in examples {
        let lc_path = fixtures_dir().join(filename);
        if let Ok(lightcurve) = load_lightcurve_csv(&lc_path) {
            println!("\nFitting {} with {:?} model...", lightcurve.object_id, model);

            match fit_lightcurve(&lightcurve, model) {
                Ok(fit) => {
                    println!(
                        "  t0: {:.3} ± {:.3} MJD, ELBO: {:.1}",
                        fit.t0, fit.t0_err, fit.elbo
                    );

                    // Generate plot
                    let output_path = output_dir().join(format!(
                        "{}_{}_{}.png",
                        lightcurve.object_id,
                        model_name(model),
                        description.replace(" ", "_")
                    ));

                    if let Err(e) = plot_lightcurve_fit(&lightcurve, &fit, &output_path, description)
                    {
                        eprintln!("Failed to generate plot: {}", e);
                    } else {
                        println!("  Plot saved: {}", output_path.display());
                    }
                }
                Err(e) => {
                    eprintln!("  Fit failed: {}", e);
                }
            }
        }
    }

    println!("\n✅ Plots generated in: {}", output_dir().display());
}

fn model_name(model: FitModel) -> &'static str {
    match model {
        FitModel::Bazin => "Bazin",
        FitModel::Villar => "Villar",
        FitModel::PowerLaw => "PowerLaw",
        FitModel::MetzgerKN => "MetzgerKN",
    }
}

fn plot_lightcurve_fit(
    lightcurve: &mm_core::LightCurve,
    fit: &mm_core::LightCurveFitResult,
    output_path: &PathBuf,
    description: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare data
    let first_mjd = lightcurve
        .measurements
        .iter()
        .map(|m| m.mjd)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();

    let times: Vec<f64> = lightcurve
        .measurements
        .iter()
        .map(|m| m.mjd - first_mjd)
        .collect();

    let peak_flux = lightcurve
        .measurements
        .iter()
        .map(|m| m.flux)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(1.0);

    let norm_flux: Vec<f64> = lightcurve
        .measurements
        .iter()
        .map(|m| m.flux / peak_flux)
        .collect();

    let norm_err: Vec<f64> = lightcurve
        .measurements
        .iter()
        .map(|m| m.flux_err / peak_flux)
        .collect();

    // Generate model prediction
    let model_times: Vec<f64> = (0..200)
        .map(|i| {
            let t_min = times.iter().cloned().fold(f64::INFINITY, f64::min);
            let t_max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            t_min + (t_max - t_min) * i as f64 / 199.0
        })
        .collect();

    let svi_model = match fit.model {
        FitModel::Bazin => mm_core::svi_models::SviModel::Bazin,
        FitModel::Villar => mm_core::svi_models::SviModel::Villar,
        FitModel::PowerLaw => mm_core::svi_models::SviModel::PowerLaw,
        FitModel::MetzgerKN => mm_core::svi_models::SviModel::MetzgerKN,
    };

    // Adjust parameters to be relative to first_mjd
    let mut adjusted_params = fit.parameters.clone();
    let t0_idx = match svi_model {
        mm_core::svi_models::SviModel::Bazin => 2,
        mm_core::svi_models::SviModel::Villar => 3,
        mm_core::svi_models::SviModel::PowerLaw => 3,
        mm_core::svi_models::SviModel::MetzgerKN => 3,
    };
    adjusted_params[t0_idx] = fit.t0 - first_mjd;

    let model_flux = eval_model_batch(svi_model, &adjusted_params, &model_times);

    // Create plot
    let root = BitMapBackend::new(output_path, (1024, 768)).into_drawing_area();
    root.fill(&WHITE)?;

    let y_min = norm_flux
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .min(0.0);
    let y_max = norm_flux
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(1.2);

    let x_min = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!(
                "{} - {} Model Fit\n{} - t0 = {:.2} ± {:.2} MJD",
                lightcurve.object_id,
                model_name(fit.model),
                description,
                fit.t0,
                fit.t0_err
            ),
            ("sans-serif", 30).into_font(),
        )
        .margin(10)
        .x_label_area_size(50)
        .y_label_area_size(60)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)?;

    chart
        .configure_mesh()
        .x_desc("Days since first detection")
        .y_desc("Normalized flux")
        .draw()?;

    // Plot data points with error bars
    chart.draw_series(
        times
            .iter()
            .zip(norm_flux.iter())
            .zip(norm_err.iter())
            .map(|((&t, &f), &e)| {
                ErrorBar::new_vertical(t, f - e, f, f + e, BLUE.filled(), 5)
            }),
    )?;

    // Plot data points
    chart.draw_series(
        times
            .iter()
            .zip(norm_flux.iter())
            .map(|(&t, &f)| Circle::new((t, f), 3, BLUE.filled())),
    )?;

    // Plot model fit
    chart
        .draw_series(LineSeries::new(
            model_times.iter().zip(model_flux.iter()).map(|(&t, &f)| (t, f)),
            &RED,
        ))?
        .label(format!(
            "SVI Fit (ELBO: {:.1})",
            fit.elbo
        ))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    // Mark t0 with vertical line
    let t0_relative = fit.t0 - first_mjd;
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(t0_relative, y_min), (t0_relative, y_max)],
        &GREEN.mix(0.7),
    )))?;

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}
