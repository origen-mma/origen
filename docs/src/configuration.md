# Configuration

ORIGIN uses a TOML configuration file. Copy the example to get started:

```bash
cp config/config.example.toml config/config.toml
```

## Sections

### `[gcn]` — GCN Kafka

```toml
[gcn]
client_id = "your-gcn-client-id"
client_secret = "your-gcn-client-secret"
topics = [
    "igwn.gwalert",
    "gcn.notices.swift.bat.guano",
    "gcn.notices.einstein_probe.wxt.alert",
    "gcn.notices.icecube.lvk_nu_track_search",
]
```

Obtain credentials from [gcn.nasa.gov](https://gcn.nasa.gov/).

### `[boom]` — BOOM/ZTF Alerts

```toml
[boom]
bootstrap_servers = "kaboom.caltech.edu:9093"
sasl_username = "your-username"
sasl_password = "your-password"
group_id = "origin"
topics = ["babamul.ztf.no-lsst-match.stellar"]
```

### `[correlator]` — RAVEN Correlator

```toml
[correlator]
time_window_before = -1.0        # seconds before GW trigger
time_window_after = 86400.0      # seconds after GW trigger (1 day)
spatial_threshold = 5.0          # degrees
far_threshold = 0.0333           # 1/30 yr⁻¹ (≈ monthly significance)
background_rate = 1.0            # optical transients per day per sq deg
trials_factor = 7.0              # number of surveys monitored
max_superevent_age = 7200.0      # seconds before superevent expires
```

### `[daily_comparison]` — Daily Comparison Service (optional)

```toml
[daily_comparison]
output_dir = "./data/daily_reports"
spatial_threshold = 5.0     # cross-match threshold (degrees)
temporal_threshold = 86400.0  # cross-match threshold (seconds)
redis_url = "redis://localhost:6379"  # optional
```

### `[simulation]` — Simulation Settings

```toml
[simulation]
enabled = true
ztf_csv_dir = "tests/fixtures/lightcurves_csv"
delay_ms = 0
skymap_storage_dir = "./data/skymaps"
```

## Environment Variables

| Variable | Purpose |
|---|---|
| `MM_CONFIG_PATH` | Override config file path |
| `GCN_CLIENT_ID` | Override GCN client ID |
| `GCN_CLIENT_SECRET` | Override GCN client secret |
| `REDIS_URL` | Redis connection URL |
