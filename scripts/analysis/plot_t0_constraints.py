#!/usr/bin/env python3
"""Plot t0 (explosion time) recovery validation results"""

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

# Determine paths
repo_root = Path(__file__).resolve().parent.parent.parent
output_dir = repo_root / 'assets'
output_dir.mkdir(exist_ok=True)

# Load data
data_file = Path('/tmp/t0_validation.dat')
if not data_file.exists():
    print(f"ERROR: {data_file} not found")
    print("Run: cargo test --package mm-core --test validate_t0_constraints -- --ignored --nocapture")
    exit(1)

kn_errors = []
kn_uncertainties = []
sn_errors = []
sn_uncertainties = []

with open(data_file, 'r') as f:
    for line in f:
        if line.startswith('#'):
            continue
        parts = line.strip().split()
        if len(parts) == 3:
            transient_type, t0_error, t0_unc = parts
            error = float(t0_error)
            unc = float(t0_unc)

            if transient_type == 'kilonova':
                kn_errors.append(error)
                kn_uncertainties.append(unc)
            elif transient_type == 'supernova':
                sn_errors.append(error)
                sn_uncertainties.append(unc)

kn_errors = np.array(kn_errors)
sn_errors = np.array(sn_errors)

print(f"Loaded {len(kn_errors)} kilonova and {len(sn_errors)} supernova t0 measurements")

# Create figure with multiple panels
fig = plt.figure(figsize=(16, 10))
gs = fig.add_gridspec(2, 2, hspace=0.3, wspace=0.3)

# ========== PANEL 1: t0 Error Histograms ==========
ax1 = fig.add_subplot(gs[0, 0])

bins = np.linspace(0, max(np.max(kn_errors), np.max(sn_errors)), 30)
ax1.hist(kn_errors, bins=bins, alpha=0.6, color='#E76F51',
         label=f'Kilonova (n={len(kn_errors)})', histtype='stepfilled')
ax1.hist(sn_errors, bins=bins, alpha=0.6, color='#F4A261',
         label=f'Supernova (n={len(sn_errors)})', histtype='stepfilled')

ax1.set_xlabel('|True t0 - Fitted t0| (days)')
ax1.set_ylabel('Count')
ax1.set_title('t0 Recovery Error Distribution')
ax1.legend()
ax1.grid(True, alpha=0.3)

# Add median lines
kn_median = np.median(kn_errors)
sn_median = np.median(sn_errors)
ax1.axvline(kn_median, color='#E76F51', linestyle='--', linewidth=2, alpha=0.8)
ax1.axvline(sn_median, color='#F4A261', linestyle='--', linewidth=2, alpha=0.8)

# Add stats box
stats_text = (
    f"Kilonova median: {kn_median:.2f}d\n"
    f"Supernova median: {sn_median:.2f}d\n"
    f"\n"
    f"KN < 1d: {100*np.sum(kn_errors < 1.0)/len(kn_errors):.0f}%\n"
    f"SN < 1d: {100*np.sum(sn_errors < 1.0)/len(sn_errors):.0f}%"
)
ax1.text(0.97, 0.97, stats_text, transform=ax1.transAxes,
         verticalalignment='top', horizontalalignment='right',
         bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.8),
         fontsize=10, family='monospace')

# ========== PANEL 2: CDF Comparison ==========
ax2 = fig.add_subplot(gs[0, 1])

kn_sorted = np.sort(kn_errors)
sn_sorted = np.sort(sn_errors)
kn_cdf = np.arange(1, len(kn_sorted) + 1) / len(kn_sorted)
sn_cdf = np.arange(1, len(sn_sorted) + 1) / len(sn_sorted)

ax2.plot(kn_sorted, kn_cdf, color='#E76F51', linewidth=2.5,
         label=f'Kilonova (n={len(kn_errors)})')
ax2.plot(sn_sorted, sn_cdf, color='#F4A261', linewidth=2.5,
         label=f'Supernova (n={len(sn_errors)})')

ax2.set_xlabel('|t0 Error| (days)')
ax2.set_ylabel('Cumulative Probability')
ax2.set_title('Cumulative Distribution Function')
ax2.legend()
ax2.grid(True, alpha=0.3)

# Add reference line at 1 day
ax2.axvline(1.0, color='gray', linestyle=':', linewidth=2, alpha=0.5)
ax2.text(1.05, 0.05, '1 day\n(GW-optical\nwindow)',
         fontsize=9, color='gray', verticalalignment='bottom')

# Highlight GW correlation window
ax2.axvspan(0, 1.0, alpha=0.1, color='green')
ax2.text(0.5, 0.95, 'GW Correlation\nWindow', transform=ax2.transAxes,
         fontsize=9, color='green', alpha=0.7, ha='center')

# ========== PANEL 3: t0 Error vs Uncertainty ==========
ax3 = fig.add_subplot(gs[1, 0])

ax3.scatter(kn_uncertainties, kn_errors, alpha=0.5, color='#E76F51',
            s=50, label='Kilonova', edgecolors='black', linewidth=0.5)
ax3.scatter(sn_uncertainties, sn_errors, alpha=0.5, color='#F4A261',
            s=50, label='Supernova', edgecolors='black', linewidth=0.5)

# Add diagonal line (error = uncertainty)
max_val = max(np.max(kn_uncertainties), np.max(sn_uncertainties))
ax3.plot([0, max_val], [0, max_val], 'k--', alpha=0.3, label='Error = Uncertainty')

ax3.set_xlabel('Fitted t0 Uncertainty (days)')
ax3.set_ylabel('Actual |t0 Error| (days)')
ax3.set_title('Error vs Uncertainty Calibration')
ax3.legend()
ax3.grid(True, alpha=0.3)

# ========== PANEL 4: Temporal Discrimination Power ==========
ax4 = fig.add_subplot(gs[1, 1])

# For a GW at t0=0, simulate distribution of fitted t0 for KN and SN
# KN: True t0=0, so fitted t0 ~ Normal(0, kn_median_error)
# SN: True t0 ~ Uniform(0, 30d), fitted t0 ~ true_t0 + error

n_sim = 10000
np.random.seed(42)

# Kilonova: prompt, so fitted t0 should be near 0
kn_fitted_t0 = np.random.normal(0, kn_median, n_sim)

# Supernova: random true t0, plus measurement error
sn_true_t0 = np.random.uniform(0, 30, n_sim)
sn_fitted_t0 = sn_true_t0 + np.random.normal(0, sn_median, n_sim)

bins_t0 = np.linspace(-5, 35, 50)
ax4.hist(kn_fitted_t0, bins=bins_t0, alpha=0.6, color='#E76F51',
         label='Kilonova (prompt)', density=True, histtype='stepfilled')
ax4.hist(sn_fitted_t0, bins=bins_t0, alpha=0.6, color='#F4A261',
         label='Supernova (random)', density=True, histtype='stepfilled')

ax4.set_xlabel('Fitted t0 (days after GW)')
ax4.set_ylabel('Probability Density')
ax4.set_title('Temporal Discrimination: Fitted t0 Distribution')
ax4.legend()
ax4.grid(True, alpha=0.3)

# Shade GW correlation window
ax4.axvspan(-1/86400, 1.0, alpha=0.2, color='green', label='GW window (-1s to +1d)')
ax4.axvline(0, color='black', linestyle='--', linewidth=1, alpha=0.5)
ax4.text(0.05, 0.95, f'GW merger\n(t0 = 0)', transform=ax4.transAxes,
         fontsize=9, verticalalignment='top')

# Calculate discrimination power
kn_in_window = np.sum((kn_fitted_t0 >= -1/86400) & (kn_fitted_t0 <= 1.0)) / len(kn_fitted_t0)
sn_in_window = np.sum((sn_fitted_t0 >= -1/86400) & (sn_fitted_t0 <= 1.0)) / len(sn_fitted_t0)
discrimination = kn_in_window / max(sn_in_window, 0.001)

disc_text = (
    f"Detection Efficiency:\n"
    f"  KN in window: {100*kn_in_window:.1f}%\n"
    f"  SN in window: {100*sn_in_window:.1f}%\n"
    f"  Discrimination: {discrimination:.0f}×"
)
ax4.text(0.97, 0.50, disc_text, transform=ax4.transAxes,
         verticalalignment='top', horizontalalignment='right',
         bbox=dict(boxstyle='round', facecolor='lightblue', alpha=0.8),
         fontsize=10, family='monospace')

plt.tight_layout()

# Save plot
output_file = output_dir / 't0_constraint_validation.png'
plt.savefig(output_file, dpi=300, bbox_inches='tight')
print(f"\nPlot saved to: {output_file}")

# Print summary
print("\n" + "="*60)
print("TEMPORAL DISCRIMINATION SUMMARY")
print("="*60)
print(f"\nKilonova t0 Recovery (n={len(kn_errors)}):")
print(f"  Median error: {np.median(kn_errors):.2f} days")
print(f"  RMS error: {np.sqrt(np.mean(kn_errors**2)):.2f} days")
print(f"  Fraction within 1 day: {100*np.sum(kn_errors < 1.0)/len(kn_errors):.1f}%")

print(f"\nSupernova t0 Recovery (n={len(sn_errors)}):")
print(f"  Median error: {np.median(sn_errors):.2f} days")
print(f"  RMS error: {np.sqrt(np.mean(sn_errors**2)):.2f} days")
print(f"  Fraction within 1 day: {100*np.sum(sn_errors < 1.0)/len(sn_errors):.1f}%")

print(f"\nTemporal Discrimination (simulated):")
print(f"  KN detection efficiency: {100*kn_in_window:.1f}%")
print(f"  SN contamination rate: {100*sn_in_window:.1f}%")
print(f"  Discrimination factor: {discrimination:.0f}×")

print(f"\nConclusion:")
if discrimination > 10:
    print(f"  ✅ Strong temporal discrimination ({discrimination:.0f}×)")
    print(f"     Kilonovae are clearly distinguishable from random SNe")
elif discrimination > 3:
    print(f"  ✓  Moderate temporal discrimination ({discrimination:.0f}×)")
    print(f"     Combined with spatial correlation provides good rejection")
else:
    print(f"  ⚠️  Weak temporal discrimination ({discrimination:.1f}×)")
    print(f"     Relies heavily on spatial correlation")

print("\n" + "="*60)
