# Analysis Scripts

Python scripts for analyzing GRB error radii and plotting FAR calibration results.

## Scripts

### `analyze_grb_error_radii.py`

Analyzes the distribution of real GRB error radii from VOEvent XML files.

**Input**: VOEvent XML files from `/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices/notices/*.xml`

**Output**:
- `/tmp/grb_error_radius_distribution.png` - Histogram and CDF plots
- `/tmp/grb_error_radius_summary.txt` - Statistical summary

**Usage**:
```bash
python3 scripts/analysis/analyze_grb_error_radii.py
```

**Key findings**:
- Analyzed 5,832 Fermi-GBM GRB detections
- Median error radius: 13.21° (NOT the commonly cited ~5°)
- Distribution is log-normal with long tail to >100°

### `plot_instrument_comparison.py`

Creates comparison plots of Fermi-GBM vs Swift-BAT spatial probability distributions from FAR calibration.

**Input**: Distribution data files from `data/far_calibration/`
- `far_calibration_fermi_gbm.dat`
- `far_calibration_swift_bat.dat`

**Output**:
- `assets/far_calibration_instrument_comparison.png` - 2×2 comparison grid

**Usage**:
```bash
# First, generate distribution data by running the Rust test:
cargo test -p mm-correlator test_o4_population_far_calibration -- --ignored --nocapture

# Then create the plot:
python3 scripts/analysis/plot_instrument_comparison.py
```

**Note**: The script expects distribution files at `/tmp/far_calibration_*.dat`. Update paths in the script if needed to use files from `data/far_calibration/`.

## Requirements

```bash
pip install numpy matplotlib
```

## Reproducibility

All plots in the main README can be reproduced using these scripts:

1. **GRB Error Radius Distribution** (README line 120):
   ```bash
   python3 scripts/analysis/analyze_grb_error_radii.py
   cp /tmp/grb_error_radius_distribution.png assets/
   ```

2. **FAR Calibration Instrument Comparison** (README line 162):
   ```bash
   # Generate data
   cargo test -p mm-correlator test_o4_population_far_calibration -- --ignored --nocapture

   # Create plot
   python3 scripts/analysis/plot_instrument_comparison.py
   ```

## Data Sources

- **GRB VOEvents**: `/Users/mcoughlin/Code/ORIGIN/growth-too-marshal-gcn-notices` - Real Fermi-GBM and Swift-BAT detections
- **O4 Skymaps**: `/Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp/` - Simulated GW event skymaps
