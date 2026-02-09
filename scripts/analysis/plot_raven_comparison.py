#!/usr/bin/env python3
"""Compare empirical FAR calibration with RAVEN analytical predictions"""

import numpy as np
import matplotlib.pyplot as plt
from matplotlib import rcParams
from pathlib import Path

# Set nice plot style
rcParams['font.size'] = 11
rcParams['font.family'] = 'sans-serif'
rcParams['axes.labelsize'] = 12
rcParams['axes.titlesize'] = 14
rcParams['legend.fontsize'] = 10

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

# Create figure with 2x2 subplots
fig, axes = plt.subplots(2, 2, figsize=(14, 10))
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

    # Calculate RAVEN FAR for each spatial probability
    raven_far_signal = np.array([calculate_raven_far(p) for p in signal_probs])
    raven_far_signal = raven_far_signal[np.isfinite(raven_far_signal)]

    # Convert to /yr for readability
    raven_far_signal_per_yr = raven_far_signal * (365.25 * 24 * 3600)

    # ===== Plot 1: Spatial Probability Distributions (Empirical) =====
    ax = axes[idx, 0]

    # Histograms
    bins = np.logspace(np.log10(min(signal_probs.min(), background_probs.min())),
                       np.log10(max(signal_probs.max(), background_probs.max())), 50)
    ax.hist(signal_probs, bins=bins, alpha=0.6, label='Signal', color='C0', density=True)
    ax.hist(background_probs, bins=bins, alpha=0.6, label='Background', color='C1', density=True)

    ax.set_xscale('log')
    ax.set_xlabel('Spatial Probability (P_spatial)')
    ax.set_ylabel('Probability Density')
    ax.set_title(f'{label}\nEmpirical Distributions')
    ax.legend()
    ax.grid(True, alpha=0.3)

    # Add statistics
    signal_median = np.median(signal_probs)
    bg_median = np.median(background_probs)
    discrimination = signal_median / bg_median
    ax.text(0.05, 0.95, f'Discrimination: {discrimination:.0f}×\n(Signal/Bg median)',
            transform=ax.transAxes, va='top', fontsize=9,
            bbox=dict(boxstyle='round', facecolor='white', alpha=0.8))

    # ===== Plot 2: RAVEN Analytical FAR Distribution =====
    ax = axes[idx, 1]

    # Histogram of RAVEN FAR values
    bins_far = np.logspace(-3, 4, 50)  # /yr
    ax.hist(raven_far_signal_per_yr, bins=bins_far, alpha=0.7, color='C2', density=True)

    ax.set_xscale('log')
    ax.set_xlabel('RAVEN Spatiotemporal FAR (/yr)')
    ax.set_ylabel('Probability Density')
    ax.set_title(f'{label}\nRAVEN Analytical FAR')
    ax.grid(True, alpha=0.3)

    # Add threshold line at 1/yr
    ax.axvline(1.0, color='red', linestyle='--', linewidth=2, alpha=0.7, label='1/yr threshold')
    ax.legend()

    # Add statistics
    far_median = np.median(raven_far_signal_per_yr)
    far_95th = np.percentile(raven_far_signal_per_yr, 95)
    n_below_threshold = np.sum(raven_far_signal_per_yr < 1.0)
    frac_below = 100 * n_below_threshold / len(raven_far_signal_per_yr)

    stats_text = f'Median: {far_median:.2e} /yr\n95th %: {far_95th:.2e} /yr\nFAR < 1/yr: {frac_below:.1f}%'
    ax.text(0.95, 0.95, stats_text, transform=ax.transAxes, va='top', ha='right',
            fontsize=9, bbox=dict(boxstyle='round', facecolor='white', alpha=0.8))

# Add overall title
fig.suptitle('Empirical vs RAVEN Analytical FAR Comparison\n' +
             f'RAVEN Parameters: Δt={TIME_WINDOW}s, R_GRB={GRB_RATE*365.25*24*3600:.0f}/yr, FAR_GW={GW_FAR:.1e} Hz',
             fontsize=14, fontweight='bold', y=0.98)

plt.tight_layout(rect=[0, 0, 1, 0.96])

output_path = output_dir / 'raven_comparison.png'
plt.savefig(output_path, dpi=150, bbox_inches='tight')
print(f"\nPlot saved to: {output_path}")

# ===== Print Summary Table =====
print("\n" + "="*70)
print("EMPIRICAL vs RAVEN ANALYTICAL COMPARISON")
print("="*70)

for instrument, error_deg, label in instruments:
    signal_probs, background_probs = load_far_data(instrument)
    if signal_probs is None:
        continue

    signal_probs = signal_probs[signal_probs > 0]
    background_probs = background_probs[background_probs > 0]

    # Empirical statistics
    signal_median = np.median(signal_probs)
    bg_median = np.median(background_probs)
    empirical_disc = signal_median / bg_median

    # RAVEN statistics
    raven_far_signal = np.array([calculate_raven_far(p) for p in signal_probs])
    raven_far_signal = raven_far_signal[np.isfinite(raven_far_signal)]
    raven_far_per_yr = raven_far_signal * (365.25 * 24 * 3600)

    raven_median = np.median(raven_far_per_yr)
    raven_95th = np.percentile(raven_far_per_yr, 95)
    n_significant = np.sum(raven_far_per_yr < 1.0)
    frac_significant = 100 * n_significant / len(raven_far_per_yr)

    print(f"\n{label}:")
    print(f"  Empirical:")
    print(f"    Signal median P_spatial: {signal_median:.6f}")
    print(f"    Background median P_spatial: {bg_median:.6f}")
    print(f"    Discrimination: {empirical_disc:.0f}×")
    print(f"  RAVEN Analytical:")
    print(f"    Median FAR: {raven_median:.2e} /yr")
    print(f"    95th percentile FAR: {raven_95th:.2e} /yr")
    print(f"    Signals with FAR < 1/yr: {n_significant}/{len(raven_far_per_yr)} ({frac_significant:.1f}%)")
    print(f"    Spatial correction: {1/signal_median:.0f}× (= 1/P_spatial)")

print("\n" + "="*70)
print("KEY INSIGHTS:")
print("="*70)
print("1. RAVEN formula calculates FAR for individual coincidences")
print("2. Empirical method measures discrimination between distributions")
print("3. Both approaches are valid but measure different things:")
print("   - RAVEN: 'How often by chance?' (event-by-event)")
print("   - Empirical: 'How much more likely is signal?' (population-level)")
print("4. RAVEN FAR < 1/yr indicates significant coincidences")
print("5. High empirical discrimination indicates good separability")
print("="*70)
