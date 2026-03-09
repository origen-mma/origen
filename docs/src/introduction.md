# ORIGIN

**ORIGIN** is a multi-messenger superevent simulation and correlation framework written in Rust. It generates synthetic gravitational wave (GW), gamma-ray burst (GRB), and optical transient events, streams them through a Kafka-based pipeline, and correlates them in real-time using the RAVEN algorithm to form multi-messenger superevents.

The framework is designed to validate the end-to-end pipeline that processes real alerts from LIGO/Virgo/KAGRA, Fermi/Swift, and optical surveys (ZTF, LSST) during observing runs.

## Key Capabilities

- **Real-time correlation**: Matches GW triggers with GRB and optical counterparts using spatial-temporal coincidence and joint false alarm rate (FAR)
- **Kilonova light curve synthesis**: MetzgerKN forward model with survey-realistic noise and cadence
- **Background simulation**: Realistic rates of unassociated optical transients (supernovae, shock cooling) for false-positive calibration
- **FAR tuning**: Monte Carlo injection campaigns to optimize detection thresholds via ROC analysis
- **Live stream comparison**: Daily GCN vs BOOM cross-matching for completeness and latency monitoring
- **Infrastructure**: Kafka streaming, Redis persistence, Prometheus metrics, Grafana dashboards

## Quick Start

```bash
git clone https://github.com/mcoughlin/origin.git
cd origin
cargo build --release

# Start infrastructure
docker compose up -d

# Run a quick FAR calibration campaign
./target/release/far-tuning-campaign -n 100 --survey ztf -o results.json

# Plot the results
python scripts/analysis/plot_far_campaign.py results.json
```
