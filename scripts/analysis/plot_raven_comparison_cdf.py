#!/usr/bin/env python3
"""Compare empirical FAR calibration with RAVEN analytical predictions using CDFs"""

import numpy as np
import matplotlib.pyplot as plt
from matplotlib import rcParams
from pathlib import Path

# Set nice plot style
rcParams['font.size'] = 10
rcParams['font.family'] = 'sans-serif'
rcParams['axes.labelsize'] = 11
rcParams['axes.titlesize'] = 12
rcParams['legend.fontsize'] = 9

# Determine paths
repo_root = Path(__file__).resolve().parent.parent.parent
output_dir = repo_root / 'assets'
output_dir.mkdir(exist_ok=True)

# RAVEN parameters
TIME_WINDOW = 10.0  # seconds (GRB: -5s to +5s)
GRB_RATE = 325.0 / (365.25 * 24 * 3600)  # 325/yr → Hz
GW_FAR = 1e-7  # Typical BNS FAR in Hz
TEMPORAL_FAR = TIME_WINDOW * GRB_RATE * GW_FAR  # Hz

def calculate_raven_far(spatial_prob):
    """Calculate RAVEN spatiotemporal FAR"""
    if spatial_prob > 0:
        return TEMPORAL_FAR / spatial_prob
    return np.inf

def load_far_data(instrument_name):
    """Load FAR calibration data"""
    filename = instrument_name.lower().replace('-', '_')
    filepath = f'/tmp/far_calibration_{filename}.dat'

    if not Path(filepath).exists():
        print(f"File not found: {filepath}")
        return None, None

    signal_probs = []
    background_probs = []

    with open(filepath, 'r') as f:
        for line in f:
            if line.startswith('#'):
                continue
            parts = line.strip().split()
            if len(parts) == 2:
                typ, prob = parts[0], float(parts[1])
                if typ == 'signal':
                    signal_probs.append(prob)
                elif typ == 'background':
                    background_probs.append(prob)

    return np.array(signal_probs), np.array(background_probs)

# Create figure with 2x3 subplots (2 instruments × 3 plot types)
fig, axes = plt.subplots(2, 3, figsize=(18, 10))
instruments = [
    ('Fermi-GBM', 13.2, 'Fermi-GBM (13.2° error)'),
    ('Swift-BAT', 0.033, 'Swift-BAT (2\' error)')
]

for idx, (instrument, error_deg, label) in enumerate(instruments):
    signal_probs, background_probs = load_far_data(instrument)

    if signal_probs is None:
        print(f"Skipping {instrument} - data not found")
        continue

    # Remove zeros for log scale
    signal_probs = signal_probs[signal_probs > 0]
    background_probs = background_probs[background_probs > 0]

    # Calculate RAVEN FAR for both populations
    raven_far_signal = np.array([calculate_raven_far(p) for p in signal_probs])
    raven_far_signal = raven_far_signal[np.isfinite(raven_far_signal)]
    raven_far_signal_per_yr = raven_far_signal * (365.25 * 24 * 3600)

    raven_far_background = np.array([calculate_raven_far(p) for p in background_probs])
    raven_far_background = raven_far_background[np.isfinite(raven_far_background)]
    raven_far_background_per_yr = raven_far_background * (365.25 * 24 * 3600)

    # ===== Plot 1: Spatial Probability Histogram =====
    ax = axes[idx, 0]

    bins = np.logspace(np.log10(min(signal_probs.min(), background_probs.min())),
                       np.log10(max(signal_probs.max(), background_probs.max())), 40)

    # Use histtype='step' for better visibility when overlapping
    ax.hist(signal_probs, bins=bins, alpha=0.7, label='Signal', color='C0',
            density=True, histtype='step', linewidth=2)
    ax.hist(background_probs, bins=bins, alpha=0.7, label='Background', color='C1',
            density=True, histtype='step', linewidth=2)

    ax.set_xscale('log')
    ax.set_yscale('log')  # Log scale for better visibility
    ax.set_xlabel('Spatial Probability')
    ax.set_ylabel('Probability Density')
    ax.set_title(f'{label}\n(a) Empirical P_spatial Distribution')
    ax.legend(loc='upper right')
    ax.grid(True, alpha=0.3)

    # ===== Plot 2: Spatial Probability CDF =====
    ax = axes[idx, 1]

    # Sort for CDF
    signal_sorted = np.sort(signal_probs)
    bg_sorted = np.sort(background_probs)

    # CDF: fraction of data ≤ x
    signal_cdf = np.arange(1, len(signal_sorted) + 1) / len(signal_sorted)
    bg_cdf = np.arange(1, len(bg_sorted) + 1) / len(bg_sorted)

    ax.plot(signal_sorted, signal_cdf, 'C0-', linewidth=2, label='Signal', alpha=0.8)
    ax.plot(bg_sorted, bg_cdf, 'C1-', linewidth=2, label='Background', alpha=0.8)

    # Add median lines
    signal_median = np.median(signal_probs)
    bg_median = np.median(background_probs)
    ax.axvline(signal_median, color='C0', linestyle='--', alpha=0.5, linewidth=1)
    ax.axvline(bg_median, color='C1', linestyle='--', alpha=0.5, linewidth=1)
    ax.axhline(0.5, color='gray', linestyle=':', alpha=0.5, linewidth=1)

    ax.set_xscale('log')
    ax.set_xlabel('Spatial Probability')
    ax.set_ylabel('Cumulative Probability')
    ax.set_title(f'(b) Empirical P_spatial CDF')
    ax.legend(loc='lower right')
    ax.grid(True, alpha=0.3)

    # Add statistics
    discrimination = signal_median / bg_median
    ax.text(0.05, 0.95, f'Discrimination: {discrimination:.0f}×',
            transform=ax.transAxes, va='top', fontsize=9,
            bbox=dict(boxstyle='round', facecolor='white', alpha=0.8))

    # ===== Plot 3: RAVEN FAR CDF =====
    ax = axes[idx, 2]

    # Sort for CDF
    signal_far_sorted = np.sort(raven_far_signal_per_yr)
    bg_far_sorted = np.sort(raven_far_background_per_yr)

    signal_far_cdf = np.arange(1, len(signal_far_sorted) + 1) / len(signal_far_sorted)
    bg_far_cdf = np.arange(1, len(bg_far_sorted) + 1) / len(bg_far_sorted)

    ax.plot(signal_far_sorted, signal_far_cdf, 'C0-', linewidth=2, label='Signal', alpha=0.8)
    ax.plot(bg_far_sorted, bg_far_cdf, 'C1-', linewidth=2, label='Background', alpha=0.8)

    # Add median lines
    signal_far_median = np.median(raven_far_signal_per_yr)
    bg_far_median = np.median(raven_far_background_per_yr)
    ax.axvline(signal_far_median, color='C0', linestyle='--', alpha=0.5, linewidth=1)
    ax.axvline(bg_far_median, color='C1', linestyle='--', alpha=0.5, linewidth=1)
    ax.axhline(0.5, color='gray', linestyle=':', alpha=0.5, linewidth=1)

    # Add 1/yr threshold
    ax.axvline(1.0, color='red', linestyle='--', linewidth=2, alpha=0.7, label='1/yr threshold')

    ax.set_xscale('log')
    ax.set_xlabel('RAVEN Spatiotemporal FAR (/yr)')
    ax.set_ylabel('Cumulative Probability')
    ax.set_title(f'(c) RAVEN FAR CDF')
    ax.legend(loc='lower right')
    ax.grid(True, alpha=0.3)

    # Add statistics showing equivalence
    raven_discrimination = bg_far_median / signal_far_median

    stats_text = (f'Signal: {signal_far_median:.2e} /yr\n'
                  f'Background: {bg_far_median:.2e} /yr\n'
                  f'RAVEN: {raven_discrimination:.0f}×\n'
                  f'= Empirical: {discrimination:.0f}×')
    ax.text(0.05, 0.95, stats_text, transform=ax.transAxes, va='top',
            fontsize=9, bbox=dict(boxstyle='round', facecolor='white', alpha=0.8))

# Add overall title
fig.suptitle('Empirical ↔ RAVEN FAR Reconciliation: Distribution Comparison\n' +
             f'RAVEN Parameters: Δt={TIME_WINDOW}s, R_GRB={GRB_RATE*365.25*24*3600:.0f}/yr, FAR_GW={GW_FAR:.1e} Hz',
             fontsize=14, fontweight='bold', y=0.98)

plt.tight_layout(rect=[0, 0, 1, 0.96])

output_path = output_dir / 'raven_comparison_cdf.png'
plt.savefig(output_path, dpi=150, bbox_inches='tight')
print(f"\nCDF comparison plot saved to: {output_path}")

# ===== Print Summary =====
print("\n" + "="*80)
print("EMPIRICAL ↔ RAVEN RECONCILIATION SUMMARY")
print("="*80)

for instrument, error_deg, label in instruments:
    signal_probs, background_probs = load_far_data(instrument)
    if signal_probs is None:
        continue

    signal_probs = signal_probs[signal_probs > 0]
    background_probs = background_probs[background_probs > 0]

    # Calculate both metrics
    signal_median = np.median(signal_probs)
    bg_median = np.median(background_probs)
    empirical_disc = signal_median / bg_median

    raven_far_signal = np.array([calculate_raven_far(p) for p in signal_probs])
    raven_far_signal = raven_far_signal[np.isfinite(raven_far_signal)]
    raven_far_signal_per_yr = raven_far_signal * (365.25 * 24 * 3600)

    raven_far_background = np.array([calculate_raven_far(p) for p in background_probs])
    raven_far_background = raven_far_background[np.isfinite(raven_far_background)]
    raven_far_background_per_yr = raven_far_background * (365.25 * 24 * 3600)

    signal_far_median = np.median(raven_far_signal_per_yr)
    bg_far_median = np.median(raven_far_background_per_yr)
    raven_disc = bg_far_median / signal_far_median

    print(f"\n{label}:")
    print(f"  Empirical Discrimination: {empirical_disc:.0f}× (P_signal / P_background)")
    print(f"  RAVEN FAR Discrimination: {raven_disc:.0f}× (FAR_bg / FAR_signal)")
    print(f"  ✅ Ratio: {raven_disc / empirical_disc:.3f} (should be ≈1.000)")
    print(f"\n  Signal FAR (median): {signal_far_median:.2e} /yr")
    print(f"  Background FAR (median): {bg_far_median:.2e} /yr")
    print(f"  Fraction signal < 1/yr: {100 * np.sum(raven_far_signal_per_yr < 1.0) / len(raven_far_signal_per_yr):.1f}%")
    print(f"  Fraction background < 1/yr: {100 * np.sum(raven_far_background_per_yr < 1.0) / len(raven_far_background_per_yr):.3f}%")

print("\n" + "="*80)
print("KEY INSIGHT:")
print("  FAR_bg/FAR_signal = (1/P_bg)/(1/P_signal) = P_signal/P_background")
print("  → RAVEN and empirical discriminations are mathematically equivalent!")
print("="*80)
