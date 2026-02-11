#!/usr/bin/env python3
"""
Generate paper-quality figures for the ORIGIN paper.

Produces PDF figures in paper/figures/ for inclusion in LaTeX.
Uses data from data/far_calibration/ and /tmp/ test outputs.

Usage:
    python3 paper/generate_figures.py
"""

import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib import rcParams
from pathlib import Path

# --- Publication-quality style ---
rcParams.update({
    'font.size': 10,
    'font.family': 'serif',
    'text.usetex': False,
    'axes.labelsize': 11,
    'axes.titlesize': 11,
    'legend.fontsize': 9,
    'xtick.labelsize': 9,
    'ytick.labelsize': 9,
    'figure.dpi': 300,
    'savefig.dpi': 300,
    'savefig.bbox': 'tight',
    'lines.linewidth': 1.5,
})

repo_root = Path(__file__).resolve().parent.parent
paper_dir = repo_root / 'paper'
fig_dir = paper_dir / 'figures'
fig_dir.mkdir(exist_ok=True)

# Data directories
data_dir = repo_root / 'data' / 'far_calibration'
tmp_dir = Path('/tmp')


def load_far_data(filename):
    """Load signal/background probabilities from a .dat file."""
    filepath = tmp_dir / filename
    if not filepath.exists():
        filepath = data_dir / filename
    if not filepath.exists():
        print(f"  WARNING: {filename} not found in /tmp or data/far_calibration/")
        return None, None

    signal, background = [], []
    with open(filepath) as f:
        for line in f:
            if line.startswith('#'):
                continue
            parts = line.strip().split()
            if len(parts) == 2:
                if parts[0] == 'signal':
                    signal.append(float(parts[1]))
                elif parts[0] == 'background':
                    background.append(float(parts[1]))
    return np.array(signal), np.array(background)


# =====================================================================
# Figure 1: GRB spatial probability — Fermi-GBM vs Swift-BAT
# =====================================================================

def figure_far_calibration_grb():
    """2x2 panel: Fermi-GBM and Swift-BAT signal/background distributions."""
    print("Generating Figure 1: GRB FAR calibration...")

    instruments = {
        'Fermi-GBM': {
            'file': 'far_calibration_fermi_gbm.dat',
            'error_label': r'$\sigma = 13.2\degree$',
            'color_sig': '#c0392b',
            'color_bg': '#e74c3c',
        },
        'Swift-BAT': {
            'file': 'far_calibration_swift_bat.dat',
            'error_label': r'$\sigma = 2\,\mathrm{arcmin}$',
            'color_sig': '#2471a3',
            'color_bg': '#5dade2',
        },
    }

    fig, axes = plt.subplots(2, 2, figsize=(7.0, 6.0))

    for row, (name, info) in enumerate(instruments.items()):
        signal, background = load_far_data(info['file'])
        if signal is None:
            continue

        sig_nz = signal[signal > 1e-10]
        bg_nz = background[background > 1e-10]

        bins = np.logspace(-8, 0, 50)

        # Left: histogram
        ax = axes[row, 0]
        ax.hist(bg_nz, bins=bins, alpha=0.4, color=info['color_bg'],
                density=True, histtype='stepfilled',
                label=f'Background ($N={len(background):,}$)')
        ax.hist(sig_nz, bins=bins, color=info['color_sig'],
                density=True, histtype='step', linewidth=2,
                label=f'Signal ($N={len(signal)}$)')
        ax.set_xscale('log')
        ax.set_yscale('log')
        ax.set_xlabel('Spatial Probability $P_\\mathrm{spatial}$')
        ax.set_ylabel('Probability Density')
        ax.set_title(f'{name} ({info["error_label"]})')
        ax.legend(loc='upper left', framealpha=0.9)

        # Stats annotation
        sig_med = np.median(signal)
        bg_med = np.median(background)
        ratio = sig_med / max(bg_med, 1e-15)
        ax.text(0.97, 0.97,
                f'Median ratio: {ratio:.0e}$\\times$',
                transform=ax.transAxes, va='top', ha='right',
                fontsize=8, bbox=dict(boxstyle='round,pad=0.3',
                                      fc='white', ec='gray', alpha=0.9))

        # Right: complementary CDF
        ax = axes[row, 1]
        sig_sorted = np.sort(signal)
        bg_sorted = np.sort(background)
        sig_ccdf = 1 - np.arange(1, len(sig_sorted)+1) / len(sig_sorted)
        bg_ccdf = 1 - np.arange(1, len(bg_sorted)+1) / len(bg_sorted)

        ax.plot(bg_sorted, bg_ccdf, color=info['color_bg'], linewidth=1.5,
                alpha=0.6, label='Background')
        ax.plot(sig_sorted, sig_ccdf, color=info['color_sig'], linewidth=2,
                label='Signal')
        ax.set_xscale('log')
        ax.set_yscale('log')
        ax.set_xlabel('Spatial Probability $P_\\mathrm{spatial}$')
        ax.set_ylabel('$P(X > x)$')
        ax.set_title(f'{name}: Complementary CDF')
        ax.legend(loc='upper right', framealpha=0.9)

    fig.tight_layout()
    outfile = fig_dir / 'far_calibration_grb.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")


# =====================================================================
# Figure 2: Optical spatial probability — KN vs SN
# =====================================================================

def figure_far_calibration_optical():
    """1x2 panel: kilonova vs supernova spatial probability."""
    print("Generating Figure 2: Optical FAR calibration...")

    signal, background = load_far_data('far_calibration_optical.dat')
    if signal is None:
        return

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(7.0, 3.0))

    sig_nz = signal[signal > 1e-10]
    bg_nz = background[background > 1e-10]
    bins = np.logspace(-8, 0, 50)

    # Histogram
    ax1.hist(bg_nz, bins=bins, alpha=0.4, color='#f39c12',
             density=True, histtype='stepfilled',
             label=f'Supernova ($N={len(background):,}$)')
    ax1.hist(sig_nz, bins=bins, color='#e74c3c',
             density=True, histtype='step', linewidth=2,
             label=f'Kilonova ($N={len(signal)}$)')
    ax1.set_xscale('log')
    ax1.set_yscale('log')
    ax1.set_xlabel('Spatial Probability $P_\\mathrm{spatial}$')
    ax1.set_ylabel('Probability Density')
    ax1.set_title('KN vs SN (2 arcsec position error)')
    ax1.legend(loc='upper left', framealpha=0.9)

    sig_med = np.median(signal)
    bg_med = np.median(background)
    ratio = sig_med / max(bg_med, 1e-15)
    ax1.text(0.97, 0.97,
             f'Median ratio: {ratio:.0e}$\\times$',
             transform=ax1.transAxes, va='top', ha='right',
             fontsize=8, bbox=dict(boxstyle='round,pad=0.3',
                                   fc='white', ec='gray', alpha=0.9))

    # Complementary CDF
    sig_sorted = np.sort(signal)
    bg_sorted = np.sort(background)
    sig_ccdf = 1 - np.arange(1, len(sig_sorted)+1) / len(sig_sorted)
    bg_ccdf = 1 - np.arange(1, len(bg_sorted)+1) / len(bg_sorted)

    ax2.plot(bg_sorted, bg_ccdf, color='#f39c12', linewidth=1.5, alpha=0.6,
             label='Supernova')
    ax2.plot(sig_sorted, sig_ccdf, color='#e74c3c', linewidth=2,
             label='Kilonova')
    ax2.set_xscale('log')
    ax2.set_yscale('log')
    ax2.set_xlabel('Spatial Probability $P_\\mathrm{spatial}$')
    ax2.set_ylabel('$P(X > x)$')
    ax2.set_title('Complementary CDF')
    ax2.legend(loc='upper right', framealpha=0.9)

    n_sig_zero = np.sum(signal < 1e-10)
    n_bg_zero = np.sum(background < 1e-10)
    ax2.text(0.05, 0.05,
             f'Zero prob: KN {100*n_sig_zero/len(signal):.1f}%, '
             f'SN {100*n_bg_zero/len(background):.1f}%',
             transform=ax2.transAxes, fontsize=7,
             bbox=dict(boxstyle='round,pad=0.3', fc='lightyellow', alpha=0.8))

    fig.tight_layout()
    outfile = fig_dir / 'far_calibration_optical.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")


# =====================================================================
# Figure 3: t0 constraint validation
# =====================================================================

def figure_t0_constraints():
    """2x2 panel: t0 recovery for KN vs SN."""
    print("Generating Figure 3: t0 constraint validation...")

    data_file = tmp_dir / 't0_validation.dat'
    if not data_file.exists():
        print("  WARNING: /tmp/t0_validation.dat not found, skipping")
        return

    kn_errors, kn_uncs, sn_errors, sn_uncs = [], [], [], []
    with open(data_file) as f:
        for line in f:
            if line.startswith('#'):
                continue
            parts = line.strip().split()
            if len(parts) == 3:
                typ, err, unc = parts[0], float(parts[1]), float(parts[2])
                if typ == 'kilonova':
                    kn_errors.append(err)
                    kn_uncs.append(unc)
                elif typ == 'supernova':
                    sn_errors.append(err)
                    sn_uncs.append(unc)

    kn_errors = np.array(kn_errors)
    sn_errors = np.array(sn_errors)
    kn_uncs = np.array(kn_uncs)
    sn_uncs = np.array(sn_uncs)

    fig, axes = plt.subplots(2, 2, figsize=(7.0, 6.0))

    # Panel 1: error histograms
    ax = axes[0, 0]
    bins = np.linspace(0, max(kn_errors.max(), sn_errors.max()), 30)
    ax.hist(kn_errors, bins=bins, alpha=0.6, color='#e74c3c',
            label=f'Kilonova ($n={len(kn_errors)}$)', histtype='stepfilled')
    ax.hist(sn_errors, bins=bins, alpha=0.6, color='#f39c12',
            label=f'Supernova ($n={len(sn_errors)}$)', histtype='stepfilled')
    kn_med = np.median(kn_errors)
    sn_med = np.median(sn_errors)
    ax.axvline(kn_med, color='#c0392b', ls='--', lw=1.5)
    ax.axvline(sn_med, color='#d68910', ls='--', lw=1.5)
    ax.set_xlabel('$|t_0^\\mathrm{true} - t_0^\\mathrm{fit}|$ (days)')
    ax.set_ylabel('Count')
    ax.set_title('$t_0$ Recovery Error')
    ax.legend(fontsize=8)

    ax.text(0.97, 0.97,
            f'KN median: {kn_med:.2f} d\n'
            f'SN median: {sn_med:.2f} d\n'
            f'KN $<$1 d: {100*np.sum(kn_errors<1)/len(kn_errors):.0f}%\n'
            f'SN $<$1 d: {100*np.sum(sn_errors<1)/len(sn_errors):.0f}%',
            transform=ax.transAxes, va='top', ha='right', fontsize=7,
            bbox=dict(boxstyle='round,pad=0.3', fc='white', ec='gray', alpha=0.9))

    # Panel 2: CDF
    ax = axes[0, 1]
    kn_sorted = np.sort(kn_errors)
    sn_sorted = np.sort(sn_errors)
    kn_cdf = np.arange(1, len(kn_sorted)+1) / len(kn_sorted)
    sn_cdf = np.arange(1, len(sn_sorted)+1) / len(sn_sorted)
    ax.plot(kn_sorted, kn_cdf, color='#e74c3c', lw=2, label='Kilonova')
    ax.plot(sn_sorted, sn_cdf, color='#f39c12', lw=2, label='Supernova')
    ax.axvline(1.0, color='gray', ls=':', lw=1.5, alpha=0.6)
    ax.axvspan(0, 1.0, alpha=0.07, color='green')
    ax.text(0.8, 0.15, 'GW correlation\nwindow',
            fontsize=7, color='green', alpha=0.8)
    ax.set_xlabel('$|t_0$ Error$|$ (days)')
    ax.set_ylabel('Cumulative Probability')
    ax.set_title('CDF of $t_0$ Error')
    ax.legend(fontsize=8)

    # Panel 3: error vs uncertainty
    ax = axes[1, 0]
    ax.scatter(kn_uncs, kn_errors, alpha=0.5, color='#e74c3c', s=25,
               edgecolors='k', linewidth=0.3, label='Kilonova', zorder=3)
    ax.scatter(sn_uncs, sn_errors, alpha=0.5, color='#f39c12', s=25,
               edgecolors='k', linewidth=0.3, label='Supernova', zorder=2)
    mx = max(np.max(kn_uncs), np.max(sn_uncs))
    ax.plot([0, mx], [0, mx], 'k--', alpha=0.3, lw=1)
    ax.set_xlabel('Fitted $t_0$ Uncertainty (days)')
    ax.set_ylabel('Actual $|t_0$ Error$|$ (days)')
    ax.set_title('Error vs Uncertainty Calibration')
    ax.legend(fontsize=8)

    # Panel 4: temporal discrimination power
    ax = axes[1, 1]
    np.random.seed(42)
    n_sim = 10000
    kn_fitted = np.random.normal(0, kn_med, n_sim)
    sn_true = np.random.uniform(0, 30, n_sim)
    sn_fitted = sn_true + np.random.normal(0, sn_med, n_sim)
    bins_t0 = np.linspace(-5, 35, 50)
    ax.hist(kn_fitted, bins=bins_t0, alpha=0.6, color='#e74c3c',
            density=True, histtype='stepfilled', label='Kilonova (prompt)')
    ax.hist(sn_fitted, bins=bins_t0, alpha=0.6, color='#f39c12',
            density=True, histtype='stepfilled', label='Supernova (random)')
    ax.axvspan(-1/86400, 1.0, alpha=0.15, color='green')
    ax.axvline(0, color='k', ls='--', lw=1, alpha=0.5)
    ax.set_xlabel('Fitted $t_0$ (days after GW)')
    ax.set_ylabel('Probability Density')
    ax.set_title('Temporal Discrimination')
    ax.legend(fontsize=8, loc='upper right')

    kn_in = np.sum((kn_fitted >= -1/86400) & (kn_fitted <= 1)) / len(kn_fitted)
    sn_in = np.sum((sn_fitted >= -1/86400) & (sn_fitted <= 1)) / len(sn_fitted)
    disc = kn_in / max(sn_in, 0.001)
    ax.text(0.97, 0.65,
            f'KN in window: {100*kn_in:.1f}%\n'
            f'SN in window: {100*sn_in:.1f}%\n'
            f'Rejection: {disc:.0f}$\\times$',
            transform=ax.transAxes, va='top', ha='right', fontsize=7,
            bbox=dict(boxstyle='round,pad=0.3', fc='lightyellow', alpha=0.9))

    fig.tight_layout()
    outfile = fig_dir / 't0_constraint_validation.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")


# =====================================================================
# Figure 4: Early rate discrimination
# =====================================================================

def figure_early_rate():
    """2x2 panel: rise/decay rate distributions and FAR multipliers."""
    print("Generating Figure 4: Early rate discrimination...")

    data_path = tmp_dir / 'early_rate_discrimination.dat'
    if not data_path.exists():
        print("  WARNING: /tmp/early_rate_discrimination.dat not found, skipping")
        return

    classes = {'KN': [], 'SNIa': [], 'SNII': []}
    with open(data_path) as f:
        for line in f:
            if line.startswith('#'):
                continue
            parts = line.strip().split()
            if len(parts) == 4 and parts[0] in classes:
                classes[parts[0]].append(
                    (float(parts[1]), float(parts[2]), float(parts[3])))
    for k in classes:
        classes[k] = np.array(classes[k])

    colors = {'KN': '#e74c3c', 'SNIa': '#3498db', 'SNII': '#2ecc71'}
    labels = {'KN': 'Kilonova', 'SNIa': 'SN Ia', 'SNII': 'SN II'}

    fig, axes = plt.subplots(2, 2, figsize=(7.0, 6.0))

    # Rise rate
    ax = axes[0, 0]
    bins = np.linspace(0, 1.5, 40)
    for k in ['KN', 'SNIa', 'SNII']:
        vals = classes[k][:, 0]
        vals = vals[np.isfinite(vals)]
        ax.hist(vals, bins=bins, alpha=0.5, label=labels[k], color=colors[k],
                density=True, histtype='stepfilled', edgecolor='k', linewidth=0.3)
    ax.axvline(0.5, color='gray', ls='--', lw=1.2, label='SN threshold')
    ax.axvline(1.0, color='gray', ls=':', lw=1.2, label='KN threshold')
    ax.set_xlabel('Rise Rate (mag/day)')
    ax.set_ylabel('Density')
    ax.set_title('Rise Rate Distributions')
    ax.legend(fontsize=7, ncol=2)

    # Decay rate
    ax = axes[0, 1]
    bins = np.linspace(0, 2.0, 40)
    for k in ['KN', 'SNIa', 'SNII']:
        vals = classes[k][:, 1]
        vals = vals[np.isfinite(vals)]
        ax.hist(vals, bins=bins, alpha=0.5, label=labels[k], color=colors[k],
                density=True, histtype='stepfilled', edgecolor='k', linewidth=0.3)
    ax.axvline(0.3, color='gray', ls='--', lw=1.2, label='KN decay threshold')
    ax.set_xlabel('Decay Rate (mag/day)')
    ax.set_ylabel('Density')
    ax.set_title('Decay Rate Distributions')
    ax.legend(fontsize=7)

    # Rise vs decay scatter
    ax = axes[1, 0]
    for k in ['SNIa', 'SNII', 'KN']:
        d = classes[k]
        mask = np.isfinite(d[:, 0]) & np.isfinite(d[:, 1])
        ax.scatter(d[mask, 0], d[mask, 1], alpha=0.25, s=10,
                   color=colors[k], label=labels[k], edgecolors='none')
    ax.axvline(0.5, color='gray', ls='--', lw=0.8, alpha=0.6)
    ax.axhline(0.3, color='gray', ls='--', lw=0.8, alpha=0.6)
    ax.set_xlabel('Rise Rate (mag/day)')
    ax.set_ylabel('Decay Rate (mag/day)')
    ax.set_title('Rise vs Decay Rate')
    ax.legend(fontsize=7)
    ax.set_xlim(0, 1.5)
    ax.set_ylim(0, 2.0)

    # FAR multiplier
    ax = axes[1, 1]
    bins = np.linspace(0, 8, 30)
    for k in ['KN', 'SNIa', 'SNII']:
        vals = classes[k][:, 2]
        vals = vals[np.isfinite(vals)]
        ax.hist(vals, bins=bins, alpha=0.5, label=labels[k], color=colors[k],
                density=True, histtype='stepfilled', edgecolor='k', linewidth=0.3)
    ax.axvline(1.0, color='k', ls='-', lw=1.5, label='Neutral')
    ax.set_xlabel('FAR Multiplier')
    ax.set_ylabel('Density')
    ax.set_title('FAR Multiplier ($>$1: penalized, $<$1: boosted)')
    ax.legend(fontsize=7)

    fig.tight_layout()
    outfile = fig_dir / 'early_rate_discrimination.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")


# =====================================================================
# Figure 5: RAVEN comparison (empirical vs analytical)
# =====================================================================

def figure_raven_comparison():
    """2x2 panel: empirical distributions vs RAVEN analytical FAR."""
    print("Generating Figure 5: RAVEN comparison...")

    TIME_WINDOW = 10.0  # seconds
    GRB_RATE = 325.0 / (365.25 * 24 * 3600)  # Hz
    GW_FAR = 1e-7  # Hz
    TEMPORAL_FAR = TIME_WINDOW * GRB_RATE * GW_FAR

    instruments = [
        ('fermi_gbm', 'Fermi-GBM ($13.2\\degree$)'),
        ('swift_bat', 'Swift-BAT (2 arcmin)'),
    ]

    fig, axes = plt.subplots(2, 2, figsize=(7.0, 6.0))

    for row, (inst, label) in enumerate(instruments):
        signal, background = load_far_data(f'far_calibration_{inst}.dat')
        if signal is None:
            continue

        sig_pos = signal[signal > 0]
        bg_pos = background[background > 0]

        # Left: spatial probability distributions
        ax = axes[row, 0]
        lo = min(sig_pos.min(), bg_pos.min())
        hi = max(sig_pos.max(), bg_pos.max())
        bins = np.logspace(np.log10(lo), np.log10(hi), 50)
        ax.hist(sig_pos, bins=bins, alpha=0.5, color='C0', density=True,
                label='Signal')
        ax.hist(bg_pos, bins=bins, alpha=0.5, color='C1', density=True,
                label='Background')
        ax.set_xscale('log')
        ax.set_xlabel('$P_\\mathrm{spatial}$')
        ax.set_ylabel('Density')
        ax.set_title(f'{label}: Spatial Probability')
        ax.legend(fontsize=8)

        disc = np.median(sig_pos) / np.median(bg_pos)
        ax.text(0.05, 0.95, f'Discrimination: {disc:.0f}$\\times$',
                transform=ax.transAxes, va='top', fontsize=8,
                bbox=dict(boxstyle='round,pad=0.3', fc='white', alpha=0.9))

        # Right: RAVEN FAR
        ax = axes[row, 1]
        raven_sig = np.array([TEMPORAL_FAR / p if p > 0 else np.inf
                              for p in sig_pos])
        raven_bg = np.array([TEMPORAL_FAR / p if p > 0 else np.inf
                             for p in bg_pos])
        raven_sig = raven_sig[np.isfinite(raven_sig)] * 365.25 * 24 * 3600
        raven_bg = raven_bg[np.isfinite(raven_bg)] * 365.25 * 24 * 3600

        lo = min(raven_sig.min(), raven_bg.min())
        hi = max(raven_sig.max(), raven_bg.max())
        bins_far = np.logspace(np.log10(lo), np.log10(hi), 50)
        ax.hist(raven_sig, bins=bins_far, alpha=0.5, color='C0', density=True,
                label='Signal')
        ax.hist(raven_bg, bins=bins_far, alpha=0.5, color='C1', density=True,
                label='Background')
        ax.axvline(1.0, color='red', ls='--', lw=1.5, alpha=0.7,
                   label='1/yr threshold')
        ax.set_xscale('log')
        ax.set_xlabel('RAVEN FAR (/yr)')
        ax.set_ylabel('Density')
        ax.set_title(f'{label}: RAVEN FAR')
        ax.legend(fontsize=7)

        sig_med_far = np.median(raven_sig)
        n_below = np.sum(raven_sig < 1.0)
        frac = 100 * n_below / len(raven_sig)
        ax.text(0.05, 0.95,
                f'Median FAR: {sig_med_far:.2e}/yr\n'
                f'FAR $<$ 1/yr: {frac:.1f}%',
                transform=ax.transAxes, va='top', fontsize=7,
                bbox=dict(boxstyle='round,pad=0.3', fc='white', alpha=0.9))

    fig.tight_layout()
    outfile = fig_dir / 'raven_comparison.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")


# =====================================================================
# Figure 6: Combined Pipeline ROC / End-to-End Efficiency
# =====================================================================

def figure_combined_pipeline():
    """3-panel figure: ROC curves for each stage, end-to-end rejection,
    and expected false associations vs. threshold."""
    print("Generating Figure 6: Combined pipeline efficiency...")

    # --- Load spatial data ---
    sig_opt, bg_opt = load_far_data('far_calibration_optical.dat')
    sig_gbm, bg_gbm = load_far_data('far_calibration_fermi_gbm.dat')
    sig_bat, bg_bat = load_far_data('far_calibration_swift_bat.dat')
    if sig_opt is None:
        return

    # --- Load t0 data if available ---
    kn_t0_err = 0.65      # KN median t0 recovery error (default)
    sn_t0_err = 0.56      # SN median t0 recovery error (default)
    t0_file = tmp_dir / 't0_validation.dat'
    if t0_file.exists():
        kn_errs, sn_errs = [], []
        with open(t0_file) as f:
            for line in f:
                if line.startswith('#'):
                    continue
                parts = line.strip().split()
                if len(parts) == 3:
                    if parts[0] == 'kilonova':
                        kn_errs.append(float(parts[1]))
                    elif parts[0] == 'supernova':
                        sn_errs.append(float(parts[1]))
        kn_t0_err = np.median(kn_errs) if kn_errs else 0.65
        sn_t0_err = np.median(sn_errs) if sn_errs else 0.56

    # --- Load early rate data if available ---
    kn_far_mult, sn_far_mult = None, None
    er_file = tmp_dir / 'early_rate_discrimination.dat'
    if er_file.exists():
        kn_m, snia_m = [], []
        with open(er_file) as f:
            for line in f:
                if line.startswith('#'):
                    continue
                parts = line.strip().split()
                if len(parts) == 4:
                    if parts[0] == 'KN':
                        kn_m.append(float(parts[3]))
                    elif parts[0] == 'SNIa':
                        snia_m.append(float(parts[3]))
        kn_far_mult = np.array(kn_m)
        sn_far_mult = np.array(snia_m)

    fig, axes = plt.subplots(1, 3, figsize=(11.0, 4.0))

    # ---- Panel 1: Spatial ROC curves for all instruments ----
    ax = axes[0]
    for label, sig, bg, color, ls in [
        ('Fermi-GBM', sig_gbm, bg_gbm, '#c0392b', '-'),
        ('Swift-BAT', sig_bat, bg_bat, '#2471a3', '--'),
        ('Optical', sig_opt, bg_opt, '#27ae60', '-.'),
    ]:
        if sig is None:
            continue
        # Proper ROC: vary threshold, compute TPR and FPR
        all_vals = np.concatenate([sig, bg])
        thresholds = np.unique(np.sort(all_vals))
        # Subsample for speed
        if len(thresholds) > 2000:
            idx = np.linspace(0, len(thresholds) - 1, 2000, dtype=int)
            thresholds = thresholds[idx]
        tpr = np.array([np.mean(sig >= t) for t in thresholds])
        fpr = np.array([np.mean(bg >= t) for t in thresholds])
        # Sort by FPR for proper curve
        order = np.argsort(fpr)
        fpr_s, tpr_s = fpr[order], tpr[order]
        # Compute AUC properly
        auc = np.trapz(tpr_s, fpr_s)
        ax.plot(fpr_s, tpr_s, color=color, ls=ls, lw=2,
                label=f'{label} (AUC={auc:.3f})')

    ax.plot([0, 1], [0, 1], 'k:', alpha=0.3, lw=1)
    ax.set_xlabel('Background Acceptance Rate')
    ax.set_ylabel('Signal Efficiency')
    ax.set_title('Spatial Discrimination ROC')
    ax.legend(fontsize=7, loc='lower right')
    ax.set_xlim(-0.02, 1.02)
    ax.set_ylim(-0.02, 1.02)

    # ---- Panel 2: End-to-end expected false associations ----
    ax = axes[1]

    # Compute expected false associations per GW event
    # Starting from N_bg = 1000 background candidates in GW error region
    N_bg = 1000

    # Spatial stage: weight each background by P_spatial / <P_spatial_signal>
    mean_sig = np.mean(sig_opt)
    mean_bg = np.mean(bg_opt)
    spatial_ratio = mean_sig / mean_bg
    n_after_spatial = N_bg / spatial_ratio

    # Temporal stage: fraction passing [0, 1] day window
    # With reliable SVI
    np.random.seed(42)
    n_mc = 50000
    kn_t0 = np.random.normal(0, kn_t0_err, n_mc)
    sn_true = np.random.uniform(0, 30, n_mc)
    sn_t0 = sn_true + np.random.normal(0, sn_t0_err, n_mc)
    kn_pass_t0 = np.mean((kn_t0 >= 0) & (kn_t0 <= 1))
    sn_pass_t0 = np.mean((sn_t0 >= 0) & (sn_t0 <= 1))
    temporal_ratio = kn_pass_t0 / max(sn_pass_t0, 1e-10)
    n_after_temporal = n_after_spatial / temporal_ratio

    # Without SVI (per-measurement fallback, ~1 day window)
    # Background SN detection within 1 day: ~2/30 chance if uniform over 30-day baseline
    sn_pass_nomatch = 2.0 / 30.0
    temporal_ratio_nosvi = kn_pass_t0 / sn_pass_nomatch
    n_after_temporal_nosvi = n_after_spatial / temporal_ratio_nosvi

    # Early rate: mean multiplier ratio
    if kn_far_mult is not None and sn_far_mult is not None:
        er_ratio = np.mean(sn_far_mult) / np.mean(kn_far_mult)
    else:
        er_ratio = 5.7 / 5.0  # from paper defaults
    n_after_er = n_after_temporal / er_ratio
    n_after_er_nosvi = n_after_temporal_nosvi / er_ratio

    stages = ['Initial\ncandidates', 'After\nspatial', 'After\ntemporal\n(SVI)',
              'After\nearly rate']
    counts_svi = [N_bg, n_after_spatial, n_after_temporal, n_after_er]
    counts_nosvi = [N_bg, n_after_spatial, n_after_temporal_nosvi, n_after_er_nosvi]

    x = np.arange(len(stages))
    width = 0.35
    bars1 = ax.bar(x - width/2, counts_svi, width, color='#2471a3', alpha=0.8,
                   label='With SVI $t_0$')
    bars2 = ax.bar(x + width/2, counts_nosvi, width, color='#f39c12', alpha=0.8,
                   label='Per-meas. fallback')
    ax.set_yscale('log')
    ax.set_ylabel('Expected False Associations')
    ax.set_title('Candidate Reduction per GW Event')
    ax.set_xticks(x)
    ax.set_xticklabels(stages, fontsize=7)
    ax.legend(fontsize=7, loc='upper right')
    ax.set_ylim(0.01, 3000)

    # Add text labels on bars
    for bar, val in zip(bars1, counts_svi):
        if val >= 0.1:
            ax.text(bar.get_x() + bar.get_width()/2, val * 1.3,
                    f'{val:.1f}', ha='center', va='bottom', fontsize=6)
    for bar, val in zip(bars2, counts_nosvi):
        if val >= 0.1:
            ax.text(bar.get_x() + bar.get_width()/2, val * 1.3,
                    f'{val:.1f}', ha='center', va='bottom', fontsize=6)

    # ---- Panel 3: Summary statistics table as text ----
    ax = axes[2]
    ax.axis('off')

    summary_text = (
        "Combined Pipeline Summary\n"
        + "\u2500" * 35 + "\n\n"
        f"Spatial discrimination:\n"
        f"  Mean $P_{{\\mathrm{{sig}}}}$ / $P_{{\\mathrm{{bg}}}}$:  "
        f"{spatial_ratio:.1f}$\\times$\n"
        f"  BG zero-prob fraction:  "
        f"{100*np.mean(bg_opt < 1e-10):.1f}%\n\n"
        f"Temporal ($t_0$ recovery):\n"
        f"  KN in [0,1]d window:   {100*kn_pass_t0:.1f}%\n"
        f"  SN in [0,1]d window:   {100*sn_pass_t0:.1f}%\n"
        f"  Discrimination:        {temporal_ratio:.0f}$\\times$\n\n"
        f"Early rate (soft scoring):\n"
    )
    if kn_far_mult is not None:
        summary_text += (
            f"  Mean KN multiplier:    {np.mean(kn_far_mult):.2f}\n"
            f"  Mean SN multiplier:    {np.mean(sn_far_mult):.2f}\n"
            f"  Net discrimination:    {er_ratio:.2f}$\\times$\n\n"
        )
    else:
        summary_text += f"  Net discrimination:    {er_ratio:.2f}$\\times$\n\n"

    summary_text += (
        f"Cumulative rejection:\n"
        f"  With SVI:   {N_bg/n_after_er:.0f}$\\times$"
        f" ({N_bg}$\\to${n_after_er:.1f})\n"
        f"  W/o SVI:    {N_bg/n_after_er_nosvi:.0f}$\\times$"
        f" ({N_bg}$\\to${n_after_er_nosvi:.1f})"
    )
    ax.text(0.05, 0.95, summary_text, transform=ax.transAxes,
            va='top', ha='left', fontsize=7.5,
            family='monospace',
            bbox=dict(boxstyle='round,pad=0.5', fc='#f8f9fa', ec='#dee2e6'))

    fig.tight_layout()
    outfile = fig_dir / 'combined_pipeline_efficiency.pdf'
    fig.savefig(outfile)
    plt.close(fig)
    print(f"  Saved: {outfile}")

    # Print detailed stats for paper
    print(f"\n  === Pipeline Statistics for Paper ===")
    print(f"  Spatial mean ratio: {spatial_ratio:.1f}x")
    print(f"  BG zero fraction: {100*np.mean(bg_opt < 1e-10):.1f}%")
    print(f"  KN t0 median error: {kn_t0_err:.3f} days")
    print(f"  SN t0 median error: {sn_t0_err:.3f} days")
    print(f"  KN pass [0,1]d: {100*kn_pass_t0:.1f}%")
    print(f"  SN pass [0,1]d: {100*sn_pass_t0:.1f}%")
    print(f"  Temporal ratio: {temporal_ratio:.1f}x")
    print(f"  Early rate ratio: {er_ratio:.2f}x")
    print(f"  Combined (with SVI): {N_bg/n_after_er:.0f}x")
    print(f"  Combined (no SVI): {N_bg/n_after_er_nosvi:.0f}x")
    print(f"  Expected false assoc (SVI): {n_after_er:.1f}")
    print(f"  Expected false assoc (no SVI): {n_after_er_nosvi:.1f}")


# =====================================================================
# Main
# =====================================================================

if __name__ == '__main__':
    print("=" * 60)
    print("ORIGIN Paper Figure Generation")
    print("=" * 60)

    figure_far_calibration_grb()
    figure_far_calibration_optical()
    figure_t0_constraints()
    figure_early_rate()
    figure_raven_comparison()
    figure_combined_pipeline()

    print("\n" + "=" * 60)
    print("All figures saved to:", fig_dir)
    print("=" * 60)
