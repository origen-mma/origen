# O4 Joint FAR Analysis

Statistical significance assessment of 178 O4 multi-messenger associations using joint FAR calculations.

## Joint FAR Formula

```
FAR_joint = N_trials × P(spatial) × P(temporal) × R_GW × R_EM
```

### Configuration

| Parameter | Value | Justification |
|-----------|-------|---------------|
| GW observing time | 1 year | O4 run duration |
| GRB rate (all sky) | 300/year | Historical SGRB rate |
| Optical transient rate | 500/sq deg/year | ZTF-like survey |
| Optical search window | 14 days | Typical follow-up duration |
| GRB time window | +/-5 seconds | Prompt GRB emission |

## Results Summary

### Association Types

| Association Type | Count | Fraction |
|------------------|-------|----------|
| GW + GRB + Optical | 5 | 2.8% |
| GW + Optical only | 173 | 97.2% |
| GW + GRB only | 0 | 0.0% |

### Joint FAR Distribution

| Statistic | Value (per year) |
|-----------|------------------|
| Mean | 4.33 × 10^4 |
| Median | 2.75 × 10^4 |
| Minimum | 1.64 × 10^-3 |
| Maximum | 5.01 × 10^5 |

### Significance Distribution

| Threshold | Count | Fraction |
|-----------|-------|----------|
| > 3σ | 3 | 1.7% |
| > 5σ | 2 | 1.1% |

!!! warning "Key Finding"
    Only **2 events (1.1%)** exceed the 5σ discovery threshold.

## Highly Significant Events

### Event 72 -- 34.9σ (Exceptional)

| Property | Value |
|----------|-------|
| Distance | 57 Mpc |
| GW SNR | 15.5 |
| Skymap area | 32 sq deg |
| Has GRB | Yes |
| Optical mag | 18.0 |
| Joint FAR | 1.64 × 10^-3 / year |
| P_astro | 99.8% |

This is analogous to GW170817 -- nearby event with excellent localization, on-axis GRB, and bright optical counterpart.

### Event 95 -- 12.4σ

| Property | Value |
|----------|-------|
| Distance | 76 Mpc |
| Skymap area | 58 sq deg |
| Optical mag | 19.5 |
| P_astro | 93.8% |

## Comparison with GW170817

| Property | GW170817 | Event 72 (O4 analog) |
|----------|----------|----------------------|
| Distance | 40 Mpc | 57 Mpc |
| GW localization | 28 sq deg | 32 sq deg |
| GRB | Yes (off-axis) | Yes (on-axis) |
| Optical magnitude | 17--18 mag | 18.0 mag |
| Significance | >5σ | 34.9σ |
| P_astro | >99% | 99.8% |

## Why Most Associations Are Low Significance

1. **Large distances (mean 412 Mpc)**: Poor GW localization -> high background EM transient rate
2. **Off-axis afterglows (97%)**: Faint optical magnitudes (>25 mag) blend with background
3. **Optical transient background**: ~500 transients/sq deg/year in ZTF, many false associations

## Implications for O5

| Distance | O4 Fraction | O5 Expected |
|----------|-------------|-------------|
| <100 Mpc | 3.4% | ~1% |
| <200 Mpc | 21.3% | ~8% |

**O5 Prediction**: ~1--2 significant (>5σ) associations per year, similar to O4 absolute rate despite 10x more events. Most O5 events will be distant (>300 Mpc) with poor localization.

## Strategies to Improve Significance

1. **LSST deep surveys**: Detects fainter afterglows (but also more background)
2. **Faster sky localization**: Reduces temporal search window, improves FAR by 10--100x
3. **Multi-messenger cuts**: Require GW + GRB + optical (reduces background 1000x, loses 97% of events)
4. **Kilonova-specific signatures**: Color evolution + fast fading reduces background by ~10x
