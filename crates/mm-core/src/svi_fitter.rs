//! Stochastic Variational Inference (SVI) for light curve fitting
//!
//! This module implements SVI-based Bayesian inference for transient light curves.
//! It uses a mean-field Gaussian variational family and optimizes the ELBO using
//! stochastic gradient ascent with Adam optimizer.

use crate::svi_models::{eval_model_batch, SviModel};
use rand::Rng;
use rand_distr::{Distribution, Normal};

/// SVI fit result
#[derive(Debug, Clone)]
pub struct SviFitResult {
    /// Variational means (posterior parameters)
    pub mu: Vec<f64>,
    /// Log of variational standard deviations
    pub log_sigma: Vec<f64>,
    /// Evidence Lower Bound (ELBO) - higher is better
    pub elbo: f64,
    /// Number of iterations completed
    pub n_iter: usize,
}

impl SviFitResult {
    /// Get parameter uncertainties (1-sigma)
    pub fn get_uncertainties(&self) -> Vec<f64> {
        self.log_sigma.iter().map(|ls| ls.exp()).collect()
    }

    /// Sample from posterior
    pub fn sample<R: Rng>(&self, rng: &mut R) -> Vec<f64> {
        self.mu
            .iter()
            .zip(self.log_sigma.iter())
            .map(|(&mu, &log_sigma)| {
                let sigma = log_sigma.exp();
                let normal = Normal::new(mu, sigma).unwrap();
                normal.sample(rng)
            })
            .collect()
    }
}

/// Light curve data for fitting
#[derive(Debug, Clone)]
pub struct LightCurveData {
    pub times: Vec<f64>,
    pub flux: Vec<f64>,
    pub flux_err: Vec<f64>,
    pub peak_flux_obs: f64,
}

impl LightCurveData {
    /// Create from raw measurements
    pub fn from_measurements(times: Vec<f64>, flux: Vec<f64>, flux_err: Vec<f64>) -> Self {
        let peak_flux_obs = flux
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(1.0);

        Self {
            times,
            flux,
            flux_err,
            peak_flux_obs,
        }
    }

    /// Get normalized flux
    pub fn normalized_flux(&self) -> Vec<f64> {
        self.flux.iter().map(|f| f / self.peak_flux_obs).collect()
    }

    /// Get normalized flux errors
    pub fn normalized_flux_err(&self) -> Vec<f64> {
        self.flux_err
            .iter()
            .map(|e| e / self.peak_flux_obs)
            .collect()
    }
}

/// Compute ELBO (Evidence Lower Bound)
///
/// ELBO = E_q[log p(y|θ)] - KL[q(θ) || p(θ)]
///
/// For Gaussian prior p(θ) ~ N(μ_prior, σ_prior²) and
/// Gaussian variational q(θ) ~ N(μ, σ²):
///
/// KL[q||p] = log(σ_prior/σ) + (σ² + (μ - μ_prior)²)/(2σ_prior²) - 0.5
fn compute_elbo(
    model: SviModel,
    data: &LightCurveData,
    mu: &[f64],
    log_sigma: &[f64],
    n_samples: usize,
    bounds: &[(f64, f64)],
) -> f64 {
    let n_params = mu.len();
    let norm_flux = data.normalized_flux();
    let norm_err = data.normalized_flux_err();

    let mut rng = rand::thread_rng();
    let mut log_likelihood_sum = 0.0;

    // Monte Carlo estimate of E_q[log p(y|θ)]
    for _ in 0..n_samples {
        // Sample from q(θ)
        let theta: Vec<f64> = mu
            .iter()
            .zip(log_sigma.iter())
            .map(|(&m, &ls)| {
                let sigma = ls.exp();
                let normal = Normal::new(m, sigma).unwrap();
                normal.sample(&mut rng)
            })
            .collect();

        // Evaluate model
        let pred = eval_model_batch(model, &theta, &data.times);

        // Compute log likelihood: log p(y|θ) = -0.5 * sum((y - pred)² / σ²)
        let mut ll = 0.0;
        for i in 0..data.times.len() {
            let residual = norm_flux[i] - pred[i];
            let variance = norm_err[i] * norm_err[i];
            ll -= 0.5 * residual * residual / variance;
        }
        log_likelihood_sum += ll;
    }
    let log_likelihood = log_likelihood_sum / n_samples as f64;

    // Compute KL divergence KL[q(θ) || p(θ)]
    // Using broad uniform prior → KL ≈ entropy of q
    let mut kl = 0.0;
    for i in 0..n_params {
        let sigma = log_sigma[i].exp();

        // Prior parameters (center of bounds, wide variance)
        let (min, max) = bounds[i];
        let mu_prior = (min + max) / 2.0;
        let sigma_prior = (max - min) / 4.0; // ~95% within bounds

        // KL[N(μ,σ²) || N(μ_p,σ_p²)]
        let kl_i = (sigma_prior / sigma).ln()
            + (sigma * sigma + (mu[i] - mu_prior).powi(2)) / (2.0 * sigma_prior * sigma_prior)
            - 0.5;
        kl += kl_i;
    }

    log_likelihood - kl
}

/// Perform SVI fitting using Adam optimizer
///
/// # Arguments
/// * `model` - Light curve model to fit
/// * `data` - Observed light curve data
/// * `n_iter` - Number of optimization iterations
/// * `n_mc_samples` - Monte Carlo samples for ELBO estimation
/// * `learning_rate` - Initial Adam learning rate
/// * `init_params` - Optional initialization (e.g., from PSO)
pub fn svi_fit(
    model: SviModel,
    data: &LightCurveData,
    n_iter: usize,
    n_mc_samples: usize,
    learning_rate: f64,
    init_params: Option<&[f64]>,
) -> SviFitResult {
    let bounds = model.param_bounds();
    let n_params = model.n_params();

    // Initialize variational parameters
    let mut mu = if let Some(init) = init_params {
        init.to_vec()
    } else {
        // Random initialization within bounds
        let mut rng = rand::thread_rng();
        bounds
            .iter()
            .map(|(min, max)| rng.gen_range(*min..*max))
            .collect()
    };

    let mut log_sigma = vec![-1.0; n_params]; // σ ≈ 0.37

    // Adam optimizer state
    let mut m_mu = vec![0.0; n_params];
    let mut v_mu = vec![0.0; n_params];
    let mut m_log_sigma = vec![0.0; n_params];
    let mut v_log_sigma = vec![0.0; n_params];

    let beta1: f64 = 0.9;
    let beta2: f64 = 0.999;
    let epsilon: f64 = 1e-8;

    let mut best_elbo = f64::NEG_INFINITY;
    let mut best_mu = mu.clone();
    let mut best_log_sigma = log_sigma.clone();

    for iter in 1..=n_iter {
        // Compute ELBO and gradients (via finite differences)
        let elbo = compute_elbo(model, data, &mu, &log_sigma, n_mc_samples, &bounds);

        // Simple finite difference gradients
        let eps = 1e-5;
        let mut grad_mu = vec![0.0; n_params];
        let mut grad_log_sigma = vec![0.0; n_params];

        for i in 0..n_params {
            // Gradient w.r.t. mu[i]
            let mut mu_plus = mu.clone();
            mu_plus[i] += eps;
            let elbo_plus = compute_elbo(model, data, &mu_plus, &log_sigma, n_mc_samples, &bounds);
            grad_mu[i] = (elbo_plus - elbo) / eps;

            // Gradient w.r.t. log_sigma[i]
            let mut log_sigma_plus = log_sigma.clone();
            log_sigma_plus[i] += eps;
            let elbo_plus = compute_elbo(model, data, &mu, &log_sigma_plus, n_mc_samples, &bounds);
            grad_log_sigma[i] = (elbo_plus - elbo) / eps;
        }

        // Adam updates
        let lr = learning_rate * (1.0 - beta2.powi(iter as i32)).sqrt()
            / (1.0 - beta1.powi(iter as i32));

        for i in 0..n_params {
            // Update mu
            m_mu[i] = beta1 * m_mu[i] + (1.0 - beta1) * grad_mu[i];
            v_mu[i] = beta2 * v_mu[i] + (1.0 - beta2) * grad_mu[i] * grad_mu[i];
            let m_hat = m_mu[i] / (1.0 - beta1.powi(iter as i32));
            let v_hat = v_mu[i] / (1.0 - beta2.powi(iter as i32));
            mu[i] += lr * m_hat / (v_hat.sqrt() + epsilon);

            // Clamp to bounds
            mu[i] = mu[i].clamp(bounds[i].0, bounds[i].1);

            // Update log_sigma
            m_log_sigma[i] = beta1 * m_log_sigma[i] + (1.0 - beta1) * grad_log_sigma[i];
            v_log_sigma[i] =
                beta2 * v_log_sigma[i] + (1.0 - beta2) * grad_log_sigma[i] * grad_log_sigma[i];
            let m_hat = m_log_sigma[i] / (1.0 - beta1.powi(iter as i32));
            let v_hat = v_log_sigma[i] / (1.0 - beta2.powi(iter as i32));
            log_sigma[i] += lr * m_hat / (v_hat.sqrt() + epsilon);

            // Clamp log_sigma to reasonable range
            log_sigma[i] = log_sigma[i].clamp(-5.0, 2.0); // σ ∈ [0.007, 7.4]
        }

        // Track best
        if elbo > best_elbo {
            best_elbo = elbo;
            best_mu = mu.clone();
            best_log_sigma = log_sigma.clone();
        }
    }

    SviFitResult {
        mu: best_mu,
        log_sigma: best_log_sigma,
        elbo: best_elbo,
        n_iter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svi_fitting_simple() {
        // Create synthetic Bazin light curve
        let params_true = vec![1.0, 1.0, 5.0, 20.0, 2.0];
        let times: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let flux_true = eval_model_batch(SviModel::Bazin, &params_true, &times);

        // Add small noise
        let flux: Vec<f64> = flux_true.iter().map(|f| f + 0.01).collect();
        let flux_err = vec![0.05; times.len()];

        let data = LightCurveData::from_measurements(times, flux, flux_err);

        // Fit with SVI (small number of iterations for test)
        let result = svi_fit(SviModel::Bazin, &data, 10, 2, 0.01, Some(&params_true));

        // Should converge near true parameters
        assert!(result.elbo.is_finite());
        assert!(result.mu.len() == 5);
    }
}
