/// Debug normalization mismatch
///
/// Run with: cargo test --test debug_normalization -- --nocapture --ignored
use mm_core::svi_models;

#[test]
#[ignore]
fn debug_normalization() {
    println!("\n=== Debugging Normalization ===\n");

    let true_params = vec![-2.0, -1.0, 0.5, 0.0, -3.0];

    // Observation times
    let obs_times = vec![-0.5, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0];

    println!("True t0 = 0.0\n");
    println!("Model predictions for different t0 values:\n");

    for &test_t0 in &[0.0, 0.5, 1.0, 1.5, 2.0] {
        let mut test_params = true_params.clone();
        test_params[3] = test_t0; // Override t0

        let predictions =
            svi_models::eval_model_batch(svi_models::SviModel::MetzgerKN, &test_params, &obs_times);

        println!("t0 = {:.1}:", test_t0);
        for (i, (&t, &pred)) in obs_times.iter().zip(predictions.iter()).enumerate() {
            let phase = t - test_t0;
            println!("  t={:5.1}, phase={:6.1}, model_pred={:.4}", t, phase, pred);
        }

        let model_peak = predictions
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        println!("  Model peak value: {:.4}", model_peak);
        println!();
    }

    println!("\n=== Key Insight ===");
    println!("The Metzger model returns values normalized to [0, 1]");
    println!("(peak = 1.0), but the observed data is normalized by its own peak.");
    println!("If the model peak doesn't align with the observed peak,");
    println!("there's a systematic mismatch in the likelihood!");
}
