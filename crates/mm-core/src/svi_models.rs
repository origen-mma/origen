//! SVI light curve models
//!
//! Physical and empirical models for fitting transient light curves using
//! Stochastic Variational Inference.

use std::f64::consts::PI;

/// Physical constants
const MSUN_CGS: f64 = 1.98847e33;
const C_CGS: f64 = 2.99792458e10;
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
            SviModel::Bazin => 5,
            SviModel::Villar => 6,
            SviModel::PowerLaw => 4,
            SviModel::MetzgerKN => 4,
        }
    }

    pub fn param_names(&self) -> Vec<&'static str> {
        match self {
            SviModel::Bazin => vec!["A", "B", "t0", "tfall", "trise"],
            SviModel::Villar => vec!["A", "beta", "gamma", "t0", "trise", "tau_exp"],
            SviModel::PowerLaw => vec!["A", "alpha", "beta", "t0"],
            SviModel::MetzgerKN => vec!["log10_mej", "log10_vej", "log10_kappa_r", "t0"],
        }
    }

    /// Get default prior bounds [min, max] for each parameter
    pub fn param_bounds(&self) -> Vec<(f64, f64)> {
        match self {
            SviModel::Bazin => vec![
                (0.1, 10.0),  // A
                (0.01, 5.0),  // B
                (-5.0, 10.0), // t0 (days)
                (1.0, 100.0), // tfall
                (0.1, 10.0),  // trise
            ],
            SviModel::Villar => vec![
                (0.1, 10.0),   // A
                (0.5, 5.0),    // beta
                (0.5, 5.0),    // gamma
                (-5.0, 10.0),  // t0
                (0.1, 10.0),   // trise
                (10.0, 200.0), // tau_exp
            ],
            SviModel::PowerLaw => vec![
                (0.1, 10.0),  // A
                (0.5, 3.0),   // alpha
                (0.5, 3.0),   // beta
                (-5.0, 10.0), // t0
            ],
            SviModel::MetzgerKN => vec![
                (-4.0, -1.0), // log10_mej (0.0001 to 0.1 Msun)
                (-1.5, -0.5), // log10_vej (0.03c to 0.3c)
                (0.0, 2.0),   // log10_kappa_r (1 to 100 cm²/g)
                (-5.0, 10.0), // t0 (days)
            ],
        }
    }
}

/// Bazin model: empirical supernova light curve
pub fn bazin_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0];
    let b = params[1];
    let t0 = params[2];
    let tfall = params[3];
    let trise = params[4];

    let dt = t - t0;
    if dt < -10.0 * trise {
        return 0.0;
    }

    let exp_rise = (-dt / trise).exp();
    let exp_fall = (dt / tfall).exp();

    a / (1.0 + exp_rise) * b / (1.0 + exp_fall)
}

/// Villar model: improved empirical model
pub fn villar_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0];
    let beta = params[1];
    let gamma = params[2];
    let t0 = params[3];
    let trise = params[4];
    let tau_exp = params[5];

    let dt = t - t0;
    if dt < -10.0 * trise {
        return 0.0;
    }

    let sigmoid = 1.0 / (1.0 + (-dt / trise).exp());
    let plateau = 1.0 - beta * dt / (gamma + dt.abs());
    let decay = (-dt / tau_exp).exp();

    (a * sigmoid * plateau * decay).max(0.0)
}

/// Power-law model: simple rise and decay
pub fn powerlaw_flux_eval(params: &[f64], t: f64) -> f64 {
    let a = params[0];
    let alpha = params[1];
    let beta = params[2];
    let t0 = params[3];

    let dt = t - t0;
    if dt <= 0.0 {
        return 0.0;
    }

    a * dt.powf(alpha) * (-dt.powf(beta)).exp()
}

/// Metzger kilonova model (1-zone approximation)
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

    // Initial conditions (scaled by 1e40 to prevent overflow)
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

        // Korobkin+Rosswog r-process
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
            if phase < grid_t_day[0] {
                return 0.0;
            }
            if phase >= grid_t_day[n_grid - 1] {
                return grid_norm[n_grid - 1];
            }

            // Binary search
            let mut lo = 0;
            let mut hi = n_grid - 1;
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if grid_t_day[mid] <= phase {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }

            // Linear interpolation
            let t1 = grid_t_day[lo];
            let t2 = grid_t_day[hi];
            let f1 = grid_norm[lo];
            let f2 = grid_norm[hi];
            let frac = (phase - t1) / (t2 - t1);
            f1 + frac * (f2 - f1)
        })
        .collect()
}

/// Evaluate model at multiple time points
pub fn eval_model_batch(model: SviModel, params: &[f64], times: &[f64]) -> Vec<f64> {
    match model {
        SviModel::MetzgerKN => metzger_kn_eval_batch(params, times),
        _ => times
            .iter()
            .map(|&t| match model {
                SviModel::Bazin => bazin_flux_eval(params, t),
                SviModel::Villar => villar_flux_eval(params, t),
                SviModel::PowerLaw => powerlaw_flux_eval(params, t),
                SviModel::MetzgerKN => unreachable!(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bazin_model() {
        let params = vec![1.0, 1.0, 5.0, 20.0, 2.0];
        let times = vec![0.0, 5.0, 10.0, 20.0];
        let fluxes = eval_model_batch(SviModel::Bazin, &params, &times);

        assert!(fluxes[0] < fluxes[1]); // Rising
        assert!(fluxes[2] > fluxes[3]); // Falling
    }

    #[test]
    fn test_metzger_kn_model() {
        // Typical kilonova parameters
        let params = vec![-2.0, -1.0, 0.5, 0.0]; // 0.01 Msun, 0.1c, kappa=3, t0=0
        let times = vec![0.5, 1.0, 2.0, 5.0];
        let fluxes = eval_model_batch(SviModel::MetzgerKN, &params, &times);

        // Should be normalized, peak should be 1.0
        let max_flux = fluxes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(max_flux > 0.0);
        assert!(max_flux <= 1.0);
    }
}
