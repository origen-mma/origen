# GW Population & Kilonova Model

This page documents the physical models used by the FAR tuning campaign to generate realistic GW-optical multi-messenger events.

## GW Population Model

BNS (and eventually NSBH) mergers are drawn from astrophysically-motivated distributions.

### Distance Distribution

Mergers are distributed uniformly in comoving volume out to the detector horizon:

```
P(d) ∝ d²  =>  F(d) = (d / d_max)³  =>  d = d_max × u^(1/3)
```

where `u ~ Uniform(0,1)`. This produces the characteristic concentration toward the horizon: the median distance is at `d_max × 0.5^(1/3) ≈ 0.794 × d_max`.

For O4 (`d_max = 190` Mpc), the median injection distance is ~151 Mpc.

### NS Mass Distribution

Component masses are drawn from a truncated Gaussian matching the Galactic double neutron star population:

- Mean: 1.35 M☉
- Std dev: 0.15 M☉
- Range: [1.1, 2.0] M☉

The heavier component is always `m₁`. The mass ratio `q = m₂/m₁` affects ejecta properties.

### Inclination

The viewing angle is drawn uniform in `cos(i)`, giving isotropic orientation:

```
cos(i) ~ Uniform(-1, 1)  =>  i = arccos(cos(i))
```

Face-on events (`i ≈ 0`) have stronger GW signals and brighter kilonovae; edge-on events (`i ≈ π/2`) are dimmer in both channels.

### Sky Localization

A `MockSkymap` is generated for each event with area scaling as distance squared:

```
r_90 = 3° × (d / 40 Mpc)    (capped at 30°)
```

This reproduces the general trend that distant events have larger error regions.

## Ejecta Model

BNS ejecta properties are computed using the Kruger & Foucart (2020) fitting formulae:

### Dynamical Ejecta

The dynamical ejecta mass depends on the mass ratio, compactness, and tidal deformability:

```
M_ej,dyn = f(q, C₁, C₂, Λ̃)
```

where `C = GM/(Rc²)` is the compactness and `Λ̃` is the combined tidal deformability.

### Wind/Disk Ejecta

A fraction of the accretion disk is unbound as wind ejecta:

```
M_disk = f(q, C₁, C₂)       M_ej,wind ≈ 0.3 × M_disk
```

### Ejecta Velocity

```
v_ej,dyn ≈ 0.2-0.35c     (depends on mass ratio)
```

```
v_ej,wind ≈ 0.05-0.15c    (slower, quasi-spherical)
```

## Kilonova Light Curve

### Peak Magnitude

An empirical scaling relation calibrated to AT2017gfo:

```
M_peak = -15.8 - 2.5 × log₁₀(M_ej / 0.05 M☉) - 1.25 × log₁₀(v_ej / 0.3c)
```

At GW170817 values (`M_ej = 0.05 M☉`, `v_ej = 0.3c`), this gives `M ≈ -15.8` mag, matching the observed i-band peak.

### Light Curve Shape: MetzgerKN Model

The time-dependent luminosity uses the Metzger (2017) semi-analytic model:

1. **r-process heating**: Radioactive decay of freshly synthesized r-process nuclei provides the energy source, with heating rate `ε_rp ∝ t^(-1.3)`
2. **Free neutron decay**: An additional early heating source from free neutrons that haven't been captured, with `τ ≈ 900` s
3. **Thermalization**: The Barnes et al. (2016) efficiency function determines what fraction of radioactive energy thermalizes
4. **Diffusion**: Photons diffuse out on a timescale set by the opacity, ejecta mass, and velocity:

```
t_diff = (3κ M_ej) / (4π c v t) + R / c
```

5. **Opacity**: `κ = 10` cm²/g (lanthanide-rich, appropriate for the combined "red" KN component)

The model is evaluated on a log-spaced time grid and the bolometric luminosity is recorded at survey observation epochs.

### Survey Sampling

The continuous light curve is sampled at the survey's observation cadence:

1. **Phase offset**: Random offset within one cadence interval (simulates when the survey first observes the field)
2. **Apparent magnitude**: `M_peak + μ_d + Δm(t)`, where `μ_d = 5 × log₁₀(d / 10 pc)` is the distance modulus
3. **Detection threshold**: SNR >= 3 required, with `SNR = 5 × 10^((m_lim - m) / 2.5)`
4. **Photometric noise**: Gaussian in magnitude space, combining systematic floor and photon noise:

```
σ_mag = sqrt(σ_floor² + (1.0857 / SNR)²)
```

5. **Band cycling**: Observations cycle through available bands (g, r for ZTF; g, r, i, z for LSST)

### Detectability Criterion

A KN is considered "detectable" if the survey obtains >= 2 photometric points above the detection threshold. This matches the correlator's requirement for light curve analysis.
