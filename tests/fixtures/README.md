# Test Fixtures

This directory contains sample data files for testing and development without requiring external data downloads.

## Directory Structure

```
tests/fixtures/
├── observing_scenarios/  # GW observing scenario simulation data
├── grb_xmls/             # GRB alert VOEvent XML files
└── lightcurves_csv/      # ZTF optical transient light curves
```

## Observing Scenarios

Sample data files from LIGO/Virgo observing run simulations:

- **O5a** (Observing Run 5a): Advanced LIGO, Advanced Virgo at design sensitivity
  - `coincs.dat`: Coincident multi-detector triggers
  - `allsky.dat`: All-sky search background events
  - `injections.dat`: Simulated GW signals injected for testing

- **O4HL** (Observing Run 4, High-Low sensitivity): LIGO at high sensitivity, Virgo at low sensitivity
- **O5c** (Observing Run 5c): Advanced LIGO+, Advanced Virgo+, KAGRA at target sensitivity

**Format**: Space-separated columns with GPS time, SNR, FAR, sky position, etc.

**Source**: [observing-scenarios](https://git.ligo.org/emfollow/observing-scenarios)

## GRB XMLs

VOEvent XML files for gamma-ray burst (GRB) alerts from various instruments:

### Fermi Alerts
- `fermi_grb_gcn.xml` - Fermi GBM ground-based localization
- `fermi_initial_grb_gcn.xml` - Fermi GBM initial alert
- `fermi_subthresh_grb_gcn.xml` - Fermi GBM sub-threshold alert
- `fermi_subthresh_grb_lowconfidence.xml` - Low confidence sub-threshold
- `fermi_subgrbtargeted_template.xml` - Targeted GRB search
- `GRB180116A_Fermi_GBM_Gnd_Pos.xml` - Real GRB180116A event

### Swift Alert
- `swift_grb_gcn.xml` - Swift BAT GRB detection

### Einstein Probe
- `einsteinprobe_grb_template.xml` - Einstein Probe WXT alert template

### SVOM
- `svom_grb_gcn.xml` - SVOM GRB detection
- `svom_grb_template.xml` - SVOM alert template

**Format**: VOEvent XML with position, time, error radius, significance

**Source**: [gwcelery test data](https://git.ligo.org/emfollow/gwcelery), [SkyPortal test data](https://github.com/skyportal/skyportal)

## Optical Light Curves

ZTF (Zwicky Transient Facility) light curves for 10 transient objects:

| Object ID | Measurements | Notes |
|-----------|--------------|-------|
| ZTF25aaaalin | 36 | Typical transient |
| ZTF25aaaawig | 50 | Multi-band coverage |
| ZTF25aaabezb | 38 | |
| ZTF25aaabnwi | 560 | Extensively observed |
| ZTF25aaabnxh | 42 | |
| ZTF25aaacrjj | 30 | |
| ZTF25aaadqsi | 249 | Well-sampled |
| ZTF25aaadytl | 242 | Well-sampled |
| ZTF25aaaecsu | 211 | |
| ZTF25aaaeykb | 36 | |

**Format**: CSV with columns: `objectId,jd,flux,flux_err,band`
- `jd`: Julian Date
- `flux`: Flux in µJy
- `flux_err`: 1-σ flux uncertainty
- `band`: Filter (g, r, i)

**Source**: ZTF alerts from [BOOM](https://github.com/skyportal/boom) processing

## Usage in Tests

### Rust Tests

```rust
use std::path::PathBuf;

#[test]
fn test_load_observing_scenario() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/observing_scenarios/coincs.dat");
    // Load and parse...
}
```

### Python Tests (sgn-llai compatibility)

```python
import os
from pathlib import Path

FIXTURES_DIR = Path(__file__).parent.parent / "tests" / "fixtures"

def test_grb_parsing():
    grb_xml = FIXTURES_DIR / "grb_xmls" / "fermi_grb_gcn.xml"
    # Parse and test...
```

## Regenerating Fixtures

To update fixtures with new data:

```bash
# Observing scenarios (requires observing-scenarios repo)
cp /path/to/observing-scenarios/runs/O5a/bgp/*.dat tests/fixtures/observing_scenarios/

# GRB XMLs (requires gwcelery repo)
cp /path/to/gwcelery/src/gwcelery/tests/data/*grb*.xml tests/fixtures/grb_xmls/

# Optical light curves (requires ZTF CSV directory)
ls /path/to/lightcurves_csv/*.csv | head -10 | xargs -I {} cp {} tests/fixtures/lightcurves_csv/
```

## CI/CD Usage

These fixtures enable tests to run in CI environments (GitHub Actions) without requiring:
- Large data downloads
- External API access
- Local data directories

All integration tests use relative paths to `tests/fixtures/`.
