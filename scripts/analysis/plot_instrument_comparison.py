#!/usr/bin/env python3
"""Compare Fermi-GBM vs Swift-BAT spatial probability distributions"""

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
# Try /tmp first (where test outputs), then fall back to data/ directory
repo_root = Path(__file__).resolve().parent.parent.parent
data_dir = Path('/tmp')
if not (data_dir / 'far_calibration_fermi_gbm.dat').exists():
    data_dir = repo_root / 'data' / 'far_calibration'
    print(f"Using data directory: {data_dir}")

# Output directory
output_dir = repo_root / 'assets'
output_dir.mkdir(exist_ok=True)

# Load data for both instruments
instruments = {
    'Fermi-GBM': {'file': str(data_dir / 'far_calibration_fermi_gbm.dat'), 'error': 13.2, 'color': '#E63946'},
    'Swift-BAT': {'file': str(data_dir / 'far_calibration_swift_bat.dat'), 'error': 0.033, 'color': '#457B9D'},
}

for name, info in instruments.items():
    signal_probs = []
    background_probs = []

    with open(info['file'], 'r') as f:
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

    info['signal'] = np.array(signal_probs)
    info['background'] = np.array(background_probs)
    print(f"{name}: {len(signal_probs)} signal, {len(background_probs)} background")

# Create 2x2 figure
fig = plt.figure(figsize=(16, 12))
gs = fig.add_gridspec(2, 2, hspace=0.3, wspace=0.3)

# ========== ROW 1: FERMI-GBM ==========
ax_fermi_hist = fig.add_subplot(gs[0, 0])
ax_fermi_cdf = fig.add_subplot(gs[0, 1])

fermi = instruments['Fermi-GBM']
signal_nonzero = fermi['signal'][fermi['signal'] > 1e-8]
bg_nonzero = fermi['background'][fermi['background'] > 1e-8]

# Fermi histogram
bins_log = np.logspace(-8, 0, 50)
ax_fermi_hist.hist(bg_nonzero, bins=bins_log, alpha=0.6, color=fermi['color'],
                   label=f'Background (N={len(fermi["background"])})', density=True, histtype='stepfilled')
ax_fermi_hist.hist(signal_nonzero, bins=bins_log, alpha=0.8, color=fermi['color'],
                   label=f'Signal (N={len(fermi["signal"])})', density=True,
                   histtype='step', linewidth=2.5)

ax_fermi_hist.set_xlabel('Spatial Probability')
ax_fermi_hist.set_ylabel('Probability Density')
ax_fermi_hist.set_title(f'Fermi-GBM (error radius = {fermi["error"]:.1f}°)')
ax_fermi_hist.set_xscale('log')
ax_fermi_hist.set_yscale('log')
ax_fermi_hist.legend()
ax_fermi_hist.grid(True, alpha=0.3, which='both')

# Add stats box
signal_med = np.median(fermi['signal'])
bg_med = np.median(fermi['background'])
stats_text = (
    f"Signal median: {signal_med:.4f}\n"
    f"Background median: {bg_med:.6e}\n"
    f"Ratio: {signal_med/bg_med:.1e}×"
)
ax_fermi_hist.text(0.03, 0.97, stats_text, transform=ax_fermi_hist.transAxes,
                   verticalalignment='top', bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.8),
                   fontsize=10, family='monospace')

# Fermi CDF
signal_sorted = np.sort(fermi['signal'])
bg_sorted = np.sort(fermi['background'])
signal_ccdf = 1 - np.arange(1, len(signal_sorted) + 1) / len(signal_sorted)
bg_ccdf = 1 - np.arange(1, len(bg_sorted) + 1) / len(bg_sorted)

ax_fermi_cdf.plot(bg_sorted, bg_ccdf, color=fermi['color'], linewidth=2, alpha=0.6,
                  label=f'Background (N={len(fermi["background"])})')
ax_fermi_cdf.plot(signal_sorted, signal_ccdf, color=fermi['color'], linewidth=2.5,
                  label=f'Signal (N={len(fermi["signal"])})')
ax_fermi_cdf.set_xlabel('Spatial Probability')
ax_fermi_cdf.set_ylabel('Complementary CDF: P(X > x)')
ax_fermi_cdf.set_title(f'Fermi-GBM: Tail Probability')
ax_fermi_cdf.set_xscale('log')
ax_fermi_cdf.set_yscale('log')
ax_fermi_cdf.legend()
ax_fermi_cdf.grid(True, alpha=0.3, which='both')

# ========== ROW 2: SWIFT-BAT ==========
ax_swift_hist = fig.add_subplot(gs[1, 0])
ax_swift_cdf = fig.add_subplot(gs[1, 1])

swift = instruments['Swift-BAT']
signal_nonzero = swift['signal'][swift['signal'] > 1e-8]
bg_nonzero = swift['background'][swift['background'] > 1e-8]

# Swift histogram
ax_swift_hist.hist(bg_nonzero, bins=bins_log, alpha=0.6, color=swift['color'],
                   label=f'Background (N={len(swift["background"])})', density=True, histtype='stepfilled')
ax_swift_hist.hist(signal_nonzero, bins=bins_log, alpha=0.8, color=swift['color'],
                   label=f'Signal (N={len(swift["signal"])})', density=True,
                   histtype='step', linewidth=2.5)

ax_swift_hist.set_xlabel('Spatial Probability')
ax_swift_hist.set_ylabel('Probability Density')
ax_swift_hist.set_title(f'Swift-BAT (error radius = {swift["error"]:.3f}° = 2 arcmin)')
ax_swift_hist.set_xscale('log')
ax_swift_hist.set_yscale('log')
ax_swift_hist.legend()
ax_swift_hist.grid(True, alpha=0.3, which='both')

# Add stats box
signal_med = np.median(swift['signal'])
bg_med = np.median(swift['background'])
stats_text = (
    f"Signal median: {signal_med:.6e}\n"
    f"Background median: {bg_med:.6e}\n"
    f"Ratio: {signal_med/max(bg_med, 1e-10):.1e}×"
)
ax_swift_hist.text(0.03, 0.97, stats_text, transform=ax_swift_hist.transAxes,
                   verticalalignment='top', bbox=dict(boxstyle='round', facecolor='lightblue', alpha=0.8),
                   fontsize=10, family='monospace')

# Swift CDF
signal_sorted = np.sort(swift['signal'])
bg_sorted = np.sort(swift['background'])
signal_ccdf = 1 - np.arange(1, len(signal_sorted) + 1) / len(signal_sorted)
bg_ccdf = 1 - np.arange(1, len(bg_sorted) + 1) / len(bg_sorted)

ax_swift_cdf.plot(bg_sorted, bg_ccdf, color=swift['color'], linewidth=2, alpha=0.6,
                  label=f'Background (N={len(swift["background"])})')
ax_swift_cdf.plot(signal_sorted, signal_ccdf, color=swift['color'], linewidth=2.5,
                  label=f'Signal (N={len(swift["signal"])})')
ax_swift_cdf.set_xlabel('Spatial Probability')
ax_swift_cdf.set_ylabel('Complementary CDF: P(X > x)')
ax_swift_cdf.set_title(f'Swift-BAT: Tail Probability')
ax_swift_cdf.set_xscale('log')
ax_swift_cdf.set_yscale('log')
ax_swift_cdf.legend()
ax_swift_cdf.grid(True, alpha=0.3, which='both')

# Save figure
output_file = output_dir / 'far_calibration_instrument_comparison.png'
plt.savefig(output_file, dpi=300, bbox_inches='tight')
print(f"\nComparison plot saved to: {output_file}")

# Print summary table
print("\n" + "="*70)
print("INSTRUMENT COMPARISON SUMMARY")
print("="*70)
for name, info in instruments.items():
    signal = info['signal']
    background = info['background']

    sig_med = np.median(signal)
    sig_mean = np.mean(signal)
    bg_med = np.median(background)
    bg_mean = np.mean(background)

    # Count zeros
    n_sig_zero = np.sum(signal < 1e-8)
    n_bg_zero = np.sum(background < 1e-8)

    print(f"\n{name} (error radius = {info['error']}°):")
    print(f"  Signal median: {sig_med:.6e}, Background median: {bg_med:.6e}")
    print(f"  Median ratio: {sig_med/max(bg_med, 1e-10):.1e}×")
    print(f"  Signal mean: {sig_mean:.6e}, Background mean: {bg_mean:.6e}")
    print(f"  Mean ratio: {sig_mean/bg_mean:.1f}×")
    print(f"  Zero probability: Signal {100*n_sig_zero/len(signal):.1f}%, Background {100*n_bg_zero/len(background):.1f}%")

print("\n" + "="*70)
