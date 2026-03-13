# System Overview

ORIGIN is structured as a Rust workspace with 9 crates, each handling a distinct layer of the multi-messenger pipeline.

## Data Flow

```
                        Simulation Layer
 ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
 │  GW + GRB Event  │  │  O4 Observing    │  │  Optical Alert   │
 │  Generator       │  │  Scenario Sim    │  │  Streamer (ZTF)  │
 │  (stream-events) │  │  (stream-o4-sim) │  │  (stream-optical)│
 └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
          │                     │                      │
          ▼                     ▼                      ▼
 ┌────────────────────────────────────────────────────────────────┐
 │                     Kafka Message Bus                          │
 │  igwn.gwalert  │  gcn.notices.grb  │  optical.alerts          │
 └────────────────────────────┬───────────────────────────────────┘
                              │
                              ▼
 ┌────────────────────────────────────────────────────────────────┐
 │               Superevent Correlator (mm-correlator)            │
 │                                                                │
 │  Temporal matching ──► Spatial matching ──► Joint FAR (RAVEN)  │
 │         │                    │                    │             │
 │         ▼                    ▼                    ▼             │
 │  SVI light curve    Early rate filter    GP feature extraction  │
 │  fitting (t0 est)   (KN vs SN pre-cut)  (background rejection) │
 └──────────────────────────┬─────────────────────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
 ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
 │    Redis     │  │  Prometheus  │  │  REST API    │
 │  (state)     │  │  + Grafana   │  │  (mm-api)    │
 └──────────────┘  └──────────────┘  └──────────────┘
```

## Joint FAR Calculation

The RAVEN-style joint false alarm rate combines temporal, spatial, and source-count information:

\[
\text{FAR}_\text{joint} = R_\text{bg} \times \frac{1}{T_\text{window}} \times P_\text{spatial} \times N_\text{trials} \times f_\text{lc}
\]

where:

- **\\(R_\text{bg}\\)**: background optical transient rate (per day per sq deg)
- **\\(T_\text{window}\\)**: temporal coincidence window
- **\\(P_\text{spatial}\\)**: spatial coincidence probability (from skymap or point-source fallback)
- **\\(N_\text{trials}\\)**: number of monitored surveys
- **\\(f_\text{lc}\\)**: light curve penalty factor (1.0 if no LC filter, <1 if KN-like)

## Superevent Lifecycle

1. **Creation**: A GW event arrives and creates a new superevent
2. **Association**: GRB and optical events within the temporal and spatial windows are associated
3. **Classification**: Updated based on what messengers are present (GW-only, GW+GRB, GW+optical, multi-messenger)
4. **Expiration**: Superevents older than `max_superevent_age` are finalized
