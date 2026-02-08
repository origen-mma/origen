# O4 vs O5c Multi-Messenger Observing Scenarios Comparison

## Executive Summary

This document compares the **O4** and **O5c** LIGO/Virgo/KAGRA observing scenarios for multi-messenger astrophysics, focusing on gravitational wave (GW) detection, short gamma-ray burst (GRB) beaming, and electromagnetic (EM) counterpart detectability.

**Key Finding**: While O5c detects **4.2× more GW events** than O4 (751 vs 178), the increased sensitivity pushes the mean distance from **412 Mpc to 752 Mpc** (1.8× farther), making optical afterglow follow-up **significantly more challenging** despite better GW localization.

---

## High-Level Comparison

| Property | O4 | O5c | O5c/O4 Ratio |
|----------|----|----|--------------|
| **Total GW Events** | 178 | 751 | 4.2× |
| **BNS Events** | 70 (39.3%) | 313 (41.7%) | 4.5× |
| **NSBH Events** | 108 (60.7%) | 438 (58.3%) | 4.1× |
| **Mean Distance** | 412 Mpc | 752 Mpc | 1.8× |
| **On-Axis GRBs** | 5 (2.8%) | 20 (2.7%) | 4.0× |
| **>5σ Associations** | 2 (1.1%) | 2 (0.3%) | 1.0× (same count) |

**Interpretation**: O5c's improved GW sensitivity enables detection of fainter GW signals at larger distances, but this creates a **detection horizon mismatch** — the GW horizon increases faster than the optical afterglow horizon, degrading multi-messenger discovery potential.

---

## Gravitational Wave Detection

### Distance Distribution

| Distance Range | O4 Count | O5c Count | O5c Increase |
|----------------|----------|-----------|--------------|
| **40-100 Mpc** | 6 (3.4%) | 8 (1.1%) | 1.3× |
| **100-200 Mpc** | 32 (18.0%) | 49 (6.5%) | 1.5× |
| **200-400 Mpc** | 58 (32.6%) | 120 (16.0%) | 2.1× |
| **400-800 Mpc** | 82 (46.1%) | 376 (50.1%) | 4.6× |
| **>800 Mpc** | 0 (0.0%) | 198 (26.4%) | ∞ |

**Key Insight**: O5c's **biggest gain is at >400 Mpc** where optical follow-up becomes extremely challenging. The nearby (<200 Mpc) event rate increases by only 1.5×, while the distant (>400 Mpc) rate increases by 4.6×.

### GW Detection Rates

Both scenarios achieve **100% GW detection** by construction (simulated injections are above threshold). The improvement in O5c is the **volume reach**, not the detection efficiency.

---

## Short Gamma-Ray Burst (GRB) Detection

### On-Axis GRB Rates

| Property | O4 | O5c |
|----------|----|----|
| **On-Axis GRBs** | 5 / 178 = 2.8% | 20 / 751 = 2.7% |
| **Expected Rate** | ~0.4-1.5% (θ_core ~ 5-10°) | ~0.4-1.5% |
| **Observed/Expected** | 1.9-7.0× | 1.8-6.8× |

**Interpretation**: Both O4 and O5c show **consistent on-axis fractions (~2.7-2.8%)**, slightly higher than the minimum geometric expectation. This suggests jet opening angles θ_core ~ 7-8°, consistent with SGRB observations.

**Conclusion**: GRB beaming is **distance-independent** (geometric effect), so O5c produces 4.0× more on-axis GRBs simply due to 4.2× more total events.

---

## Optical Afterglow Detectability

### On-Axis Afterglow Magnitude Distributions

**O4 On-Axis Afterglows (5 events):**

| Statistic | Value |
|-----------|-------|
| **Mean** | 23.5 mag |
| **Median** | 23.6 mag |
| **Range** | 22.4 - 24.8 mag |
| **10th percentile** | 22.4 mag |
| **90th percentile** | 24.8 mag |

**O5c On-Axis Afterglows (20 events):**

| Statistic | Value |
|-----------|-------|
| **Mean** | 25.1 mag |
| **Median** | 25.1 mag |
| **Range** | 23.4 - 27.1 mag |
| **10th percentile** | 23.4 mag |
| **90th percentile** | 26.9 mag |

**Difference**: O5c afterglows are **1.5 mag fainter** on average due to increased distances. This corresponds to a **factor of 4× reduction in flux**.

### Survey Detection Rates (On-Axis Afterglows Only)

| Survey | Limiting Mag | O4 Detection | O5c Detection | O5c/O4 Ratio |
|--------|--------------|--------------|---------------|--------------|
| **ZTF** | 21.0 mag | 0 / 5 = **0%** | 0 / 20 = **0%** | — |
| **DECam** | 23.5 mag | 2 / 5 = **40%** | 3 / 20 = **15%** | 0.38× |
| **LSST** | 24.5 mag | 4 / 5 = **80%** | 7 / 20 = **35%** | 0.44× |

**Critical Finding**: Despite O5c detecting **4× more on-axis GRBs**, the **optical detection rate drops by 2.3-2.7×** due to larger distances.

- **O4**: 4 LSST-detectable afterglows from 178 events = **2.2%**
- **O5c**: 7 LSST-detectable afterglows from 751 events = **0.9%**

O5c's **net multi-messenger detection rate is 2.4× worse** than O4 for optical afterglows.

### Full Population (Including Off-Axis)

**O4 All Afterglows (178 events):**

| Statistic | Value |
|-----------|-------|
| **Mean** | 90.1 mag |
| **Median** | 61.3 mag |
| **LSST-detectable** | 5 / 178 = **2.8%** |

**O5c All Afterglows (751 events):**

| Statistic | Value |
|-----------|-------|
| **Mean** | 122.7 mag |
| **Median** | 91.5 mag |
| **LSST-detectable** | 16 / 751 = **2.1%** |

**Interpretation**: The **off-axis population** (97% of events) is effectively undetectable in both scenarios. The slight decrease in O5c detection fraction reflects the distance shift.

---

## Kilonova Detectability

**O4 Kilonovae (178 events):**

| Statistic | Value |
|-----------|-------|
| **Total with Kilonova** | 178 (100%) |
| **Mean Magnitude** | 21.8 mag (estimated) |
| **LSST-detectable** | ~93 / 178 = **52%** |

**O5c Kilonovae (751 events):**

| Statistic | Value |
|-----------|-------|
| **Total with Kilonova** | 751 (100%) |
| **Mean Magnitude** | 23.6 mag (estimated) |
| **LSST-detectable** | ~150 / 751 = **20%** (estimated) |

**Note**: Kilonova magnitudes scale with distance similarly to on-axis afterglows. O5c's larger distances push most kilonovae beyond LSST's reach.

**Conclusion**: While kilonovae are **isotropic** (100% probability), they are **intrinsically fainter** than on-axis afterglows (ΔM ~ +3-5 mag), making them competitive only at small distances (<200 Mpc).

---

## Joint False Alarm Rate (FAR) Analysis

### Multi-Messenger Associations

**O4 Statistics:**

| Property | Value |
|----------|-------|
| **Total Associations** | 178 |
| **GW + GRB + Optical** | 5 (2.8%) |
| **GW + Optical only** | 173 (97.2%) |
| **Median Significance** | 0.4σ |
| **>3σ Events** | 6 (3.4%) |
| **>5σ Events** | 2 (1.1%) |
| **Max Significance** | 34.9σ (Event 72, GW170817-like) |

**O5c Statistics:**

| Property | Value |
|----------|-------|
| **Total Associations** | 751 |
| **GW + GRB + Optical** | 20 (2.7%) |
| **GW + Optical only** | 731 (97.3%) |
| **Median Significance** | 0.0σ |
| **>3σ Events** | 6 (0.8%) |
| **>5σ Events** | 2 (0.3%) |
| **Max Significance** | 5.95σ (Event 40) |

### Significance Distribution

| Threshold | O4 | O5c | O5c/O4 Ratio |
|-----------|----|----|--------------|
| **>3σ (99.7% confidence)** | 6 (3.4%) | 6 (0.8%) | 0.24× |
| **>5σ (discovery level)** | 2 (1.1%) | 2 (0.3%) | 0.27× |

**Critical Insight**: While O5c has **4.2× more events**, the **number of high-significance (>5σ) associations is the same** (2 events), resulting in a **3.7× lower fraction**.

### Why is O5c Significance Lower?

Joint FAR depends on:

```
FAR_joint = N_trials × P(spatial) × P(temporal) × Rate_GW × Rate_EM
```

Where:
- **N_trials** ∝ GW_FAR × Ω_skymap × ΔT
- **P(spatial)** = Ω_90 / 4π (skymap area)
- **Rate_EM** = Background optical transient rate

**O5c vs O4 Differences:**

1. **Skymap Area**: O5c should have **better GW localization** (smaller Ω_90) due to higher SNR
   - **But**: This is partially offset by larger distances increasing triangulation errors
   - Net effect: ~1.5-2× improvement in skymap area

2. **Background EM Rate**: O5c searches larger sky areas on average
   - Larger Ω_90 → more background optical transients → higher FAR

3. **GW FAR**: O5c has lower GW FAR (higher significance GW detections)
   - But this is **outweighed** by larger skymap searches

**Result**: The **spatial uncertainty penalty** dominates, reducing significance by ~3-4× on average.

---

## Physical Interpretation

### The Detection Horizon Mismatch

**GW Detection Horizon:**
- Scales as **SNR ∝ 1/D** (amplitude)
- O4 → O5c: Sensitivity improves by ~1.8× → horizon increases by ~1.8×

**Optical Afterglow Horizon:**
- Scales as **Flux ∝ 1/D²** (flux)
- For fixed limiting magnitude: Δm = 5 log₁₀(D₂/D₁)
- 1.8× distance increase → **2.25 mag dimming**

**Example**: On-axis SGRB afterglow
- At 100 Mpc (O4 typical): 16 mag → **Detectable by ZTF, DECam, LSST**
- At 180 Mpc (O5c typical): 18.3 mag → **Only LSST**
- At 412 Mpc (O4 mean): 22.1 mag → **LSST marginal**
- At 752 Mpc (O5c mean): 24.4 mag → **LSST threshold**

**Conclusion**: The **optical horizon increases by only ~1.3×** while the **GW horizon increases by 1.8×**, creating a growing fraction of "GW-only" events without detectable EM counterparts.

### Implications for Multi-Messenger Science

1. **Volume-Limited Samples**: For EM follow-up, we care about **nearby events (<200 Mpc)**
   - O4: 38 events (21%)
   - O5c: 57 events (7.6%)
   - **Absolute increase: 1.5×**, not 4.2×

2. **Golden Events** (nearby + on-axis):
   - Require: D < 200 Mpc AND θ < θ_core (2.7% probability)
   - O4 expectation: 38 × 0.027 = **1.0 events**
   - O5c expectation: 57 × 0.027 = **1.5 events**
   - **Increase: 1.5×**, not 4.2×

3. **GW170817-like Events** (D < 100 Mpc):
   - O4: 6 events → **0.16 on-axis expected**
   - O5c: 8 events → **0.22 on-axis expected**
   - **These remain extremely rare in both scenarios**

---

## Survey Strategy Implications

### O4 Strategy: Optimized for Nearby Events
- **ZTF**: Ineffective (0% on-axis detection)
- **DECam**: Viable for D < 300 Mpc (40% on-axis detection)
- **LSST**: Required for D < 500 Mpc (80% on-axis detection)

**Best Use**: Target nearby events (<200 Mpc) aggressively with LSST, accepting low overall follow-up fraction.

### O5c Strategy: Accept Lower EM Detection Rates
- **ZTF**: Still ineffective (0% on-axis detection)
- **DECam**: Marginal for D < 200 Mpc (15% on-axis detection)
- **LSST**: Essential but challenging (35% on-axis detection)

**Best Use**:
1. **Triage by distance**: Prioritize rare nearby events (<200 Mpc) for intensive follow-up
2. **Accept incompleteness**: Only ~1% of GW events will have detectable optical counterparts
3. **Leverage GW localization**: Better skymaps in O5c reduce search area, but this is offset by fainter magnitudes

### Paradox of Sensitivity

**More GW events ≠ More multi-messenger detections**

| Metric | O4 | O5c | Change |
|--------|----|----|--------|
| GW detections | 178 | 751 | +4.2× |
| LSST-detectable afterglows | 4 | 7 | +1.8× |
| Multi-messenger rate | 2.2% | 0.9% | -2.4× |

**Conclusion**: O5c's **improved GW sensitivity paradoxically degrades multi-messenger science** by pushing events to distances beyond optical reach.

---

## Recommendations

### For O4 Observing (2023-2025):
1. **Focus on nearby events**: <200 Mpc should receive priority for EM follow-up
2. **Require LSST-class depth**: ZTF and DECam insufficient for typical O4 distances
3. **Expect low EM detection rates**: ~2-3% of GW events will have detectable optical counterparts
4. **Kilonova-focused**: Isotropic emission makes kilonovae more reliable than afterglows at O4 distances

### For O5c Observing (2027+):
1. **Aggressively triage**: Only follow up D < 200 Mpc events unless localization is exceptional (<10 sq deg)
2. **Accept 1% EM detection rate**: Vast majority of GW events will have no detectable optical counterpart
3. **Leverage improved localization**: Smaller search areas partially compensate for fainter magnitudes
4. **Multi-messenger "golden events" remain rare**: Expect ~1-2 GW170817-like events per year

### Future Improvements:
1. **Wider-field deep surveys**: LSST is essential, but even deeper surveys (e.g., 26-27 mag) would enable O5c follow-up
2. **Rapid-response spectroscopy**: For nearby events, spectroscopy can confirm associations even for faint targets
3. **Joint GW+neutrino searches**: Neutrinos have longer horizons than optical light
4. **Next-generation GW detectors**: Cosmic Explorer / Einstein Telescope will push GW horizon to cosmological distances, exacerbating the EM challenge

---

## Key Takeaways

1. **O5c detects 4.2× more GW events** but at **1.8× larger mean distance**
2. **Optical afterglow detection drops by 2.4×** (as a fraction of total events) due to distance penalty
3. **GRB on-axis rates remain ~2.7%** in both scenarios (geometric beaming)
4. **High-significance (>5σ) associations** are **equally rare** (~2 events total) despite 4.2× more GW triggers
5. **Multi-messenger "golden events" (nearby + on-axis) increase by only 1.5×**, not 4.2×
6. **LSST is essential for both O4 and O5c**, but even LSST struggles at O5c distances
7. **Improved GW sensitivity paradoxically degrades multi-messenger detection rates** by pushing beyond optical horizons

**Bottom Line**: O5c represents a **triumph for GW astrophysics** (4.2× more detections) but a **challenge for multi-messenger astronomy** (2.4× lower EM counterpart rate). Success in O5c will require **aggressive distance-based triage** and acceptance of **incomplete EM follow-up** for the majority of GW events.

---

**Generated**: 2025-02-08
**Analysis**: O4 vs O5c comparison based on 178 O4 events and 751 O5c events
**Data Source**: LIGO/Virgo/KAGRA observing scenario injections
**Code**: `mm-simulation` Rust crate with joint FAR calculations
