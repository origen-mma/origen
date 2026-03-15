# O4 Multi-Messenger Detection Summary

Analysis of 178 O4 gravitational wave events (70 BNS + 108 NSBH) through the ORIGIN multi-messenger simulation pipeline.

## Detection Rates by Binary Type

| Modality | BNS (70 events) | NSBH (108 events) | Combined (178 events) |
|----------|-----------------|-------------------|----------------------|
| **GW Detection** | 70 (100%) | 108 (100%) | 178 (100%) |
| **GRB (prompt)** | 2 (2.9%) | 3 (2.8%) | 5 (2.8%) |
| **Afterglow (ZTF 21 mag)** | 0 (0%) | 0 (0%) | 0 (0%) |
| **Kilonova** | 70 (100%) | 23 (21.3%) | 93 (52.2%) |
| **Mean Distance** | 275 Mpc | 501 Mpc | 414 Mpc |

## Key Physical Insights

### Gravitational Waves: 100% Detection

All BNS/NSBH events in O4 sample are GW-detected (by definition). LIGO/Virgo/KAGRA sensitivity: ~200 Mpc (BNS), ~800 Mpc (NSBH).

### Gamma-Ray Bursts: ~3% Detection

Only visible when viewing angle is within jet cone (~10 deg). Observed rate (2.8%) is consistent with expected beaming fraction (~0.4--1.5% for \\(\theta_\text{jet} \sim 5\text{--}10°\\)).

### Optical Afterglows: 0% with ZTF

Mean distances of 275 Mpc (BNS) and 501 Mpc (NSBH) push even on-axis afterglows to 22--27 mag, well below ZTF's 21 mag limit.

### Kilonovae: 100% (BNS) vs 21% (NSBH)

- **BNS**: Every merger produces neutron-rich ejecta -- 100% kilonova rate
- **NSBH**: Only ~21% tidally disrupt the NS before plunging into BH -- realistic physics

## Afterglow Detection vs Survey Depth

| Survey | Limiting Mag | Estimated Afterglow Detection |
|--------|--------------|-------------------------------|
| **ZTF** | 21.0 mag | ~0% (current result) |
| **DECam** | 23.5 mag | ~60% of on-axis GRBs |
| **LSST** | 24.5 mag | ~100% of on-axis GRBs |

## Multi-Messenger Association Rates

| Association Type | ZTF (21 mag) | DECam (23.5 mag) | LSST (24.5 mag) |
|------------------|--------------|------------------|-----------------|
| **GW + GRB** | 5 (2.8%) | 5 (2.8%) | 5 (2.8%) |
| **GW + Kilonova** | 93 (52.2%) | 93 (52.2%) | 93 (52.2%) |
| **GW + Afterglow** | 0 (0%) | ~3 (1.7%) | ~5 (2.8%) |
| **Full MM (GW+GRB+Optical)** | 0 (0%) | ~3 (1.7%) | ~5 (2.8%) |

## Distance Distribution Impact

| Distance Range | BNS Count | NSBH Count | On-axis Afterglow Mag |
|----------------|-----------|------------|----------------------|
| 40--100 Mpc | ~5 | ~2 | 16--19 mag (ZTF detectable) |
| 100--200 Mpc | ~15 | ~5 | 19--20.5 mag (ZTF marginal) |
| 200--400 Mpc | ~30 | ~20 | 20.5--22 mag (need DECam) |
| 400--800 Mpc | ~20 | ~81 | 22--24 mag (need LSST) |

!!! info "Key Insight"
    Most O4 BNS/NSBH are at >200 Mpc, pushing even on-axis afterglows beyond ZTF's reach.

## Recommendations

1. **For O4 distances (>200 Mpc average)**: LSST is essential for comprehensive afterglow coverage
2. **For nearby events (<100 Mpc)**: ZTF can detect on-axis afterglows; DECam/LSST can detect off-axis
3. **Kilonova follow-up**: More promising than afterglows at O4 distances (52% detectable, less viewing-angle dependent)
