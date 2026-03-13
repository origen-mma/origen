#!/usr/bin/env python3
"""
Visualize results from the FAR tuning injection campaign.

Usage:
    # First run a campaign:
    far-tuning-campaign -n 500 --survey ztf -o results.json

    # Then plot:
    python scripts/analysis/plot_far_campaign.py results.json

    # Or with custom output:
    python scripts/analysis/plot_far_campaign.py results.json -o my_plots.png
"""

import argparse
import json
import sys
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from matplotlib import rcParams

# Publication-quality defaults
rcParams["font.size"] = 11
rcParams["font.family"] = "serif"
rcParams["axes.labelsize"] = 13
rcParams["axes.titlesize"] = 14
rcParams["legend.fontsize"] = 10
rcParams["figure.dpi"] = 150


def load_results(path):
    with open(path) as f:
        return json.load(f)


def plot_roc_curve(ax, results):
    """Panel 1: ROC curve — detection efficiency vs false positive rate."""
    roc = results["roc_curve"]
    if not roc:
        ax.text(0.5, 0.5, "No ROC data", ha="center", va="center", transform=ax.transAxes)
        return

    eff = [r["efficiency"] for r in roc]
    fpr = [r["false_positive_rate"] for r in roc]
    thresh = [r["far_threshold"] for r in roc]

    ax.plot(fpr, eff, "o-", color="#2a9d8f", linewidth=2, markersize=4)

    # Annotate a few FAR threshold values
    for i in range(0, len(roc), max(1, len(roc) // 5)):
        ax.annotate(
            f"{thresh[i]:.1e}",
            (fpr[i], eff[i]),
            textcoords="offset points",
            xytext=(8, -5),
            fontsize=7,
            color="gray",
        )

    ax.set_xlabel("False Positive Rate")
    ax.set_ylabel("Detection Efficiency")
    ax.set_title("ROC Curve")
    ax.set_xlim(-0.02, 1.02)
    ax.set_ylim(-0.02, 1.02)
    ax.plot([0, 1], [0, 1], "--", color="lightgray", linewidth=1)
    ax.grid(True, alpha=0.3)


def plot_efficiency_vs_distance(ax, results):
    """Panel 2: Detection efficiency in distance bins."""
    evd = results["efficiency_vs_distance"]
    if not evd:
        ax.text(0.5, 0.5, "No data", ha="center", va="center", transform=ax.transAxes)
        return

    d_max = [e[0] for e in evd]
    eff = [e[1] for e in evd]

    ax.bar(
        range(len(d_max)),
        [e * 100 for e in eff],
        color="#e76f51",
        alpha=0.8,
        edgecolor="#c0392b",
    )
    ax.set_xticks(range(len(d_max)))
    ax.set_xticklabels([f"≤{d:.0f}" for d in d_max])
    ax.set_xlabel("Distance (Mpc)")
    ax.set_ylabel("Detection Efficiency (%)")
    ax.set_title("Efficiency vs Distance")
    ax.set_ylim(0, 105)
    ax.grid(True, alpha=0.3, axis="y")


def plot_distance_distribution(ax, results):
    """Panel 3: Distribution of injection distances with detectability."""
    outcomes = results["injection_outcomes"]
    if not outcomes:
        return

    d_all = [o["distance_mpc"] for o in outcomes]
    d_det = [o["distance_mpc"] for o in outcomes if o["detectable"]]
    d_rec = [o["distance_mpc"] for o in outcomes if o["recovered"]]

    bins = np.linspace(0, max(d_all) * 1.05, 25)

    ax.hist(d_all, bins=bins, alpha=0.3, color="gray", label="All injections")
    ax.hist(d_det, bins=bins, alpha=0.5, color="#2a9d8f", label="Detectable")
    if d_rec:
        ax.hist(d_rec, bins=bins, alpha=0.7, color="#e76f51", label="Recovered")

    ax.set_xlabel("Distance (Mpc)")
    ax.set_ylabel("Count")
    ax.set_title("Injection Distance Distribution")
    ax.legend()
    ax.grid(True, alpha=0.3)


def plot_magnitude_distribution(ax, results):
    """Panel 4: Apparent peak magnitude distribution."""
    outcomes = results["injection_outcomes"]
    if not outcomes:
        return

    mag_all = [o["apparent_peak_mag"] for o in outcomes]
    mag_det = [o["apparent_peak_mag"] for o in outcomes if o["detectable"]]
    mag_rec = [o["apparent_peak_mag"] for o in outcomes if o["recovered"]]

    bins = np.linspace(min(mag_all) - 0.5, max(mag_all) + 0.5, 30)

    ax.hist(mag_all, bins=bins, alpha=0.3, color="gray", label="All injections")
    ax.hist(mag_det, bins=bins, alpha=0.5, color="#2a9d8f", label="Detectable")
    if mag_rec:
        ax.hist(mag_rec, bins=bins, alpha=0.7, color="#e76f51", label="Recovered")

    # Survey limiting magnitude line
    # Infer from the survey name if available
    ax.axvline(20.5, color="black", linestyle="--", linewidth=1, alpha=0.5, label="ZTF lim (20.5)")

    ax.set_xlabel("Apparent Peak Magnitude")
    ax.set_ylabel("Count")
    ax.set_title("Peak Magnitude Distribution")
    ax.legend(fontsize=8)
    ax.grid(True, alpha=0.3)
    ax.invert_xaxis()  # brighter = left


def plot_far_distribution(ax, results):
    """Panel 5: Joint FAR distribution for recovered signals vs false positives."""
    outcomes = results["injection_outcomes"]
    bg_outcomes = results["background_outcomes"]

    signal_fars = [o["joint_far"] for o in outcomes if o["joint_far"] is not None]
    bg_fars = [o["joint_far"] for o in bg_outcomes if o["joint_far"] is not None]

    if not signal_fars and not bg_fars:
        ax.text(
            0.5, 0.5, "No FAR data\n(no matches found)",
            ha="center", va="center", transform=ax.transAxes, fontsize=11,
        )
        ax.set_title("Joint FAR Distribution")
        return

    bins = np.logspace(-8, 2, 40)

    if signal_fars:
        ax.hist(signal_fars, bins=bins, alpha=0.7, color="#2a9d8f", label=f"Signal (N={len(signal_fars)})")
    if bg_fars:
        ax.hist(bg_fars, bins=bins, alpha=0.5, color="#e76f51", label=f"Background (N={len(bg_fars)})")

    ax.set_xscale("log")
    ax.set_xlabel("Joint FAR (yr⁻¹)")
    ax.set_ylabel("Count")
    ax.set_title("Joint FAR Distribution")
    ax.legend()
    ax.grid(True, alpha=0.3, which="both")


def plot_ejecta_scatter(ax, results):
    """Panel 6: Ejecta mass vs distance, colored by detectability."""
    outcomes = results["injection_outcomes"]
    if not outcomes:
        return

    d_nd = [o["distance_mpc"] for o in outcomes if not o["detectable"]]
    m_nd = [o["mej_total"] for o in outcomes if not o["detectable"]]
    d_det = [o["distance_mpc"] for o in outcomes if o["detectable"] and not o["recovered"]]
    m_det = [o["mej_total"] for o in outcomes if o["detectable"] and not o["recovered"]]
    d_rec = [o["distance_mpc"] for o in outcomes if o["recovered"]]
    m_rec = [o["mej_total"] for o in outcomes if o["recovered"]]

    ax.scatter(d_nd, m_nd, alpha=0.3, s=10, color="gray", label="Not detectable")
    ax.scatter(d_det, m_det, alpha=0.5, s=15, color="#2a9d8f", label="Detectable")
    if d_rec:
        ax.scatter(d_rec, m_rec, alpha=0.8, s=25, color="#e76f51", marker="*", label="Recovered")

    ax.set_xlabel("Distance (Mpc)")
    ax.set_ylabel("Ejecta Mass (M☉)")
    ax.set_yscale("log")
    ax.set_title("Ejecta Mass vs Distance")
    ax.legend(fontsize=8, loc="upper right")
    ax.grid(True, alpha=0.3)


def main():
    parser = argparse.ArgumentParser(description="Plot FAR tuning campaign results")
    parser.add_argument("input", help="Path to campaign results JSON file")
    parser.add_argument("-o", "--output", help="Output plot file (default: <input>_analysis.png)")
    args = parser.parse_args()

    results = load_results(args.input)
    output = args.output or str(Path(args.input).with_suffix("")) + "_analysis.png"

    n_inj = results["n_injections"]
    n_det = results["n_detectable"]
    n_rec = results["n_recovered"]
    n_bg = results["n_background_tested"]
    n_fp = results["n_background_false"]

    print(f"Campaign: {n_inj} injections")
    print(f"  Detectable: {n_det} ({100*n_det/max(n_inj,1):.1f}%)")
    print(f"  Recovered:  {n_rec} ({100*n_rec/max(n_det,1):.1f}% of detectable)")
    print(f"  Background: {n_bg} tested, {n_fp} false positives")

    fig, axes = plt.subplots(2, 3, figsize=(16, 10))
    fig.suptitle(
        f"FAR Tuning Campaign — {n_inj} injections, "
        f"{n_det} detectable ({100*n_det/max(n_inj,1):.0f}%), "
        f"{n_rec} recovered",
        fontsize=15,
        fontweight="bold",
    )

    plot_roc_curve(axes[0, 0], results)
    plot_efficiency_vs_distance(axes[0, 1], results)
    plot_distance_distribution(axes[0, 2], results)
    plot_magnitude_distribution(axes[1, 0], results)
    plot_far_distribution(axes[1, 1], results)
    plot_ejecta_scatter(axes[1, 2], results)

    plt.tight_layout()
    plt.savefig(output, dpi=300, bbox_inches="tight")
    print(f"\nPlot saved to: {output}")


if __name__ == "__main__":
    main()
