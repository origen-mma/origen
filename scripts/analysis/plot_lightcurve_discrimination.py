#!/usr/bin/env python3
"""Illustrate temporal discrimination using simulated light curves"""

import numpy as np
import matplotlib.pyplot as plt
from matplotlib import rcParams
from pathlib import Path
import subprocess
import sys

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

print("Generating synthetic light curves using Rust simulation...")

# Use Rust to generate light curve examples
# We'll create a simple Rust program that outputs light curve data
rust_generate_code = '''
use mm_core::{svi_models, Photometry};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn main() {
    let mut rng = StdRng::seed_from_u64(42);

    // Generate kilonova
    println!("KILONOVA");
    let kn_params = vec![-2.0, -1.0, 0.5, 0.0, -3.0];
    let kn_times = vec![0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 14.0];
    let kn_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::MetzgerKN,
        &kn_params,
        &kn_times
    );

    let scale = 200.0;
    let snr = 20.0;
    for (i, &t) in kn_times.iter().enumerate() {
        let flux = kn_fluxes[i] * scale;
        let err = flux / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let noisy_flux = (flux + noise).max(0.1);
        println!("{:.3} {:.6} {:.6}", t, noisy_flux, err);
    }

    // Generate supernova at t0=10 days
    println!("SUPERNOVA_T0_10");
    let sn_params = vec![0.0, 0.0, 10.0, (3.0_f64).ln(), (25.0_f64).ln(), -3.0];
    let sn_times: Vec<f64> = (0..30).map(|i| 5.0 + i as f64 * 1.5).collect();
    let sn_fluxes = svi_models::eval_model_batch(
        svi_models::SviModel::Bazin,
        &sn_params,
        &sn_times
    );

    let scale = 150.0;
    for (i, &t) in sn_times.iter().enumerate() {
        let flux = sn_fluxes[i] * scale;
        let err = flux.max(1.0) / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let noisy_flux = (flux + noise).max(0.1);
        println!("{:.3} {:.6} {:.6}", t, noisy_flux, err);
    }

    // Generate another SN at t0=20 days
    println!("SUPERNOVA_T0_20");
    let sn_params2 = vec![0.0, 0.0, 20.0, (3.0_f64).ln(), (25.0_f64).ln(), -3.0];
    let sn_times2: Vec<f64> = (0..30).map(|i| 15.0 + i as f64 * 1.5).collect();
    let sn_fluxes2 = svi_models::eval_model_batch(
        svi_models::SviModel::Bazin,
        &sn_params2,
        &sn_times2
    );

    for (i, &t) in sn_times2.iter().enumerate() {
        let flux = sn_fluxes2[i] * scale;
        let err = flux.max(1.0) / snr;
        let noise = rng.gen::<f64>() * err * 2.0 - err;
        let noisy_flux = (flux + noise).max(0.1);
        println!("{:.3} {:.6} {:.6}", t, noisy_flux, err);
    }
}
'''

# Save and compile Rust generator
temp_dir = Path('/tmp/lightcurve_gen')
temp_dir.mkdir(exist_ok=True)
(temp_dir / 'Cargo.toml').write_text('''
[package]
name = "lightcurve_gen"
version = "0.1.0"
edition = "2021"

[dependencies]
mm-core = { path = "%s/crates/mm-core" }
rand = "0.8"
''' % repo_root)

src_dir = temp_dir / 'src'
src_dir.mkdir(exist_ok=True)
(src_dir / 'main.rs').write_text(rust_generate_code)

print("Compiling light curve generator...")
result = subprocess.run(
    ['cargo', 'build', '--release', '--manifest-path', str(temp_dir / 'Cargo.toml')],
    capture_output=True,
    text=True
)

if result.returncode != 0:
    print("Error compiling generator:")
    print(result.stderr)
    sys.exit(1)

# Run generator
print("Running generator...")
result = subprocess.run(
    [str(temp_dir / 'target/release/lightcurve_gen')],
    capture_output=True,
    text=True
)

if result.returncode != 0:
    print("Error running generator:")
    print(result.stderr)
    sys.exit(1)

# Parse output
lines = result.stdout.strip().split('\n')
data = {'KILONOVA': [], 'SUPERNOVA_T0_10': [], 'SUPERNOVA_T0_20': []}
current_type = None

for line in lines:
    line = line.strip()
    if line in data.keys():
        current_type = line
    elif current_type and line:
        try:
            t, flux, err = map(float, line.split())
            data[current_type].append((t, flux, err))
        except:
            pass

# Convert to numpy arrays
for key in data:
    if data[key]:
        data[key] = np.array(data[key])

print(f"Loaded {len(data['KILONOVA'])} KN points, {len(data['SUPERNOVA_T0_10'])} SN points")

# Create comprehensive figure
fig = plt.figure(figsize=(16, 12))
gs = fig.add_gridspec(3, 2, hspace=0.35, wspace=0.3)

colors = {'KILONOVA': '#E76F51', 'SUPERNOVA_T0_10': '#F4A261', 'SUPERNOVA_T0_20': '#E9C46A'}

# ========== PANEL 1: Example Kilonova ==========
ax1 = fig.add_subplot(gs[0, 0])

kn_data = data['KILONOVA']
ax1.errorbar(kn_data[:, 0], kn_data[:, 1], yerr=kn_data[:, 2],
             fmt='o', color=colors['KILONOVA'], markersize=6, capsize=3,
             label='Kilonova (prompt, t0=0)', alpha=0.8)

# Mark GW merger time
ax1.axvline(0, color='black', linestyle='--', linewidth=2, alpha=0.5, label='GW merger')
ax1.axvspan(-0.5, 1.0, alpha=0.1, color='green', label='GW window (1 day)')

ax1.set_xlabel('Time since GW merger (days)')
ax1.set_ylabel('Flux (arbitrary units)')
ax1.set_title('Signal: Kilonova Light Curve')
ax1.legend(loc='upper right', fontsize=9)
ax1.grid(True, alpha=0.3)
ax1.set_xlim(-1, 15)

# Add annotation
ax1.text(0.05, 0.95, 'Prompt emission\nFast evolution (~1 week)\nt0 = 0 days',
         transform=ax1.transAxes, verticalalignment='top',
         bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.7),
         fontsize=9)

# ========== PANEL 2: Example Supernova at t0=10d ==========
ax2 = fig.add_subplot(gs[0, 1])

sn_data = data['SUPERNOVA_T0_10']
ax2.errorbar(sn_data[:, 0], sn_data[:, 1], yerr=sn_data[:, 2],
             fmt='s', color=colors['SUPERNOVA_T0_10'], markersize=6, capsize=3,
             label='Supernova (random time)', alpha=0.8)

ax2.axvline(0, color='black', linestyle='--', linewidth=2, alpha=0.5, label='GW merger')
ax2.axvline(10, color=colors['SUPERNOVA_T0_10'], linestyle=':', linewidth=2, alpha=0.7, label='SN t0=10d')
ax2.axvspan(-0.5, 1.0, alpha=0.1, color='green', label='GW window (1 day)')

ax2.set_xlabel('Time since GW merger (days)')
ax2.set_ylabel('Flux (arbitrary units)')
ax2.set_title('Background: Supernova Light Curve (Random t0)')
ax2.legend(loc='upper right', fontsize=9)
ax2.grid(True, alpha=0.3)
ax2.set_xlim(-1, 55)

# Add annotation
ax2.text(0.05, 0.95, 'Random arrival time\nSlow evolution (~months)\nt0 = 10 days (outside window)',
         transform=ax2.transAxes, verticalalignment='top',
         bbox=dict(boxstyle='round', facecolor='lightblue', alpha=0.7),
         fontsize=9)

# ========== PANEL 3: Multiple Kilonovae (Population) ==========
ax3 = fig.add_subplot(gs[1, 0])

# Simulate multiple kilonovae with scatter
np.random.seed(42)
n_kn = 10
for i in range(n_kn):
    # Add small time jitter to show population
    time_jitter = np.random.uniform(-0.2, 0.2)
    flux_scatter = np.random.uniform(0.8, 1.2)

    times = kn_data[:, 0] + time_jitter
    fluxes = kn_data[:, 1] * flux_scatter
    errors = kn_data[:, 2] * flux_scatter

    alpha = 0.3 if i > 0 else 0.8
    label = 'Kilonova population' if i == 0 else None
    ax3.errorbar(times, fluxes, yerr=errors, fmt='o', color=colors['KILONOVA'],
                markersize=4, capsize=2, alpha=alpha, label=label)

ax3.axvline(0, color='black', linestyle='--', linewidth=2, alpha=0.5, label='GW merger')
ax3.axvspan(-0.5, 1.0, alpha=0.15, color='green')

ax3.set_xlabel('Time since GW merger (days)')
ax3.set_ylabel('Flux (arbitrary units)')
ax3.set_title(f'Signal Population: {n_kn} Kilonovae (All Prompt)')
ax3.legend(loc='upper right', fontsize=9)
ax3.grid(True, alpha=0.3)
ax3.set_xlim(-1, 15)

# Add text
ax3.text(0.5, 0.05, 'All transients appear within ~1 day of merger\nTight temporal clustering enables discrimination',
         transform=ax3.transAxes, ha='center',
         bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.7),
         fontsize=9)

# ========== PANEL 4: Multiple Supernovae (Population) ==========
ax4 = fig.add_subplot(gs[1, 1])

# Simulate multiple SNe with random t0
n_sn = 10
sn_t0s = np.random.uniform(5, 30, n_sn)

for i, t0 in enumerate(sn_t0s):
    # Create SN light curve at this t0
    times = np.linspace(t0 - 5, t0 + 40, 25)
    # Simple Bazin model approximation
    tau_rise = 3.0
    tau_fall = 25.0
    fluxes = 150.0 * np.exp(-(times - t0) / tau_fall) / (1 + np.exp(-(times - t0) / tau_rise))

    alpha = 0.3 if i > 0 else 0.8
    color_alpha = 0.4 if i > 0 else 0.8
    label = 'Supernova population' if i == 0 else None
    ax4.plot(times, fluxes, 'o-', color=colors['SUPERNOVA_T0_10'],
            markersize=3, alpha=alpha, linewidth=1, label=label)

    # Mark each SN's t0
    ax4.axvline(t0, color=colors['SUPERNOVA_T0_10'], linestyle=':', linewidth=1, alpha=0.2)

ax4.axvline(0, color='black', linestyle='--', linewidth=2, alpha=0.5, label='GW merger')
ax4.axvspan(-0.5, 1.0, alpha=0.15, color='green', label='GW window')

ax4.set_xlabel('Time since GW merger (days)')
ax4.set_ylabel('Flux (arbitrary units)')
ax4.set_title(f'Background Population: {n_sn} Supernovae (Random Times)')
ax4.legend(loc='upper right', fontsize=9)
ax4.grid(True, alpha=0.3)
ax4.set_xlim(-1, 60)

# Add text
ax4.text(0.5, 0.05, 'Transients appear at random times over ~30 days\nNo temporal clustering around merger',
         transform=ax4.transAxes, ha='center',
         bbox=dict(boxstyle='round', facecolor='lightblue', alpha=0.7),
         fontsize=9)

# ========== PANEL 5: Fitted t0 Distribution (Simulated) ==========
ax5 = fig.add_subplot(gs[2, :])

# Simulate fitted t0 distributions
n_sim = 1000
np.random.seed(42)

# Kilonova: true t0=0, fitted with ~0.5 day error
kn_fitted_t0 = np.random.normal(0, 0.5, n_sim)

# Supernova: random true t0 in [0, 30], fitted with ~2 day error
sn_true_t0 = np.random.uniform(0, 30, n_sim)
sn_fitted_t0 = sn_true_t0 + np.random.normal(0, 2.0, n_sim)

bins = np.linspace(-5, 35, 50)
ax5.hist(kn_fitted_t0, bins=bins, alpha=0.6, color=colors['KILONOVA'],
        label=f'Kilonova (n={n_sim}, prompt)', density=True, histtype='stepfilled')
ax5.hist(sn_fitted_t0, bins=bins, alpha=0.6, color=colors['SUPERNOVA_T0_10'],
        label=f'Supernova (n={n_sim}, random)', density=True, histtype='stepfilled')

# Shade GW correlation window
ax5.axvspan(-1/86400, 1.0, alpha=0.2, color='green', label='GW window (-1s to +1d)')
ax5.axvline(0, color='black', linestyle='--', linewidth=2, alpha=0.5, label='GW merger')

ax5.set_xlabel('Fitted t0 (days after GW)')
ax5.set_ylabel('Probability Density')
ax5.set_title('Temporal Discrimination: Fitted t0 Distribution')
ax5.legend(loc='upper right')
ax5.grid(True, alpha=0.3)

# Calculate and display discrimination
kn_in_window = np.sum((kn_fitted_t0 >= -1/86400) & (kn_fitted_t0 <= 1.0)) / len(kn_fitted_t0)
sn_in_window = np.sum((sn_fitted_t0 >= -1/86400) & (sn_fitted_t0 <= 1.0)) / len(sn_fitted_t0)
discrimination = kn_in_window / max(sn_in_window, 0.001)

stats_text = (
    f"TEMPORAL DISCRIMINATION\n"
    f"━━━━━━━━━━━━━━━━━━━━━━━━\n"
    f"KN efficiency: {100*kn_in_window:.1f}%\n"
    f"SN contamination: {100*sn_in_window:.1f}%\n"
    f"Discrimination: {discrimination:.0f}×\n"
    f"\n"
    f"Combined with spatial:\n"
    f"  Spatial only: 290,000×\n"
    f"  Temporal: {discrimination:.0f}×\n"
    f"  Total: ~{290000 * discrimination / 1e6:.1f} million×"
)
ax5.text(0.98, 0.97, stats_text, transform=ax5.transAxes,
        verticalalignment='top', horizontalalignment='right',
        bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.9),
        fontsize=10, family='monospace')

plt.tight_layout()

# Save figure
output_file = output_dir / 'temporal_discrimination_illustration.png'
plt.savefig(output_file, dpi=300, bbox_inches='tight')
print(f"\nPlot saved to: {output_file}")

print("\n" + "="*60)
print("TEMPORAL DISCRIMINATION ILLUSTRATION")
print("="*60)
print("\nKey Points:")
print("  • Kilonovae appear promptly within ~1 day of GW merger")
print("  • Supernovae occur at random times over weeks/months")
print(f"  • Temporal discrimination: ~{discrimination:.0f}× rejection of random SNe")
print(f"  • Combined spatio-temporal discrimination: ~{290000 * discrimination / 1e6:.1f} million×")
print("\n" + "="*60)
