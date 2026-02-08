#!/bin/bash
# Restart demo with slower rate for better Grafana visualization

cd "$(dirname "$0")/.."

# Stop existing processes
pkill -f stream-o4-simulation || true
pkill -f correlation-exporter || true
sleep 2

echo "🚀 Starting services with slow rate for Grafana demo..."

# Start exporter
RUST_LOG=info ./target/release/correlation-exporter > /tmp/correlation-exporter.log 2>&1 &
EXPORTER_PID=$!
echo "✅ Exporter PID: $EXPORTER_PID"

sleep 2

# Start simulator with slow rate (0.2 events/sec = 1 event every 5 seconds)
RUST_LOG=info ./target/release/stream-o4-simulation \
    /Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp \
    --rate 0.2 \
    --max-events 500 \
    --limiting-magnitude 24.5 > /tmp/stream-o4-simulation.log 2>&1 &
SIMULATOR_PID=$!
echo "✅ Simulator PID: $SIMULATOR_PID"

echo ""
echo "📊 Grafana: http://localhost:3000"
echo "   Set time range to 'Last 5 minutes' and enable auto-refresh (5s)"
echo ""
echo "🛑 Stop: kill $EXPORTER_PID $SIMULATOR_PID"
