#!/usr/bin/env python3
"""Analyze GRB error radius distribution from real VOEvent data"""

import xml.etree.ElementTree as ET
import numpy as np
import matplotlib.pyplot as plt
from pathlib import Path
from matplotlib import rcParams
import sys

# Set nice plot style
rcParams['font.size'] = 12
rcParams['font.family'] = 'sans-serif'

# Determine paths
repo_root = Path(__file__).resolve().parent.parent.parent
output_dir = repo_root / 'assets'
output_dir.mkdir(exist_ok=True)

# Parse all VOEvent XML files
# Try sibling repository first, then fall back to absolute path
voevent_dir = repo_root.parent / 'growth-too-marshal-gcn-notices' / 'notices'
if not voevent_dir.exists():
    voevent_dir = Path("/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices")

if not voevent_dir.exists():
    print(f"ERROR: VOEvent directory not found at {voevent_dir}")
    print("Please ensure growth-too-marshal-gcn-notices repository is available.")
    sys.exit(1)

xml_files = list(voevent_dir.glob("*.xml"))

print(f"Found {len(xml_files)} VOEvent XML files")

error_radii = []
instruments = []

for xml_file in xml_files:
    try:
        tree = ET.parse(xml_file)
        root = tree.getroot()

        # Define namespaces
        ns = {'voe': 'http://www.ivoa.net/xml/VOEvent/v2.0'}

        # Extract instrument from ivorn
        ivorn = root.get('ivorn', '')
        if 'Fermi' in ivorn or 'GBM' in ivorn:
            instrument = 'Fermi-GBM'
        elif 'Swift' in ivorn or 'BAT' in ivorn:
            instrument = 'Swift-BAT'
        else:
            instrument = 'Other'

        # Find Error2Radius in Position2D
        error_elem = root.find('.//Error2Radius')
        if error_elem is not None:
            error_rad = float(error_elem.text)

            # Filter out 1.0 deg placeholders
            if abs(error_rad - 1.0) < 0.01:
                continue

            error_radii.append(error_rad)
            instruments.append(instrument)

    except Exception as e:
        # Skip files that can't be parsed
        continue

error_radii = np.array(error_radii)
instruments = np.array(instruments)

print(f"\nParsed {len(error_radii)} GRB error radii (excluding 1.0° placeholders)")

# Separate by instrument
fermi_radii = error_radii[instruments == 'Fermi-GBM']
swift_radii = error_radii[instruments == 'Swift-BAT']
other_radii = error_radii[instruments == 'Other']

print(f"\nInstrument breakdown:")
print(f"  Fermi-GBM: {len(fermi_radii)} events")
print(f"  Swift-BAT: {len(swift_radii)} events")
print(f"  Other: {len(other_radii)} events")

# Print statistics
if len(fermi_radii) > 0:
    print(f"\nFermi-GBM error radii:")
    print(f"  Min: {fermi_radii.min():.2f}°")
    print(f"  25th: {np.percentile(fermi_radii, 25):.2f}°")
    print(f"  Median: {np.median(fermi_radii):.2f}°")
    print(f"  75th: {np.percentile(fermi_radii, 75):.2f}°")
    print(f"  Max: {fermi_radii.max():.2f}°")
    print(f"  Mean: {fermi_radii.mean():.2f}°")

if len(swift_radii) > 0:
    print(f"\nSwift-BAT error radii:")
    print(f"  Min: {swift_radii.min():.4f}°")
    print(f"  25th: {np.percentile(swift_radii, 25):.4f}°")
    print(f"  Median: {np.median(swift_radii):.4f}°")
    print(f"  75th: {np.percentile(swift_radii, 75):.4f}°")
    print(f"  Max: {swift_radii.max():.4f}°")
    print(f"  Mean: {swift_radii.mean():.4f}°")

# Create distribution plot
fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))

# Left panel: Combined histogram on log scale
bins_log = np.logspace(-3, 2, 50)  # 0.001° to 100°

if len(fermi_radii) > 0:
    ax1.hist(fermi_radii, bins=bins_log, alpha=0.6, color='#E63946',
             label=f'Fermi-GBM (N={len(fermi_radii)})', histtype='stepfilled')
if len(swift_radii) > 0:
    ax1.hist(swift_radii, bins=bins_log, alpha=0.6, color='#457B9D',
             label=f'Swift-BAT (N={len(swift_radii)})', histtype='stepfilled')

ax1.set_xlabel('Error Radius (degrees)')
ax1.set_ylabel('Count')
ax1.set_title('GRB Error Radius Distribution')
ax1.set_xscale('log')
ax1.legend()
ax1.grid(True, alpha=0.3, which='both')

# Add median lines
if len(fermi_radii) > 0:
    fermi_median = np.median(fermi_radii)
    ax1.axvline(fermi_median, color='#E63946', linestyle='--', linewidth=2, alpha=0.8)
    ax1.text(fermi_median * 1.2, ax1.get_ylim()[1] * 0.9,
             f'Fermi median: {fermi_median:.1f}°',
             rotation=0, fontsize=10, color='#E63946')

if len(swift_radii) > 0:
    swift_median = np.median(swift_radii)
    ax1.axvline(swift_median, color='#457B9D', linestyle='--', linewidth=2, alpha=0.8)
    ax1.text(swift_median * 1.2, ax1.get_ylim()[1] * 0.8,
             f'Swift median: {swift_median:.3f}°',
             rotation=0, fontsize=10, color='#457B9D')

# Right panel: CDF
if len(fermi_radii) > 0:
    fermi_sorted = np.sort(fermi_radii)
    fermi_cdf = np.arange(1, len(fermi_sorted) + 1) / len(fermi_sorted)
    ax2.plot(fermi_sorted, fermi_cdf, color='#E63946', linewidth=2,
             label=f'Fermi-GBM (N={len(fermi_radii)})')

if len(swift_radii) > 0:
    swift_sorted = np.sort(swift_radii)
    swift_cdf = np.arange(1, len(swift_sorted) + 1) / len(swift_sorted)
    ax2.plot(swift_sorted, swift_cdf, color='#457B9D', linewidth=2,
             label=f'Swift-BAT (N={len(swift_radii)})')

ax2.set_xlabel('Error Radius (degrees)')
ax2.set_ylabel('Cumulative Probability')
ax2.set_title('Cumulative Distribution Function')
ax2.set_xscale('log')
ax2.legend()
ax2.grid(True, alpha=0.3, which='both')

# Add reference lines
ax2.axhline(0.5, color='gray', linestyle='--', alpha=0.5, linewidth=1)
ax2.text(0.001, 0.52, 'Median', fontsize=9, color='gray')

plt.tight_layout()
output_file = output_dir / 'grb_error_radius_distribution.png'
plt.savefig(output_file, dpi=300, bbox_inches='tight')
print(f"\nPlot saved to: {output_file}")

# Save summary statistics to file
summary_file = '/tmp/grb_error_radius_summary.txt'
with open(summary_file, 'w') as f:
    f.write("GRB Error Radius Distribution Summary\n")
    f.write("="*50 + "\n\n")
    f.write(f"Total GRBs: {len(error_radii)}\n")
    f.write(f"  Fermi-GBM: {len(fermi_radii)}\n")
    f.write(f"  Swift-BAT: {len(swift_radii)}\n")
    f.write(f"  Other: {len(other_radii)}\n\n")

    if len(fermi_radii) > 0:
        f.write(f"Fermi-GBM Statistics:\n")
        f.write(f"  Median: {np.median(fermi_radii):.2f}°\n")
        f.write(f"  Mean: {fermi_radii.mean():.2f}°\n")
        f.write(f"  Std: {fermi_radii.std():.2f}°\n")
        f.write(f"  Range: [{fermi_radii.min():.2f}°, {fermi_radii.max():.2f}°]\n\n")

    if len(swift_radii) > 0:
        f.write(f"Swift-BAT Statistics:\n")
        f.write(f"  Median: {np.median(swift_radii):.4f}° ({np.median(swift_radii)*60:.2f} arcmin)\n")
        f.write(f"  Mean: {swift_radii.mean():.4f}° ({swift_radii.mean()*60:.2f} arcmin)\n")
        f.write(f"  Std: {swift_radii.std():.4f}°\n")
        f.write(f"  Range: [{swift_radii.min():.4f}°, {swift_radii.max():.4f}°]\n")

print(f"Summary written to: {summary_file}")
