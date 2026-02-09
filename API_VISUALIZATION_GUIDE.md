# Multi-Messenger API & Visualization Guide

This guide explains how to run the multi-messenger API server and visualize events with skymaps.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  stream_o4_simulation                                            │
│  ├─ Generates GW events + GRBs + Optical transients             │
│  ├─ Publishes to Kafka                                          │
│  └─ Posts events to REST API                                    │
└─────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│  mm-api REST Server (port 8080)                                 │
│  ├─ GET /api/events              - List all events              │
│  ├─ GET /api/events/{id}         - Get specific event           │
│  ├─ GET /api/skymaps/{id}        - Download FITS skymap         │
│  └─ GET /api/skymaps/{id}/moc    - Get MOC format               │
└─────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│  Visualization Layer                                            │
│  ├─ Grafana Dashboard (port 3000)                               │
│  ├─ Aladin Lite v3 (interactive sky viewer)                     │
│  └─ Prometheus metrics (port 9090)                              │
└─────────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Start Infrastructure Services

```bash
# Start Kafka, Zookeeper, Grafana, Prometheus, Redis
docker compose up -d

# Verify services are running
docker compose ps
```

### 2. Start the API Server

```bash
# In one terminal
cargo run --bin mm_api_server --release -- --bind 0.0.0.0:8080

# Test the API
curl http://localhost:8080/health
# Expected: {"status":"ok","service":"mm-api"}
```

### 3. Run the O4 Simulation with Background

```bash
# In another terminal
cargo run --bin stream_o4_simulation --release -- \
  --simulate-background \
  --background-duration-days 1.0

# This will:
# - Generate GW events, GRBs, optical transients
# - Publish to Kafka topic: multi-messenger-events
# - Post events to API at http://localhost:8080
# - Show real-time rejection statistics
```

### 4. Access Visualizations

- **Grafana Dashboard**: http://localhost:3000 (admin/admin)
- **Prometheus Metrics**: http://localhost:9090
- **API Events**: http://localhost:8080/api/events

## API Endpoints

### Get All Events

```bash
curl http://localhost:8080/api/events | jq
```

Response:
```json
[
  {
    "event_id": "G123456",
    "gpstime": 1234567890.5,
    "ra": 123.45,
    "dec": -23.45,
    "skymap_url": "/api/skymaps/G123456",
    "snr": 12.5,
    "far": 1e-8,
    "grb_detections": [],
    "optical_detections": []
  }
]
```

### Get Specific Event

```bash
curl http://localhost:8080/api/events/G123456 | jq
```

### Download Skymap FITS File

```bash
curl -O http://localhost:8080/api/skymaps/G123456
# Downloads G123456.fits
```

### Get Skymap in MOC Format (for faster loading)

```bash
curl http://localhost:8080/api/skymaps/G123456/moc | jq
```

## Visualizing Skymaps with Aladin Lite

You can visualize skymaps directly in your browser using Aladin Lite v3:

```html
<!DOCTYPE html>
<html>
<head>
    <title>Multi-Messenger Skymap Viewer</title>
    <script src="https://aladin.cds.unistra.fr/AladinLite/api/v3/latest/aladin.js"></script>
    <style>
        #aladin-lite-div { width: 800px; height: 600px; }
    </style>
</head>
<body>
    <h1>Multi-Messenger Event Skymap</h1>
    <div id="aladin-lite-div"></div>

    <script>
        // Initialize Aladin
        let aladin = A.aladin('#aladin-lite-div', {
            survey: 'P/DSS2/color',
            fov: 180,
            projection: 'AIT'
        });

        // Fetch events from API
        fetch('http://localhost:8080/api/events')
            .then(response => response.json())
            .then(events => {
                if (events.length > 0) {
                    const event = events[0];  // Show first event

                    // Download and display skymap
                    const skymapUrl = `http://localhost:8080${event.skymap_url}`;
                    aladin.displayFITS(skymapUrl);

                    // Add marker at event position
                    let catalog = A.catalog({name: 'GW Event'});
                    aladin.addCatalog(catalog);
                    catalog.addSources([
                        A.marker(event.ra, event.dec, {
                            popupTitle: event.event_id,
                            popupDesc: `SNR: ${event.snr}<br>FAR: ${event.far}`
                        })
                    ]);
                }
            });
    </script>
</body>
</html>
```

## Grafana Dashboard

The Grafana dashboard shows:

1. **Event Timeline** - GW events, GRBs, and optical transients over time
2. **Skymap Viewer** - Interactive Aladin panel showing GW localization
3. **Background Rejection Stats** - Temporal and spatial coincidence rates
4. **Correlation Efficiency** - Real-time correlation metrics

Access at: http://localhost:3000

## Development Workflow

### Adding a New Event to the API

```rust
use mm_api::client::ApiClient;

#[tokio::main]
async fn main() {
    let client = ApiClient::new("http://localhost:8080");

    // Publish GW event
    client.publish_gw_event(
        "G654321",           // event_id
        1234567890.5,        // gpstime
        180.0,               // ra
        -45.0,               // dec
        15.3,                // snr
        1e-10,               // far
        Some(skymap_data),   // FITS data
    ).await.unwrap();

    // Add GRB detection
    client.add_grb_detection(
        "G654321",
        1234567891.2,        // detection_time (1.7s after GW)
        179.8,               // ra
        -44.9,               // dec
        "Swift/BAT",         // instrument
        5.2e-7,              // fluence
    ).await.unwrap();
}
```

## Troubleshooting

### API Server Won't Start

```bash
# Check if port 8080 is already in use
lsof -i :8080

# Use a different port
cargo run --bin mm_api_server -- --bind 0.0.0.0:9090
```

### Grafana Can't Connect to API

1. Check API is running: `curl http://localhost:8080/health`
2. Check CORS is enabled (should allow all origins by default)
3. Update Grafana datasource URL to match API bind address

### No Events Showing

1. Verify stream_o4_simulation is running
2. Check API logs for errors
3. Query API directly: `curl http://localhost:8080/api/events`

## Performance Notes

- **FITS Format**: Full precision but ~100-500 KB per skymap
- **MOC Format**: Faster loading, ~10-50 KB per skymap (TODO: implement conversion)
- **API Caching**: Events stored in-memory (no persistence yet)
- **Concurrent Requests**: Actix-web handles 10 workers by default

## Next Steps

- [ ] Implement FITS → MOC conversion for faster skymap loading
- [ ] Add persistence layer (SQLite or PostgreSQL)
- [ ] Create custom Grafana panel plugin for Aladin integration
- [ ] Add WebSocket support for real-time event streaming
- [ ] Implement authentication (OAuth2/JWT)
