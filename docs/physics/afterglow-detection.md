# Afterglow Detection

The afterglow simulation uses AB magnitudes for detection thresholds, making them directly comparable to real survey sensitivities.

## Survey Sensitivities

```rust
use mm_simulation::grb_simulation::AfterglowConfig;

// Pre-configured survey models
let ztf = AfterglowConfig::ztf();         // 21.0 mag limiting
let lsst = AfterglowConfig::lsst();       // 24.5 mag limiting
let decam = AfterglowConfig::decam();      // 23.5 mag limiting
```

| Survey | Limiting Magnitude | Typical Afterglow Range |
|--------|-------------------|------------------------|
| ZTF | 21.0 mag | Detectable to ~50 Mpc |
| DECam | 23.5 mag | Detectable to ~300 Mpc |
| LSST | 24.5 mag | Detectable to ~500 Mpc |

## Detection Physics

On-axis GRB afterglows at O4 distances (100--500 Mpc) typically peak at 22--27 mag in R-band, making them:

- **Undetectable by ZTF** at most distances (limit 21 mag)
- **Marginally detectable by DECam** for nearby events
- **Detectable by LSST** for a significant fraction

!!! warning "ZTF Afterglow Rate"
    In O4 simulations, **0%** of afterglows were detectable by ZTF. Even on-axis afterglows at the mean BNS distance (275 Mpc) reach only ~22.4 mag -- below ZTF's limit.

## Magnitude Calculation

Peak apparent magnitude in R-band:

```
m_R = M_R + 5 × log₁₀(d / 10 pc) + A_R
```

where `M_R` is the absolute magnitude (from afterglow physics), `d` is the luminosity distance, and `A_R` is Galactic extinction.
