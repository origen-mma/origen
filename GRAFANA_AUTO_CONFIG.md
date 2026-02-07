# Grafana Auto-Configuration Guide

Grafana is now **automatically configured** with Prometheus and dashboards! No manual setup required.

## What's Configured

### ✅ Prometheus Data Source
- **Auto-configured** as the default data source
- **URL**: http://mm-prometheus:9090
- **Scrape interval**: 5 seconds
- **Editable**: Yes (you can modify settings in the UI)

### ✅ Pre-loaded Dashboard
- **Name**: Multi-Messenger Correlations
- **Location**: Multi-Messenger folder
- **Panels**:
  1. **Total Correlations** (Gauge) - Current correlation count
  2. **Correlation Rate** (Time series) - Correlations per minute
  3. **Event Counters** (Stats) - GW, GRB, and Optical event counts
  4. **Skymap Overlap** (Time series) - Overlap area statistics
  5. **Event Rate** (Bars) - 5-minute event rate breakdown
  6. **Recent Correlations** (Table) - Time offsets for recent matches

## Accessing Grafana

1. **Open**: http://localhost:3000
2. **Login**:
   - Username: `admin`
   - Password: `admin`
3. **Dashboard**: Opens automatically to "Multi-Messenger Correlations"

## File Structure

```
grafana/
├── provisioning/
│   ├── datasources/
│   │   └── prometheus.yml          # Prometheus config
│   └── dashboards/
│       └── dashboards.yml          # Dashboard provider config
└── dashboards/
    └── multi-messenger-correlations.json  # Main dashboard
```

## Metrics Available

The dashboard expects these Prometheus metrics from your correlator:

### Core Metrics
- `mm_correlations_total` - Total correlations detected
- `mm_gw_events_total` - Total GW events processed
- `mm_grb_events_total` - Total GRB events processed
- `mm_optical_alerts_total` - Total optical alerts processed

### Overlap Metrics
- `mm_overlap_area_sq_deg` - Skymap overlap area (square degrees)
- `mm_gw_90cr_area_sq_deg` - GW 90% credible region area
- `mm_grb_90cr_area_sq_deg` - GRB 90% credible region area

### Time Offset Metrics
- `mm_correlation_time_offset_seconds` - Time difference between correlated events

## Customizing Dashboards

### Edit Existing Dashboard
1. Open the dashboard
2. Click the ⚙️ (Settings) icon in the top right
3. Edit panels, add new ones, or modify queries
4. Click "Save dashboard"

### Add New Dashboard
1. Click the **+** icon in the left sidebar
2. Select "Dashboard"
3. Add panels with queries like:
   ```promql
   rate(mm_correlations_total[5m])
   ```
4. Save your dashboard

### Export Dashboard
To save your customized dashboard to version control:
1. Click ⚙️ → "JSON Model"
2. Copy the JSON
3. Save to `grafana/dashboards/my-dashboard.json`
4. Restart Grafana: `docker compose restart grafana`

## Troubleshooting

### Dashboard not loading?
```bash
# Check Grafana logs
docker logs mm-grafana

# Verify files are mounted
docker exec mm-grafana ls /etc/grafana/provisioning/datasources
docker exec mm-grafana ls /etc/grafana/dashboards
```

### Prometheus not connected?
1. Go to Configuration → Data Sources
2. Click "Prometheus"
3. Scroll to bottom and click "Test"
4. Should show "Data source is working"

### No data showing?
- Make sure the correlator service is running
- Check Prometheus is scraping: http://localhost:9090/targets
- Verify metrics are being exported: http://localhost:9091/metrics

## Quick Test

After starting services, you should see:

1. **Prometheus**: http://localhost:9090 - Shows targets as "UP"
2. **Grafana**: http://localhost:3000 - Dashboard with live data
3. **Metrics**: http://localhost:9091/metrics - Raw Prometheus metrics

## Next Steps

- **Add Alerts**: Configure Grafana alerts for anomalies
- **Create Panels**: Add panels for specific analysis
- **Export Dashboard**: Save your customizations to Git

---

**Configuration is persistent!** Even if you restart Docker containers, Grafana will remember:
- ✅ Prometheus connection
- ✅ Dashboard configuration
- ✅ User preferences

No more manual setup! 🎉
