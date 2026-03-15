# FAR Tuning Campaign

The `far-tuning-campaign` binary runs a Monte Carlo injection campaign to calibrate the joint false alarm rate (FAR) threshold of the GW-optical correlator. It measures detection efficiency versus false positive rate across a range of FAR thresholds, producing ROC curves for optimal threshold selection.

## Motivation

The RAVEN correlator assigns a joint FAR to each GW-optical candidate association. Choosing the right FAR threshold involves a trade-off:

- **Too loose** (high FAR threshold): many false positives from background supernovae
- **Too strict** (low FAR threshold): real kilonovae are missed, especially at large distances

The campaign quantifies this trade-off by injecting realistic kilonova signals into a realistic background of optical transients and measuring what the correlator recovers.

## Usage

```bash
# Quick campaign (100 injections, ZTF survey)
far-tuning-campaign -n 100 --survey ztf -o results.json

# Full campaign (1000 injections, longer window)
far-tuning-campaign -n 1000 --survey ztf --window-days 14 -o results.json

# LSST-era campaign with O5 horizon
far-tuning-campaign -n 1000 --survey lsst --d-horizon 330 -o results_lsst.json

# Verbose mode with custom seed
far-tuning-campaign -n 500 -v --seed 123 -o results.json

# Plot the results
python scripts/analysis/plot_far_campaign.py results.json
```

## Pipeline

For each of the N injections:

```text
1. Draw GW event        2. Compute ejecta       3. Generate KN light curve
+------------------+   +------------------+    +----------------------+
| distance ~ d^3   |   | Kruger & Foucart |    | MetzgerKN forward    |
| cos(i) uniform   |-->| fitting formulae |--->| model + survey noise |
| NS masses Gauss  |   | -> M_ej, v_ej    |    | + cadence sampling   |
+------------------+   +------------------+    +----------+-----------+
                                                          |
4. Generate background  5. Feed correlator      6. Record outcome
+------------------+   +------------------+    +----------------------+
| SNe Ia + shock   |   | Fresh correlator |    | recovered? joint_far?|
| cooling at real  |-->| per injection:   |--->| false positives?     |
| ZTF/LSST rates   |   | GW -> optical LCs|    | -> ROC curve         |
+------------------+   +------------------+    +----------------------+
```

## Physical Models

### GW Population

BNS mergers drawn from astrophysically-motivated distributions:

| Parameter | Distribution | O4 Default | O5 Default |
|---|---|---|---|
| Distance | Uniform in comoving volume (\\(\propto d^3\\)) | 0--190 Mpc | 0--330 Mpc |
| Inclination | Uniform in \\(\cos i\\) | \\([0, \pi]\\) | \\([0, \pi]\\) |
| NS mass | Truncated Gaussian | \\(\mu\\)=1.35, \\(\sigma\\)=0.15 \\(M_\odot\\) | same |
| Sky position | Isotropic | -- | -- |
| GW FAR | Log-uniform | \\(10^{-4}\\)--\\(10^{-1}\\) yr\\(^{-1}\\) | same |

### Kilonova Light Curve

1. **Ejecta properties** from Kruger & Foucart (2020) fitting formulae: BNS masses to dynamical + wind ejecta mass and velocity
2. **Peak absolute magnitude** from empirical scaling calibrated to AT2017gfo:

\[
M_\text{peak} = -15.8 - 2.5 \log_{10}\!\left(\frac{M_\text{ej}}{0.05\,M_\odot}\right) - 1.25 \log_{10}\!\left(\frac{v_\text{ej}}{0.3c}\right)
\]

3. **Light curve shape** from the MetzgerKN semi-analytic model (Metzger 2017), including r-process heating, thermalization, and diffusion
4. **Survey sampling** at the appropriate cadence (ZTF: 2-day, LSST: 3-day) with photometric noise scaling as \\(\text{SNR} = 5 \times 10^{(m_\text{lim} - m)/2.5}\\)

### Survey Models

| Parameter | ZTF | LSST |
|---|---|---|
| Cadence | 2 days | 3 days |
| Bands | g, r | g, r, i, z |
| Limiting magnitude | 20.5 | 24.5 |
| Noise floor | 0.02 mag | 0.005 mag |
| Sky fraction | 47% | 45% |

### Background Transients

Generated at observed rates using the `BackgroundOpticalConfig`:

- **ZTF**: ~1000 transients/night (to 21 mag)
- **LSST**: ~10,000 transients/night (to 24.5 mag)

Mix of Type Ia supernovae (~weeks timescale) and shock-cooling transients (~hours timescale).

## Output

### JSON Structure

```json
{
  "n_injections": 1000,
  "n_detectable": 320,
  "n_recovered": 45,
  "n_background_tested": 1500,
  "n_background_false": 12,
  "median_injection_distance": 150.3,
  "injection_outcomes": [
    {
      "injection_id": 0,
      "distance_mpc": 142.5,
      "mej_total": 0.023,
      "apparent_peak_mag": 20.1,
      "detectable": true,
      "recovered": true,
      "joint_far": 0.0012
    }
  ],
  "roc_curve": [
    {
      "far_threshold": 0.001,
      "efficiency": 0.65,
      "false_positive_rate": 0.02
    }
  ],
  "efficiency_vs_distance": [
    [50.0, 0.95],
    [100.0, 0.72],
    [200.0, 0.15]
  ]
}
```

### Visualization

Generate analysis plots with:

```bash
python scripts/analysis/plot_far_campaign.py results.json
```

This produces a 6-panel figure:

1. **ROC curve** -- detection efficiency vs false positive rate, annotated with FAR thresholds
2. **Efficiency vs distance** -- bar chart of recovery fraction in distance bins
3. **Distance distribution** -- histogram of injected, detectable, and recovered events
4. **Peak magnitude distribution** -- histogram with survey limiting magnitude
5. **Joint FAR distribution** -- log-scale histogram of signal vs background FARs
6. **Ejecta mass vs distance** -- scatter plot colored by detection outcome

## Interpreting Results

- **Detectable fraction** (~30% at ZTF O4 horizon): fraction of injected KNe bright enough for >=2 survey detections. Dominated by the (distance/limiting mag) cut.
- **Recovery fraction** (of detectable): fraction the correlator matches to the GW trigger. Depends on the spatial threshold, temporal window, and the correlator's light curve filtering.
- **Optimal FAR threshold**: the "knee" of the ROC curve where efficiency saturates before false positives rise steeply.
