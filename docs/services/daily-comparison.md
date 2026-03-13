# Daily GCN-BOOM Comparison Service

The `daily-comparison` binary is a continuous service that consumes events from both GCN Kafka and BOOM alert streams, accumulates them per UTC day, and at each day boundary produces a comprehensive comparison report.

## Purpose

Multi-messenger astronomy relies on multiple alert streams that have different coverage, latency, and reliability characteristics. This service answers:

- **Completeness**: What fraction of events from each stream were also seen by the other?
- **Latency**: Which stream reports events first, and by how much?
- **Cross-matching**: Which events are the same astrophysical source seen by both streams?
- **Correlation quality**: What does the RAVEN correlator produce when run on the day's events?

## Usage

```bash
# Continuous mode (runs indefinitely, produces daily reports at midnight UTC)
daily-comparison --config config/config.toml --boom

# Single-day mode (produce one report and exit -- useful for cron or testing)
daily-comparison --config config/config.toml --boom --single-day

# With Redis persistence
daily-comparison --config config/config.toml --boom --redis-url redis://localhost:6379

# Override thresholds via CLI (takes precedence over config file)
daily-comparison --spatial-threshold 10.0 --temporal-threshold 43200.0

# Historical replay from earliest available Kafka offset
daily-comparison --config config/config.toml --boom --from-beginning --single-day
```

## Architecture

```
  GCN Kafka                          BOOM Kafka
  (gcn.nasa.gov)                     (kaboom.caltech.edu)
       │                                   │
       ▼                                   ▼
  ┌──────────┐      mpsc channel     ┌──────────┐
  │  OAuth    │◄─────────────────────►│  SASL    │
  │  Consumer │                       │  Consumer│
  └─────┬─────┘                       └─────┬────┘
        │                                   │
        ▼                                   ▼
  ┌─────────────────────────────────────────────┐
  │              DayAccumulator                  │
  │                                              │
  │  gcn_inventory: Vec<InventoryEntry>          │
  │  boom_inventory: Vec<InventoryEntry>         │
  │  gcn_events: Vec<Event>      (RAVEN replay)  │
  │  boom_lightcurves: Vec<(LC, Pos, ID)>        │
  └─────────────────────┬───────────────────────┘
                        │
                   midnight UTC
                        │
                        ▼
  ┌─────────────────────────────────────────────┐
  │              Report Generation               │
  │                                              │
  │  1. cross_match_events()  -- spatial+temporal│
  │  2. compute_completeness() -- per source     │
  │  3. compute_latency_stats() -- who was first │
  │  4. run_daily_correlation() -- RAVEN batch   │
  └─────────────────────┬───────────────────────┘
                        │
              ┌─────────┼─────────┐
              ▼                   ▼
        JSON file            Redis store
   daily_report_YYYY-MM-DD   daily_report:{date}
```

## Cross-Matching Algorithm

Events are matched using greedy nearest-neighbor assignment:

1. For each (GCN, BOOM) pair, compute angular separation and time difference
2. Filter by spatial threshold (default 5 deg) and temporal threshold (default 86400s)
3. Sort candidate pairs by angular separation (smallest first)
4. Greedily assign each BOOM event to its closest unmatched GCN event

## Output Format

Reports are written as JSON files to the output directory:

```json
{
  "date": "2026-03-08",
  "generated_at": "2026-03-09T00:00:05Z",
  "uptime_s": 86395.0,
  "gcn_event_count": 12,
  "boom_event_count": 4500,
  "total_cross_matches": 3,
  "gcn_completeness": {
    "total_events": 12,
    "matched_events": 3,
    "completeness_fraction": 0.25
  },
  "boom_completeness": {
    "total_events": 4500,
    "matched_events": 3,
    "completeness_fraction": 0.00067
  },
  "gcn_first_count": 2,
  "boom_first_count": 1,
  "correlator_summary": {
    "total_superevents": 5,
    "with_grb": 2,
    "with_optical": 1,
    "multi_messenger": 0,
    "significant_candidates": []
  }
}
```

## Redis Schema

| Key | Type | TTL | Content |
|---|---|---|---|
| `daily_report:{YYYY-MM-DD}` | String | 7 days | Full `DailyReport` JSON |
| `daily_reports` | Sorted set | none | Index of report dates (score = Unix timestamp) |
