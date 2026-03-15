# Light Curve Fitting

The `mm-core::lightcurve_fitting` module extracts merger time (`t₀`) from optical transient light curves, enabling more accurate temporal correlation with gravitational wave events compared to using first detection times.

## Supported Models

ORIGIN fits four analytical models to incoming light curves:

| Model | Use Case | Parameters |
|-------|----------|------------|
| **Bazin** | Type Ia supernovae | Amplitude, t₀, rise time, fall time |
| **Villar** | General transients | Similar to Bazin with different parameterization |
| **PowerLaw** | Fast transients, afterglows | Amplitude, t₀, power-law index |
| **MetzgerKN** | Kilonovae | M_ej, v_ej, κ, t₀ |

## Classification Examples

### Kilonova Candidate (MetzgerKN)

![MetzgerKN fit](../plots/ZTF25aaabnwi_MetzgerKN_MetzgerKN_model_example.png)

The MetzgerKN model captures the rapid rise and red-dominated decline characteristic of r-process powered emission. The extracted `t₀` localizes the merger time to within hours.

![Kilonova classification](../plots/ZTF25aaabnwi_MetzgerKN_Kilonova_candidate.png)

### Supernova (Bazin)

![Bazin fit](../plots/ZTF25aaaalin_Bazin_Bazin_model_example.png)

The Bazin model's weeks-long timescale immediately distinguishes supernovae from kilonovae, providing an effective background rejection filter.

![Supernova classification](../plots/ZTF25aaaalin_Bazin_Supernova-like.png)

### Fast Transient (PowerLaw)

![Power law fit](../plots/ZTF25aaaawig_PowerLaw_PowerLaw_model_example.png)

Power-law decays are characteristic of GRB afterglows and other fast-evolving transients.

![Fast transient classification](../plots/ZTF25aaaawig_PowerLaw_Fast_transient.png)

## API

```rust
use mm_core::lightcurve_fitting::{fit_lightcurve, FitResult, LightCurveModel};

let result: FitResult = fit_lightcurve(&lightcurve, LightCurveModel::MetzgerKN);

if result.is_reliable() {
    println!("t0 = {:.2} +/- {:.2} MJD", result.t0_mjd, result.t0_err_mjd);
    println!("t0 (GPS) = {:.2} s", result.t0_gps);
}
```

The fitter returns `t₀` estimates with uncertainty in both MJD and GPS time. Quality is assessed via `is_reliable()`, which checks that the uncertainty is below a configurable threshold.
