/// Find the true peak time of the Metzger model
///
/// Run with: cargo test --test find_peak_time -- --nocapture --ignored
use mm_core::svi_models;

#[test]
#[ignore]
fn find_peak_time() {
    println!("\n=== Finding Metzger Model Peak Time ===\n");

    let params = vec![-2.0, -1.0, 0.5, 0.0, -3.0]; // t0 = 0

    // Fine grid to find peak
    let times: Vec<f64> = (0..100).map(|i| 0.01 + i as f64 * 0.05).collect();

    let predictions =
        svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &params, &times);

    let (peak_idx, peak_val) = predictions
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap();

    let peak_time = times[peak_idx];

    println!("Metzger model (M_ej=0.01 Msun, v_ej=0.1c, κ_r=3.2 cm²/g):");
    println!("  Peak time: {:.3} days after t0", peak_time);
    println!("  Peak value (normalized): {:.4}", peak_val);
    println!();

    // Show predictions around our observation window
    println!("Predictions in observation window:");
    for &t in &[0.0, 0.25, 0.35, 0.4, 0.5, 0.75, 1.0, 1.5, 2.0] {
        let pred = svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &params, &[t])[0];
        let marker = if (t - peak_time).abs() < 0.1 {
            " ← PEAK"
        } else {
            ""
        };
        println!("  t={:.2}: pred={:.4}{}", t, pred, marker);
    }

    println!("\n=== Analysis ===");
    println!("If first detection is at t=0.5 days:");
    println!("  - Observed peak will be at t=0.5 (normalized to 1.0)");
    println!(
        "  - Model with t0=0 predicts {:.4} at t=0.5",
        svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &params, &[0.5])[0]
    );
    println!(
        "  - Mismatch: {:.4}",
        1.0 - svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &params, &[0.5])[0]
    );
    println!();
    println!("To make model peak align with observed peak at t=0.5:");
    println!("  Required: phase_peak = 0.5 - t0 = {:.3}", peak_time);
    println!(
        "  Solution: t0 = 0.5 - {:.3} = {:.3} days",
        peak_time,
        0.5 - peak_time
    );
    println!();
    println!("But we're getting t0 ≈ 1.7 days, which would place the peak at:");
    println!(
        "  t_peak = t0 + {:.3} = 1.7 + {:.3} = {:.3} days",
        peak_time,
        peak_time,
        1.7 + peak_time
    );
}
