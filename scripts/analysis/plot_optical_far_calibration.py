#!/usr/bin/env python3
"""Plot kilonova vs supernova spatial probability distributions"""

import numpy as np
import matplotlib.pyplot as plt
from matplotlib import rcParams
from pathlib import Path

# Set nice plot style
rcParams['font.size'] = 12
rcParams['font.family'] = 'sans-serif'
rcParams['axes.labelsize'] = 14
rcParams['axes.titlesize'] = 16
rcParams['legend.fontsize'] = 11

# Determine data directory
repo_root = Path(__file__).resolve().parent.parent.parent
data_dir = Path('/tmp')
if not (data_dir / 'far_calibration_optical.dat').exists():
    data_dir = repo_root / 'data' / 'far_calibration'
    print(f"Using data directory: {data_dir}")

output_dir = repo_root / 'assets'
output_dir.mkdir(exist_ok=True)

# Load data
signal_probs = []
background_probs = []

data_file = data_dir / 'far_calibration_optical.dat'
with open(data_file, 'r') as f:
    for line in f:
        if line.startswith('#'):
            continue
        parts = line.strip().split()
        if len(parts) == 2:
            prob_type, prob_value = parts
            prob = float(prob_value)
            if prob_type == 'signal':
                signal_probs.append(prob)
            elif prob_type == 'background':
                background_probs.append(prob)

signal_probs = np.array(signal_probs)
background_probs = np.array(background_probs)

print(f"Loaded {len(signal_probs)} kilonova (signal) trials")
print(f"Loaded {len(background_probs)} supernova (background) trials")

# Create figure with two subplots
fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))

# Filter out zeros for log-scale plotting
signal_nonzero = signal_probs[signal_probs > 1e-8]
background_nonzero = background_probs[background_probs > 1e-8]

bins_log = np.logspace(-8, 0, 50)

# Left: Histogram with density
ax1.hist(background_nonzero, bins=bins_log, alpha=0.6, color='#F4A261',
         label=f'Supernova (N={len(background_probs)})', density=True, histtype='stepfilled')
ax1.hist(signal_nonzero, bins=bins_log, alpha=0.8, color='#E76F51',
         label=f'Kilonova (N={len(signal_probs)})', density=True,
         histtype='step', linewidth=2.5)

ax1.set_xlabel('Spatial Probability')
ax1.set_ylabel('Probability Density')
ax1.set_title('Optical Transients: Kilonova vs Supernova')
ax1.set_xscale('log')
ax1.set_yscale('log')
ax1.legend()
ax1.grid(True, alpha=0.3, which='both')

# Add stats box
signal_med = np.median(signal_probs)
bg_med = np.median(background_probs)
stats_text = (
    f"Kilonova median: {signal_med:.6e}\n"
    f"Supernova median: {bg_med:.6e}\n"
    f"Ratio: {signal_med/max(bg_med, 1e-10):.1e}×\n"
    f"\n"
    f"Position error: 2 arcsec\n"
    f"Time window: -1s to +1 day\n"
    f"SN rate / KN rate: ~10,000×"
)
ax1.text(0.03, 0.97, stats_text, transform=ax1.transAxes,
         verticalalignment='top', bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.8),
         fontsize=9, family='monospace')

# Right: Complementary CDF
signal_sorted = np.sort(signal_probs)
bg_sorted = np.sort(background_probs)
signal_ccdf = 1 - np.arange(1, len(signal_sorted) + 1) / len(signal_sorted)
bg_ccdf = 1 - np.arange(1, len(bg_sorted) + 1) / len(bg_sorted)

ax2.plot(bg_sorted, bg_ccdf, color='#F4A261', linewidth=2, alpha=0.6,
         label=f'Supernova (N={len(background_probs)})')
ax2.plot(signal_sorted, signal_ccdf, color='#E76F51', linewidth=2.5,
         label=f'Kilonova (N={len(signal_probs)})')
ax2.set_xlabel('Spatial Probability')
ax2.set_ylabel('Complementary CDF: P(X > x)')
ax2.set_title('Tail Probability (Log-Log)')
ax2.set_xscale('log')
ax2.set_yscale('log')
ax2.legend()
ax2.grid(True, alpha=0.3, which='both')

# Add text explaining
n_sig_zero = np.sum(signal_probs < 1e-8)
n_bg_zero = np.sum(background_probs < 1e-8)
zero_text = (
    f"Zero probability:\n"
    f"  KN: {100*n_sig_zero/len(signal_probs):.1f}%\n"
    f"  SN: {100*n_bg_zero/len(background_probs):.1f}%"
)
ax2.text(0.05, 0.05, zero_text, transform=ax2.transAxes,
         bbox=dict(boxstyle='round', facecolor='lightblue', alpha=0.5),
         fontsize=9, family='monospace')

plt.tight_layout()

# Save plot
output_file = output_dir / 'far_calibration_optical.png'
plt.savefig(output_file, dpi=300, bbox_inches='tight')
print(f"\nPlot saved to: {output_file}")

# Print summary
print("\n" + "="*60)
print("OPTICAL FAR CALIBRATION SUMMARY")
print("="*60)
print(f"\nKilonova (N={len(signal_probs)}):")
print(f"  Median: {np.median(signal_probs):.6e}")
print(f"  Mean: {np.mean(signal_probs):.6e}")
print(f"  Zero probability: {100*n_sig_zero/len(signal_probs):.1f}%")

print(f"\nSupernova (N={len(background_probs)}):")
print(f"  Median: {np.median(background_probs):.6e}")
print(f"  Mean: {np.mean(background_probs):.6e}")
print(f"  Zero probability: {100*n_bg_zero/len(background_probs):.1f}%")

print(f"\nDiscrimination:")
print(f"  Median ratio: {np.median(signal_probs)/max(np.median(background_probs), 1e-10):.1e}×")
print(f"  Mean ratio: {np.mean(signal_probs)/np.mean(background_probs):.1f}×")

# Fraction exceeding bg 95th percentile
bg_95th = np.percentile(background_probs, 95)
n_exceed = np.sum(signal_probs > bg_95th)
print(f"  KN exceeding SN 95th percentile: {n_exceed}/{len(signal_probs)} ({100*n_exceed/len(signal_probs):.1f}%)")

print("\n" + "="*60)
print("\nNote: Tiny optical error circle (2 arcsec) yields exceptional")
print("spatial discrimination, similar to Swift-BAT (~2 arcmin).")
print("Temporal discrimination is even stronger: GRBs are prompt,")
print("SNe occur randomly in time within +1 day window.")
