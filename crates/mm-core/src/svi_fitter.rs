//! Stochastic Variational Inference for light curve fitting
//!
//! Implements mean-field Gaussian variational inference with the
//! reparameterization trick and analytical gradients for efficient
//! Bayesian parameter estimation.

use crate::pso_fitter::BandFitData;
use crate::svi_models::{eval_model_batch, eval_model_grad_batch, SviModel};
use rand::Rng;

/// Log of the standard normal CDF Φ(x)
/// Used for upper limit likelihood calculation
fn log_normal_cdf(x: f64) -> f64 {
    if x > 8.0 {
        return 0.0; // Φ(x) ≈ 1
    }
    if x < -30.0 {
        return -0.5 * x * x - 0.5 * (2.0 * std::f64::consts::PI).ln() - (-x).ln();
    }
    // Use erfc approximation (Abramowitz & Stegun 7.1.26)
    let z = -x * std::f64::consts::FRAC_1_SQRT_2; // erfc(z) = 2*Φ(x) when z = -x/sqrt(2)
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erfc_z = poly * (-z * z).exp();
    let phi = if z >= 0.0 {
        0.5 * erfc_z
    } else {
        1.0 - 0.5 * erfc_z
    };
    (phi.max(1e-300)).ln()
}

/// Derivative of log Φ(x) with respect to x
/// Used for gradient calculation with upper limits
fn dlog_normal_cdf_dx(x: f64) -> f64 {
    // d/dx log Φ(x) = φ(x) / Φ(x) where φ is the normal PDF
    let phi = (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let log_cdf = log_normal_cdf(x);
    phi / log_cdf.exp().max(1e-300)
}

/// Manual Adam optimizer (avoids built-in Adam issues)
struct ManualAdam {
    m: Vec<f64>,
    v: Vec<f64>,
    lr: f64,
    beta1: f64,
    beta2: f64,
    eps: f64,
    t: usize,
}

impl ManualAdam {
    fn new(n_params: usize, lr: f64) -> Self {
        Self {
            m: vec![0.0; n_params],
            v: vec![0.0; n_params],
            lr,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            t: 0,
        }
    }

    fn step(&mut self, params: &mut [f64], grads: &[f64]) {
        self.t += 1;
        let bc1 = 1.0 - self.beta1.powi(self.t as i32);
        let bc2 = 1.0 - self.beta2.powi(self.t as i32);
        for i in 0..params.len() {
            let g = grads[i];
            if !g.is_finite() {
                continue;
            }
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * g;
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * g * g;
            let m_hat = self.m[i] / bc1;
            let v_hat = self.v[i] / bc2;
            params[i] -= self.lr * m_hat / (v_hat.sqrt() + self.eps);
        }
    }
}

/// SVI fit result
pub struct SviFitResult {
    pub model: SviModel,
    pub mu: Vec<f64>,        // Variational means (unconstrained space)
    pub log_sigma: Vec<f64>, // Log of variational stds
    pub elbo: f64,           // Final ELBO estimate
}

/// Gaussian priors for each model's parameters
fn prior_params(model: SviModel) -> Vec<(f64, f64)> {
    // Returns (center, width) for each parameter
    match model {
        SviModel::Bazin => {
            // log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            vec![
                (0.0, 2.0),
                (0.0, 0.5),
                (0.0, 50.0),
                (1.0, 2.0),
                (3.0, 2.0),
                (-2.0, 2.0),
            ]
        }
        SviModel::Villar => {
            // log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            vec![
                (0.0, 2.0),
                (0.0, 0.05),
                (2.0, 2.0),
                (0.0, 50.0),
                (1.0, 2.0),
                (3.5, 2.0),
                (-2.0, 2.0),
            ]
        }
        SviModel::PowerLaw => {
            // log_a, log_alpha, log_beta, t0, log_sigma_extra
            vec![(0.0, 2.0), (0.0, 1.0), (0.4, 1.0), (0.0, 50.0), (-2.0, 2.0)]
        }
        SviModel::MetzgerKN => {
            // log10_mej, log10_vej, log10_kappa_r, t0, log_sigma_extra
            vec![
                (-2.5, 1.0),
                (-1.0, 0.5),
                (1.0, 1.0),
                (0.0, 50.0),
                (-2.0, 2.0),
            ]
        }
    }
}

/// Run SVI optimization
pub fn svi_fit(
    model: SviModel,
    data: &BandFitData,
    n_steps: usize,
    n_samples: usize,
    lr: f64,
    pso_init: Option<&[f64]>,
    enable_safeguards: bool,
    scale_clamp_range: (f64, f64),
) -> SviFitResult {
    let n_params = model.n_params();
    let n_variational = 2 * n_params; // mu + log_sigma for each param

    // Initialize variational parameters
    let mut var_params = vec![0.0; n_variational];
    if let Some(pso_params) = pso_init {
        // Use PSO initialization for mu
        for i in 0..n_params {
            var_params[i] = pso_params[i]; // mu
            var_params[n_params + i] = -1.0; // log_sigma (sigma ~ 0.37)
        }
    } else {
        // Fallback initialization
        for i in 0..n_params {
            var_params[i] = 0.0; // mu
            var_params[n_params + i] = -1.0; // log_sigma
        }
    }

    let mut adam = ManualAdam::new(n_variational, lr);

    // Precompute observational variance
    let obs_var: Vec<f64> = data.flux_err.iter().map(|e| e * e + 1e-10).collect();

    // Index of log_sigma_extra in the parameter vector
    let se_idx = model.sigma_extra_idx();

    let mut final_elbo = f64::NEG_INFINITY;
    let mut rng = rand::thread_rng();

    for _step in 0..n_steps {
        let mu = &var_params[..n_params];
        let log_sigma = &var_params[n_params..];
        let sigma: Vec<f64> = log_sigma.iter().map(|ls| ls.exp()).collect();

        // Accumulators for gradients
        let mut grad_mu = vec![0.0; n_params];
        let mut grad_log_sigma = vec![0.0; n_params];
        let mut elbo_sum = 0.0;

        for _ in 0..n_samples {
            // Draw epsilon ~ N(0, 1) and compute theta via reparameterization
            let mut eps = vec![0.0; n_params];
            let mut theta = vec![0.0; n_params];
            for j in 0..n_params {
                // Box-Muller transform
                let u1: f64 = rng.gen::<f64>().max(1e-10);
                let u2: f64 = rng.gen();
                eps[j] = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                theta[j] = mu[j] + sigma[j] * eps[j];
            }

            // sigma_extra = exp(log_sigma_extra)
            let sigma_extra = theta[se_idx].exp();
            let sigma_extra_sq = sigma_extra * sigma_extra;

            // Compute log-likelihood and its gradient w.r.t. theta
            let mut preds = eval_model_batch(model, &theta, &data.times);
            let mut grads = eval_model_grad_batch(model, &theta, &data.times);

            // CRITICAL FIX: Renormalize MetzgerKN predictions to match observed peak
            // The Metzger model returns values normalized by the model's internal peak,
            // but observations are normalized by max(observed flux). If the true peak
            // occurs before first detection, this creates a systematic bias in t0.
            let _scale_factor = if model == SviModel::MetzgerKN {
                // Find max prediction at DETECTION times only (not upper limits)
                let max_pred = preds
                    .iter()
                    .zip(data.is_upper.iter())
                    .filter(|(_, &is_up)| !is_up)
                    .map(|(p, _)| *p)
                    .fold(f64::NEG_INFINITY, f64::max);

                // Renormalize so max(predictions at detections) = 1.0
                if max_pred > 1e-10 && max_pred.is_finite() {
                    let scale = 1.0 / max_pred;

                    if enable_safeguards {
                        // NUMERICAL SAFEGUARDS: Clamp scale factor to prevent instability
                        let scale_clamped = scale.clamp(scale_clamp_range.0, scale_clamp_range.1);

                        // If we had to clamp significantly, skip this sample (bad parameter region)
                        if (scale - scale_clamped).abs() / scale > 0.5 {
                            // Return zero likelihood to skip this sample
                            continue; // Skip to next Monte Carlo sample
                        }

                        for pred in preds.iter_mut() {
                            *pred *= scale_clamped;
                            // Safety check after scaling
                            if !pred.is_finite() {
                                continue; // Skip this sample
                            }
                        }
                        // Gradients must also be scaled: d(scale*f)/dθ = scale * df/dθ
                        for grad_vec in grads.iter_mut() {
                            for grad in grad_vec.iter_mut() {
                                *grad *= scale_clamped;
                            }
                        }
                        scale_clamped
                    } else {
                        // No safeguards - apply scale factor directly
                        for pred in preds.iter_mut() {
                            *pred *= scale;
                            if !pred.is_finite() {
                                continue; // Still check for NaN/Inf
                            }
                        }
                        for grad_vec in grads.iter_mut() {
                            for grad in grad_vec.iter_mut() {
                                *grad *= scale;
                            }
                        }
                        scale
                    }
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let mut log_lik = 0.0;
            let mut dll_dtheta = vec![0.0; n_params];

            for i in 0..data.times.len() {
                let pred = preds[i];
                if !pred.is_finite() {
                    continue;
                }

                let total_var = obs_var[i] + sigma_extra_sq;
                let sigma_total = total_var.sqrt();

                if data.is_upper[i] {
                    // Upper limit: log Φ((f_upper - f_pred) / σ_total)
                    let z = (data.upper_flux[i] - pred) / sigma_total;
                    log_lik += log_normal_cdf(z);

                    // Gradient: d/dθ log Φ(z) = -φ(z)/Φ(z) * d_pred/dθ / σ_total
                    let dlog_phi_dz = dlog_normal_cdf_dx(z);
                    let dz_dpred = -1.0 / sigma_total;

                    for j in 0..n_params {
                        if j != se_idx && grads[i][j].is_finite() {
                            dll_dtheta[j] += dlog_phi_dz * dz_dpred * grads[i][j];
                        }
                    }

                    // Gradient w.r.t. log_sigma_extra (via total_var)
                    let dz_dsigma = (data.upper_flux[i] - pred) / (total_var * sigma_total);
                    dll_dtheta[se_idx] += dlog_phi_dz * dz_dsigma * sigma_extra_sq;
                } else {
                    // Detection: standard Gaussian likelihood
                    let residual = data.flux[i] - pred;
                    let inv_total = 1.0 / total_var;
                    let r2 = residual * residual;
                    log_lik +=
                        -0.5 * (r2 * inv_total + (2.0 * std::f64::consts::PI * total_var).ln());

                    // Gradient w.r.t. flux model parameters
                    for j in 0..n_params {
                        if j != se_idx && grads[i][j].is_finite() {
                            dll_dtheta[j] += residual * inv_total * grads[i][j];
                        }
                    }

                    // Gradient w.r.t. log_sigma_extra
                    dll_dtheta[se_idx] += (r2 * inv_total * inv_total - inv_total) * sigma_extra_sq;
                }
            }

            // Log-prior: Gaussian priors on parameters
            let priors = prior_params(model);
            let mut log_prior = 0.0;
            let mut dlp_dtheta = vec![0.0; n_params];
            for j in 0..n_params {
                let (center, width) = priors[j];
                let var = width * width;
                log_prior += -0.5 * (theta[j] - center).powi(2) / var;
                dlp_dtheta[j] = -(theta[j] - center) / var;
            }

            elbo_sum += log_lik + log_prior;

            // Reparameterization trick gradients:
            // d(ELBO)/d(mu_j) = d(log_lik+log_prior)/d(theta_j)
            // d(ELBO)/d(log_sigma_j) = d(log_lik+log_prior)/d(theta_j) * sigma_j * eps_j
            for j in 0..n_params {
                let df_dtheta = dll_dtheta[j] + dlp_dtheta[j];
                grad_mu[j] += df_dtheta;
                grad_log_sigma[j] += df_dtheta * sigma[j] * eps[j];
            }
        }

        // Average over samples
        let ns = n_samples as f64;
        for j in 0..n_params {
            grad_mu[j] /= ns;
            grad_log_sigma[j] /= ns;
        }
        elbo_sum /= ns;

        // Add entropy: H[q] = sum(log_sigma_j) + 0.5 * P * ln(2*pi*e)
        let entropy: f64 = log_sigma.iter().sum::<f64>()
            + 0.5 * n_params as f64 * (2.0 * std::f64::consts::PI * std::f64::consts::E).ln();
        final_elbo = elbo_sum + entropy;

        // d(entropy)/d(log_sigma_j) = 1
        for j in 0..n_params {
            grad_log_sigma[j] += 1.0;
        }

        // Build the full gradient of -ELBO (we minimize -ELBO)
        let mut neg_elbo_grad = Vec::with_capacity(n_variational);
        for j in 0..n_params {
            neg_elbo_grad.push(-grad_mu[j]);
        }
        for j in 0..n_params {
            neg_elbo_grad.push(-grad_log_sigma[j]);
        }

        // Adam step
        adam.step(&mut var_params, &neg_elbo_grad);

        // Clamp log_sigma to prevent collapse or explosion
        for i in 0..n_params {
            var_params[n_params + i] = var_params[n_params + i].clamp(-6.0, 2.0);
        }
    }

    SviFitResult {
        model,
        mu: var_params[..n_params].to_vec(),
        log_sigma: var_params[n_params..].to_vec(),
        elbo: final_elbo,
    }
}

/// Run SVI optimization with t0 fixed
///
/// This is used for profile likelihood optimization where we fix t0
/// and optimize all other parameters. Reduces dimensionality and
/// eliminates multi-modality in t0.
///
/// # Returns
///
/// SviFitResult with parameters EXCLUDING t0 (must be inserted manually)
pub fn svi_fit_fixed_t0(
    model: SviModel,
    data: &BandFitData,
    t0_fixed: f64,
    n_steps: usize,
    n_samples: usize,
    lr: f64,
    enable_safeguards: bool,
    scale_clamp_range: (f64, f64),
) -> SviFitResult {
    let n_params_full = model.n_params();
    let t0_idx = model.t0_idx();

    // Number of parameters excluding t0
    let n_params = n_params_full - 1;
    let n_variational = 2 * n_params;

    // Initialize variational parameters (excluding t0)
    let mut var_params = vec![0.0; n_variational];
    let priors = prior_params(model);

    for i in 0..n_params {
        // Map index accounting for t0 position
        let full_idx = if i < t0_idx { i } else { i + 1 };
        var_params[i] = priors[full_idx].0; // mu
        var_params[n_params + i] = -1.0; // log_sigma
    }

    let mut optimizer = ManualAdam::new(n_variational, lr);
    let mut rng = rand::thread_rng();

    let mut final_elbo = f64::NEG_INFINITY;

    for step in 0..n_steps {
        let mut total_elbo = 0.0;
        let mut total_grad = vec![0.0; n_variational];

        for _ in 0..n_samples {
            // Sample from variational distribution (excluding t0)
            let mut theta_reduced = Vec::with_capacity(n_params);
            for i in 0..n_params {
                let eps: f64 = rng.gen::<f64>() * 2.0 - 1.0;
                let sigma = var_params[n_params + i].exp();
                theta_reduced.push(var_params[i] + eps * sigma);
            }

            // Reconstruct full parameter vector with fixed t0
            let mut theta = Vec::with_capacity(n_params_full);
            for i in 0..n_params_full {
                if i == t0_idx {
                    theta.push(t0_fixed);
                } else {
                    let reduced_idx = if i < t0_idx { i } else { i - 1 };
                    theta.push(theta_reduced[reduced_idx]);
                }
            }

            // Compute log-likelihood and gradients
            let mut preds = eval_model_batch(model, &theta, &data.times);
            let mut grads = eval_model_grad_batch(model, &theta, &data.times);

            // Apply MetzgerKN normalization if needed
            if model == SviModel::MetzgerKN {
                let max_pred = preds
                    .iter()
                    .zip(data.is_upper.iter())
                    .filter(|(_, &is_up)| !is_up)
                    .map(|(p, _)| *p)
                    .fold(f64::NEG_INFINITY, f64::max);

                if max_pred > 1e-10 && max_pred.is_finite() {
                    let scale = 1.0 / max_pred;

                    if enable_safeguards {
                        let scale_clamped = scale.clamp(scale_clamp_range.0, scale_clamp_range.1);
                        if (scale - scale_clamped).abs() / scale > 0.5 {
                            continue;
                        }
                        for pred in preds.iter_mut() {
                            *pred *= scale_clamped;
                            if !pred.is_finite() {
                                continue;
                            }
                        }
                        for grad_vec in grads.iter_mut() {
                            for grad in grad_vec.iter_mut() {
                                *grad *= scale_clamped;
                            }
                        }
                    } else {
                        for pred in preds.iter_mut() {
                            *pred *= scale;
                            if !pred.is_finite() {
                                continue;
                            }
                        }
                        for grad_vec in grads.iter_mut() {
                            for grad in grad_vec.iter_mut() {
                                *grad *= scale;
                            }
                        }
                    }
                }
            }

            let mut log_lik = 0.0;
            let mut dll_dtheta_full = vec![0.0; n_params_full];

            // Compute likelihood
            let se_idx_full = model.sigma_extra_idx();
            let sigma_extra = theta[se_idx_full].exp();
            let sigma_extra_sq = sigma_extra * sigma_extra;

            for i in 0..data.times.len() {
                let pred = preds[i];
                if !pred.is_finite() {
                    continue;
                }

                let total_var = data.flux_err[i] * data.flux_err[i] + sigma_extra_sq + 1e-10;
                let sigma_total = total_var.sqrt();

                if data.is_upper[i] {
                    let z = (data.upper_flux[i] - pred) / sigma_total;
                    log_lik += log_normal_cdf(z);

                    let dlog_phi_dz = dlog_normal_cdf_dx(z);
                    let dz_dpred = -1.0 / sigma_total;
                    for j in 0..n_params_full {
                        dll_dtheta_full[j] += dlog_phi_dz * dz_dpred * grads[i][j];
                    }
                } else {
                    let diff = pred - data.flux[i];
                    log_lik += -0.5 * (diff * diff / total_var + total_var.ln());

                    for j in 0..n_params_full {
                        dll_dtheta_full[j] += -diff / total_var * grads[i][j];
                    }
                }
            }

            // Extract gradients for non-t0 parameters
            let mut dll_dtheta = Vec::with_capacity(n_params);
            for i in 0..n_params_full {
                if i != t0_idx {
                    dll_dtheta.push(dll_dtheta_full[i]);
                }
            }

            // Compute prior contribution
            let mut log_prior = 0.0;
            let mut dlp_dtheta = Vec::with_capacity(n_params);

            for i in 0..n_params {
                let full_idx = if i < t0_idx { i } else { i + 1 };
                let (center, width) = priors[full_idx];
                let diff = theta_reduced[i] - center;
                log_prior += -0.5 * (diff / width).powi(2);
                dlp_dtheta.push(-diff / (width * width));
            }

            // ELBO = E[log p(data|θ)] + E[log p(θ)] - E[log q(θ)]
            let mut entropy = 0.0;
            for i in 0..n_params {
                entropy +=
                    var_params[n_params + i] + 0.5 * (1.0 + (2.0 * std::f64::consts::PI).ln());
            }

            let elbo = log_lik + log_prior + entropy;
            total_elbo += elbo;

            // Gradients w.r.t. variational parameters
            for i in 0..n_params {
                // Gradient w.r.t. mu
                total_grad[i] += dll_dtheta[i] + dlp_dtheta[i];

                // Gradient w.r.t. log_sigma
                let sigma = var_params[n_params + i].exp();
                total_grad[n_params + i] += (dll_dtheta[i] + dlp_dtheta[i]) * sigma + 1.0;
            }
        }

        // Average gradients and ELBO
        for g in total_grad.iter_mut() {
            *g /= n_samples as f64;
        }
        final_elbo = total_elbo / n_samples as f64;

        // Update variational parameters
        optimizer.step(&mut var_params, &total_grad);

        // Clamp log_sigma
        for i in 0..n_params {
            var_params[n_params + i] = var_params[n_params + i].clamp(-6.0, 2.0);
        }
    }

    SviFitResult {
        model,
        mu: var_params[..n_params].to_vec(),
        log_sigma: var_params[n_params..].to_vec(),
        elbo: final_elbo,
    }
}
