# ORIGIN

**ORIGIN** is a multi-messenger superevent simulation and correlation framework written in Rust. It generates synthetic gravitational wave (GW), gamma-ray burst (GRB), and optical transient events, streams them through a Kafka-based pipeline, and correlates them in real-time using the RAVEN algorithm to form multi-messenger superevents.

The framework validates the end-to-end pipeline that processes real alerts from LIGO/Virgo/KAGRA, Fermi/Swift, and optical surveys (ZTF, LSST) during observing runs.

## Key Capabilities

- **Real-time correlation** -- Matches GW triggers with GRB and optical counterparts using spatial-temporal coincidence and joint false alarm rate (FAR)
- **Kilonova light curve synthesis** -- MetzgerKN forward model with survey-realistic noise and cadence
- **Background simulation** -- Realistic rates of unassociated optical transients (supernovae, shock cooling) for false-positive calibration
- **FAR tuning** -- Monte Carlo injection campaigns to optimize detection thresholds via ROC analysis
- **Live stream comparison** -- Daily GCN vs BOOM cross-matching for completeness and latency monitoring
- **Infrastructure** -- Kafka streaming, Redis persistence, Prometheus metrics, Grafana dashboards

## Quick Start

```bash
git clone https://github.com/origen-mma/origen.git
cd origen
cargo build --release

# Start infrastructure
docker compose up -d

# Run a quick FAR calibration campaign
./target/release/far-tuning-campaign -n 100 --survey ztf -o results.json

# Plot the results
python scripts/analysis/plot_far_campaign.py results.json
```

## Light Curve Classification Examples

ORIGIN classifies optical transients using multiple forward models. Here are examples from ZTF alerts processed by the pipeline:

### Kilonova Candidate (MetzgerKN Model)

![MetzgerKN model fit](plots/ZTF25aaabnwi_MetzgerKN_MetzgerKN_model_example.png)

A kilonova candidate identified by the MetzgerKN forward model. The rapid rise and red color evolution are characteristic of r-process powered transients.

![Kilonova classification](plots/ZTF25aaabnwi_MetzgerKN_Kilonova_candidate.png)

### Supernova (Bazin Model)

![Bazin model fit](plots/ZTF25aaaalin_Bazin_Bazin_model_example.png)

A Type Ia supernova fit with the Bazin phenomenological model, showing the characteristic weeks-long rise and decline.

![Supernova classification](plots/ZTF25aaaalin_Bazin_Supernova-like.png)

### Fast Transient (Power Law Model)

![Power law model fit](plots/ZTF25aaaawig_PowerLaw_PowerLaw_model_example.png)

A fast-evolving transient fit with a power-law decay model.

![Fast transient classification](plots/ZTF25aaaawig_PowerLaw_Fast_transient.png)

### Synthetic Kilonova Validation

![Synthetic KN validation](plots/synthetic_kilonova_validation.png)

Validation of the MetzgerKN model against synthetic kilonova light curves, demonstrating accurate recovery of injected parameters.
