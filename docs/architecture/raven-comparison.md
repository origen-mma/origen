# RAVEN Methodology Comparison

Analysis of LIGO RAVEN's implementation and comparison with ORIGIN's approach to multi-messenger correlation.

## Spatial Correlation (Skymap Overlap Integral)

**RAVEN** (`search.py:263-461`):

```python
def skymap_overlap_integral(gw_skymap, ext_skymap=None, ra=None, dec=None, ...):
    # Point-source case:
    if ra is not None and dec is not None:
        c = SkyCoord(ra=ra*u.degree, dec=dec*u.degree)
        catalog = SkyCoord(ra=ra_gw, dec=dec_gw)
        ind, d2d, d3d = c.match_to_catalog_sky(catalog)
        return (gw_skymap_prob[ind] / u.sr / sky_prior / se_norm).to(1).value
```

**ORIGIN** (`mm-correlator/src/spatial.rs`):

```rust
pub fn calculate_spatial_probability_from_skymap(
    position: &SkyPosition,
    skymap: &ParsedSkymap,
) -> f64 {
    skymap.probability_at_position(position.ra, position.dec)
}
```

**Verdict: Equivalent approaches.** Both query the skymap at the transient position. RAVEN normalizes by sky prior; ORIGIN handles this in skymap parsing.

---

## Temporal + Spatial FAR Combination

**RAVEN Formula** (`search.py:578-620`):

```python
temporal_far = (th - tl) * ext_rate * se_far
spatiotemporal_far = temporal_far / skymap_overlap
```

**ORIGIN**: Uses empirical distributions from Monte Carlo simulation rather than the analytical formula.

| Approach | Pros | Cons |
|----------|------|------|
| **RAVEN (analytical)** | Fast, no pre-computation | Assumes independence of spatial/temporal |
| **ORIGIN (empirical)** | Captures correlations, realistic error models | Requires calibration runs |

---

## Background Event Rates

**RAVEN** (`search.py:536-551`):

| Source | Rate |
|--------|------|
| GRB (Fermi + Swift + SVOM) | 325/yr |
| Sub-threshold GRBs | 65/yr |
| IceCube neutrinos | 13.91/yr |

**ORIGIN**: Uses Fermi-GBM + Swift-BAT simulations from O4 catalog; supernova rate at ~10,000x kilonova rate (literature-based). Consistent at order-of-magnitude level.

---

## Temporal Coincidence Windows

| | RAVEN | ORIGIN |
|---|---|---|
| **GRB** | -60s to +600s (symmetric) | -5s to +5s (prompt) |
| **Optical** | -- | -1s to +86400s (1 day) |
| **Philosophy** | Symmetric around GW time | Asymmetric, physics-motivated |

ORIGIN's asymmetric windows reflect the astrophysics: GRBs are prompt (seconds), kilonovae take hours to become detectable and peak at ~1--2 days.

---

## FAR Validation

Both use random backgrounds to establish null distributions:

- **RAVEN**: Random times + skymaps (joint null)
- **ORIGIN**: Random positions + times (separate validation for spatial/temporal)

ORIGIN uses real O4 skymaps for realistic validation:

```rust
// SIGNAL: Real GRB at true position (within error radius)
signal_prob = integrate_skymap_over_circle(grb_position, error_radius, skymap);

// BACKGROUND: Random GRBs at random sky positions
for _ in 0..1000 {
    bg_position = random_sky_position();
    bg_prob = integrate_skymap_over_circle(bg_position, error_radius, skymap);
}
```

---

## Combined Discrimination

| Component | ORIGIN Result |
|-----------|--------------|
| Spatial discrimination | 290,000x |
| Temporal discrimination | 82x |
| **Combined** | **~23.8 million** |

This is consistent with RAVEN's methodology but uses empirical distributions.

---

## Summary

| Aspect | RAVEN | ORIGIN | Match? |
|--------|-------|--------|--------|
| Skymap integration | HEALPix query | HEALPix query | Yes |
| FAR formula | Analytical | Empirical (Monte Carlo) | Complementary |
| Temporal windows | Symmetric | Physics-motivated asymmetric | ORIGIN more precise |
| Validation | Random backgrounds | Real O4 skymaps | ORIGIN more realistic |
| Background rates | Literature-based | Simulation-based | Consistent |

## References

- RAVEN Paper: [arXiv:1803.04089](https://arxiv.org/abs/1803.04089)
- RAVEN Code: [git.ligo.org/lscsoft/raven](https://git.ligo.org/lscsoft/raven)
- RAVEN Docs: [ligo-raven.readthedocs.io](https://ligo-raven.readthedocs.io/)
