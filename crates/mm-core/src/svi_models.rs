//! SVI light curve models with analytical gradients
//!
//! Physical and empirical models for fitting transient light curves using
//! Stochastic Variational Inference. All models use proper parameterization
//! with log transforms for positive parameters and include analytical gradients
//! for efficient optimization.

use std::f64::consts::PI;

/// Physical constants (CGS)
const MSUN_CGS: f64 = 1.989e33; // grams
const C_CGS: f64 = 2.998e10; // cm/s
const SECS_PER_DAY: f64 = 86400.0;

/// Light curve model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SviModel {
    Bazin,
    Villar,
    PowerLaw,
    MetzgerKN,
}

impl SviModel {
    pub fn name(&self) -> &'static str {
        match self {
            SviModel::Bazin => "Bazin",
            SviModel::Villar => "Villar",
            SviModel::PowerLaw => "PowerLaw",
            SviModel::MetzgerKN => "MetzgerKN",
        }
    }

    pub fn n_params(&self) -> usize {
        match self {
            SviModel::Bazin => 6, // log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            SviModel::Villar => 7, // log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra
            SviModel::PowerLaw => 5, // log_a, log_alpha, log_beta, t0, log_sigma_extra
            SviModel::MetzgerKN => 5, // log10_mej, log10_vej, log10_kappa_r, t0, log_sigma_extra
        }
    }

    /// Index of log_sigma_extra in the parameter vector
    pub fn sigma_extra_idx(&self) -> usize {
        self.n_params() - 1
    }

    /// Index of t0 in the parameter vector
    pub fn t0_idx(&self) -> usize {
        match self {
            SviModel::Bazin => 2,
            SviModel::Villar => 3,
            SviModel::PowerLaw => 3,
            SviModel::MetzgerKN => 3,
        }
    }

    pub fn param_names(&self) -> Vec<&'static str> {
        match self {
            SviModel::Bazin => vec![
                "log_a",
                "b",
                "t0",
                "log_tau_rise",
                "log_tau_fall",
                "log_sigma_extra",
            ],
            SviModel::Villar => vec![
                "log_a",
                "beta",
                "log_gamma",
                "t0",
                "log_tau_rise",
                "log_tau_fall",
                "log_sigma_extra",
            ],
            SviModel::PowerLaw => vec!["log_a", "log_alpha", "log_beta", "t0", "log_sigma_extra"],
            SviModel::MetzgerKN => vec![
                "log10_mej",
                "log10_vej",
                "log10_kappa_r",
                "t0",
                "log_sigma_extra",
            ],
        }
    }

    /// Whether this model requires batch (whole-lightcurve) evaluation.
    pub fn is_sequential(&self) -> bool {
        matches!(self, SviModel::MetzgerKN)
    }
}

// ---------------------------------------------------------------------------
// Bazin model: empirical supernova light curve
// ---------------------------------------------------------------------------

/// Bazin model flux evaluation
/// params: [log_a, b, t0, log_tau_rise, log_tau_fall]
pub fn bazin_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0].exp();
    let b = params[1];
    let t0 = params[2];
    let tau_rise = params[3].exp();
    let tau_fall = params[4].exp();
    let dt = t - t0;
    let e_fall = (-dt / tau_fall).exp();
    let sig = 1.0 / (1.0 + (-dt / tau_rise).exp());
    a * e_fall * sig + b
}

/// Analytical gradient of Bazin model
/// params: [log_a, b, t0, log_tau_rise, log_tau_fall, log_sigma_extra]
pub fn bazin_flux_grad(params: &[f64], t: f64) -> Vec<f64> {
    let a = params[0].exp();
    let t0 = params[2];
    let tau_rise = params[3].exp();
    let tau_fall = params[4].exp();
    let dt = t - t0;
    let e_fall = (-dt / tau_fall).exp();
    let sig = 1.0 / (1.0 + (-dt / tau_rise).exp());
    let base = a * e_fall * sig; // flux - b

    // d(flux)/d(log_a) = a * e_fall * sig = base
    let d_log_a = base;

    // d(flux)/d(b) = 1
    let d_b = 1.0;

    // d(flux)/d(t0)
    let d_t0 = base * (1.0 / tau_fall - (1.0 - sig) / tau_rise);

    // d(flux)/d(log_tau_rise)
    let d_log_tau_rise = -base * (1.0 - sig) * dt / tau_rise;

    // d(flux)/d(log_tau_fall)
    let d_log_tau_fall = base * dt / tau_fall;

    // d(flux)/d(log_sigma_extra) = 0 (affects likelihood, not flux prediction)
    vec![d_log_a, d_b, d_t0, d_log_tau_rise, d_log_tau_fall, 0.0]
}

// ---------------------------------------------------------------------------
// Villar model: improved empirical model
// ---------------------------------------------------------------------------

/// Villar model flux evaluation
/// params: [log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall]
pub fn villar_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0].exp();
    let beta = params[1];
    let gamma = params[2].exp();
    let t0 = params[3];
    let tau_rise = params[4].exp();
    let tau_fall = params[5].exp();
    let phase = t - t0;
    let sig_rise = 1.0 / (1.0 + (-phase / tau_rise).exp());
    let k = 10.0;
    let w = 1.0 / (1.0 + (-k * (phase - gamma)).exp());
    let piece_left = 1.0 - beta * phase;
    let piece_right = (1.0 - beta * gamma) * ((gamma - phase) / tau_fall).exp();
    let piece = (1.0 - w) * piece_left + w * piece_right;
    a * sig_rise * piece
}

/// Analytical gradient of Villar model
/// params: [log_a, beta, log_gamma, t0, log_tau_rise, log_tau_fall, log_sigma_extra]
pub fn villar_flux_grad(params: &[f64], t: f64) -> Vec<f64> {
    let a = params[0].exp();
    let beta = params[1];
    let gamma = params[2].exp();
    let t0 = params[3];
    let tau_rise = params[4].exp();
    let tau_fall = params[5].exp();
    let phase = t - t0;
    let k = 10.0;

    let sig_rise = 1.0 / (1.0 + (-phase / tau_rise).exp());
    let w = 1.0 / (1.0 + (-k * (phase - gamma)).exp());
    let piece_left = 1.0 - beta * phase;
    let e_decay = ((gamma - phase) / tau_fall).exp();
    let piece_right = (1.0 - beta * gamma) * e_decay;
    let piece = (1.0 - w) * piece_left + w * piece_right;
    let flux = a * sig_rise * piece;

    // d(flux)/d(log_a) = flux
    let d_log_a = flux;

    // d(flux)/d(beta)
    let d_pl_dbeta = -phase;
    let d_pr_dbeta = -gamma * e_decay;
    let d_piece_dbeta = (1.0 - w) * d_pl_dbeta + w * d_pr_dbeta;
    let d_beta = a * sig_rise * d_piece_dbeta;

    // d(flux)/d(log_gamma): gamma = exp(log_gamma)
    let dw_dgamma = -k * w * (1.0 - w);
    let dw_dloggamma = dw_dgamma * gamma;
    let dpr_dgamma = e_decay * (-beta + (1.0 - beta * gamma) / tau_fall);
    let dpr_dloggamma = dpr_dgamma * gamma;
    let d_piece_dloggamma = dw_dloggamma * (piece_right - piece_left) + w * dpr_dloggamma;
    let d_log_gamma = a * sig_rise * d_piece_dloggamma;

    // d(flux)/d(t0)
    let dsig_dphase = sig_rise * (1.0 - sig_rise) / tau_rise;
    let dsig_dt0 = -dsig_dphase;
    let dw_dphase = k * w * (1.0 - w);
    let dw_dt0 = -dw_dphase;
    let dpl_dt0 = beta;
    let dpr_dt0 = (1.0 - beta * gamma) * e_decay / tau_fall;
    let d_piece_dt0 = dw_dt0 * (piece_right - piece_left) + (1.0 - w) * dpl_dt0 + w * dpr_dt0;
    let d_t0 = a * (dsig_dt0 * piece + sig_rise * d_piece_dt0);

    // d(flux)/d(log_tau_rise)
    let d_log_tau_rise = a * piece * sig_rise * (1.0 - sig_rise) * (-phase / tau_rise);

    // d(flux)/d(log_tau_fall)
    let d_pr_dlogtf = piece_right * (phase - gamma) / tau_fall;
    let d_log_tau_fall = a * sig_rise * w * d_pr_dlogtf;

    vec![
        d_log_a,
        d_beta,
        d_log_gamma,
        d_t0,
        d_log_tau_rise,
        d_log_tau_fall,
        0.0,
    ]
}

// ---------------------------------------------------------------------------
// Power-law model: simple rise and decay
// ---------------------------------------------------------------------------

/// Power-law model flux evaluation
/// params: [log_a, log_alpha, log_beta, t0]
pub fn powerlaw_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0].exp();
    let alpha = params[1].exp();
    let beta = params[2].exp();
    let t0 = params[3];

    let dt = t - t0;
    if dt <= 0.0 {
        return 0.0;
    }

    a * dt.powf(alpha) * (-dt.powf(beta)).exp()
}

/// Analytical gradient of power-law model
/// params: [log_a, log_alpha, log_beta, t0, log_sigma_extra]
pub fn powerlaw_flux_grad(params: &[f64], t: f64) -> Vec<f64> {
    let a = params[0].exp();
    let alpha = params[1].exp();
    let beta = params[2].exp();
    let t0 = params[3];

    let dt = t - t0;
    if dt <= 1e-10 {
        return vec![0.0; 5];
    }

    let dt_alpha = dt.powf(alpha);
    let dt_beta = dt.powf(beta);
    let exp_decay = (-dt_beta).exp();
    let flux = a * dt_alpha * exp_decay;

    // d(flux)/d(log_a) = flux
    let d_log_a = flux;

    // d(flux)/d(log_alpha) = flux * ln(dt) * alpha (chain rule for log_alpha)
    let d_log_alpha = flux * dt.ln() * alpha;

    // d(flux)/d(log_beta) = flux * (-dt^beta * ln(dt)) * beta
    let d_log_beta = -flux * dt_beta * dt.ln() * beta;

    // d(flux)/d(t0) = flux * (-alpha/dt + beta * dt^(beta-1))
    let d_t0 = flux * (-alpha / dt + beta * dt.powf(beta - 1.0));

    vec![d_log_a, d_log_alpha, d_log_beta, d_t0, 0.0]
}

// ---------------------------------------------------------------------------
// Metzger kilonova model (1-zone approximation)
// ---------------------------------------------------------------------------

/// Metzger KN model batch evaluation
/// params: [log10_mej, log10_vej, log10_kappa_r, t0]
pub fn metzger_kn_eval_batch(params: &[f64], obs_times: &[f64]) -> Vec<f64> {
    let m_ej = 10f64.powf(params[0]) * MSUN_CGS;
    let v_ej = 10f64.powf(params[1]) * C_CGS;
    let kappa_r = 10f64.powf(params[2]);
    let t0 = params[3];

    let phases: Vec<f64> = obs_times.iter().map(|&t| t - t0).collect();
    let phase_max = phases.iter().cloned().fold(0.01f64, f64::max);
    if phase_max <= 0.01 {
        return vec![0.0; obs_times.len()];
    }

    // Fine log-spaced integration grid (days)
    let n_grid: usize = 200;
    let log_t_min = 0.01f64.ln();
    let log_t_max = (phase_max * 1.05).ln();
    let grid_t_day: Vec<f64> = (0..n_grid)
        .map(|i| (log_t_min + (log_t_max - log_t_min) * i as f64 / (n_grid - 1) as f64).exp())
        .collect();

    // Neutron composition parameters
    let ye: f64 = 0.1;
    let xn0: f64 = 1.0 - 2.0 * ye;

    // Initial conditions (scaled by 1e40)
    let scale: f64 = 1e40;
    let e0 = 0.5 * m_ej * v_ej * v_ej;
    let mut e_th = e0 / scale;
    let mut e_kin = e0 / scale;
    let mut v = v_ej;
    let mut r = grid_t_day[0] * SECS_PER_DAY * v;

    let mut grid_lrad: Vec<f64> = Vec::with_capacity(n_grid);

    for i in 0..n_grid {
        let t_day = grid_t_day[i];
        let t_sec = t_day * SECS_PER_DAY;

        // Thermalization efficiency (Barnes+16 eq. 34)
        let eth_factor = 0.34 * t_day.powf(0.74);
        let eth = 0.36
            * ((-0.56 * t_day).exp()
                + if eth_factor > 1e-10 {
                    (1.0 + eth_factor).ln() / eth_factor
                } else {
                    1.0
                });

        // Heating rates (erg/g/s)
        let xn = xn0 * (-t_sec / 900.0).exp();
        let eps_neutron = 3.2e14 * xn;
        let time_term = (0.5 - ((t_sec - 1.3) / 0.11).atan() / PI).max(1e-30);
        let eps_rp = 2e18 * eth * time_term.powf(1.3);
        let l_heat = m_ej * (eps_neutron + eps_rp) / scale;

        // Effective opacity
        let xr = 1.0 - xn0;
        let xn_decayed = xn0 - xn;
        let kappa_eff = 0.4 * xn_decayed + kappa_r * xr;

        // Diffusion timescale + light-crossing
        let t_diff = 3.0 * kappa_eff * m_ej / (4.0 * PI * C_CGS * v * t_sec) + r / C_CGS;

        // Radiative luminosity
        let l_rad = if e_th > 0.0 && t_diff > 0.0 {
            e_th / t_diff
        } else {
            0.0
        };
        grid_lrad.push(l_rad);

        // PdV work
        let l_pdv = if r > 0.0 { e_th * v / r } else { 0.0 };

        // Euler step
        if i < n_grid - 1 {
            let dt_sec = (grid_t_day[i + 1] - grid_t_day[i]) * SECS_PER_DAY;
            e_th += (l_heat - l_pdv - l_rad) * dt_sec;
            if e_th < 0.0 {
                e_th = e_th.abs();
            }
            e_kin += l_pdv * dt_sec;
            v = (2.0 * e_kin * scale / m_ej).sqrt().min(C_CGS);
            r += v * dt_sec;
        }
    }

    // Normalize by peak
    let l_peak = grid_lrad.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if l_peak <= 0.0 || !l_peak.is_finite() {
        return vec![0.0; obs_times.len()];
    }
    let grid_norm: Vec<f64> = grid_lrad.iter().map(|l| l / l_peak).collect();

    // Interpolate to observation times
    phases
        .iter()
        .map(|&phase| {
            if phase <= 0.0 {
                return 0.0;
            }
            if phase <= grid_t_day[0] {
                return grid_norm[0];
            }
            if phase >= grid_t_day[n_grid - 1] {
                return *grid_norm.last().unwrap();
            }
            let idx = grid_t_day
                .partition_point(|&gt| gt < phase)
                .min(n_grid - 1)
                .max(1);
            let frac = (phase - grid_t_day[idx - 1]) / (grid_t_day[idx] - grid_t_day[idx - 1]);
            grid_norm[idx - 1] + frac * (grid_norm[idx] - grid_norm[idx - 1])
        })
        .collect()
}

/// Finite-difference gradient for MetzgerKN (batch)
/// Returns grads[i][j] = d(pred_i)/d(theta_j)
pub fn metzger_kn_grad_batch(params: &[f64], times: &[f64]) -> Vec<Vec<f64>> {
    let n_times = times.len();
    let n_params = 5;
    let n_phys = 4; // first 4 are physical; sigma_extra has 0 flux gradient

    let base = metzger_kn_eval_batch(params, times);
    let eps = 1e-5;
    let mut grads: Vec<Vec<f64>> = vec![vec![0.0; n_params]; n_times];

    for j in 0..n_phys {
        let mut p_plus = params.to_vec();
        p_plus[j] += eps;
        let f_plus = metzger_kn_eval_batch(&p_plus, times);
        for i in 0..n_times {
            grads[i][j] = (f_plus[i] - base[i]) / eps;
        }
    }
    // grads[i][4] (log_sigma_extra) stays 0.0
    grads
}

// ---------------------------------------------------------------------------
// Batch evaluation dispatch
// ---------------------------------------------------------------------------

/// Evaluate model at all observation times
pub fn eval_model_batch(model: SviModel, params: &[f64], times: &[f64]) -> Vec<f64> {
    if model.is_sequential() {
        metzger_kn_eval_batch(params, times)
    } else {
        times
            .iter()
            .map(|&t| match model {
                SviModel::Bazin => bazin_flux_eval(params, t),
                SviModel::Villar => villar_flux_eval(params, t),
                SviModel::PowerLaw => powerlaw_flux_eval(params, t),
                SviModel::MetzgerKN => unreachable!(),
            })
            .collect()
    }
}

/// Gradient of model predictions w.r.t. params, at all times
/// Returns grads[i][j] = d(pred_i)/d(theta_j)
pub fn eval_model_grad_batch(model: SviModel, params: &[f64], times: &[f64]) -> Vec<Vec<f64>> {
    if model.is_sequential() {
        metzger_kn_grad_batch(params, times)
    } else {
        times
            .iter()
            .map(|&t| match model {
                SviModel::Bazin => bazin_flux_grad(params, t),
                SviModel::Villar => villar_flux_grad(params, t),
                SviModel::PowerLaw => powerlaw_flux_grad(params, t),
                SviModel::MetzgerKN => unreachable!(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bazin_model() {
        let params = vec![0.0, 0.0, 5.0, 0.69, 3.0, -3.0]; // log_a=0 -> a=1
        let times = vec![0.0, 5.0, 10.0, 20.0];
        let fluxes = eval_model_batch(SviModel::Bazin, &params, &times);

        assert!(fluxes[0] < fluxes[1]); // Rising
        assert!(fluxes[2] > fluxes[3]); // Falling
    }

    #[test]
    fn test_metzger_kn_model() {
        let params = vec![-2.0, -1.0, 0.5, 0.0, -3.0];
        let times = vec![0.5, 1.0, 2.0, 5.0];
        let fluxes = eval_model_batch(SviModel::MetzgerKN, &params, &times);

        let max_flux = fluxes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(max_flux > 0.0);
        assert!(max_flux <= 1.0);
    }

    #[test]
    fn test_analytical_gradients() {
        let params = vec![0.0, 0.1, 5.0, 0.69, 3.0, -3.0];
        let t = 7.0;
        let grads = bazin_flux_grad(&params, t);

        // Check that gradients have correct length
        assert_eq!(grads.len(), 6);

        // Check that gradients are finite
        for g in &grads {
            assert!(g.is_finite(), "Gradient should be finite");
        }
    }
}
