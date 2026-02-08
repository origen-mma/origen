#!/bin/bash
# Quick start script for Grafana demo
# This starts both the simulator and exporter in background

set -e

cd "$(dirname "$0")/.."

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Starting O4 Multi-Messenger Grafana Demo                ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check if Kafka is running
if ! docker ps | grep -q mm-kafka; then
    echo "⚠️  Kafka not running. Starting..."
    docker compose up -d kafka
    echo "   Waiting for Kafka to be ready..."
    sleep 10
fi

# Build binaries if needed
if [ ! -f target/release/stream-o4-simulation ] || [ ! -f target/release/correlation-exporter ]; then
    echo "📦 Building binaries..."
    cargo build --release --bin stream-o4-simulation --bin correlation-exporter
fi

# Kill any existing processes
pkill -f stream-o4-simulation || true
pkill -f correlation-exporter || true

echo ""
echo "🚀 Starting services..."
echo ""

# Start correlation exporter in background
echo "1️⃣  Starting correlation-exporter (Kafka → Prometheus)"
RUST_LOG=info ./target/release/correlation-exporter > /tmp/correlation-exporter.log 2>&1 &
EXPORTER_PID=$!
echo "   PID: $EXPORTER_PID"
echo "   Logs: tail -f /tmp/correlation-exporter.log"
echo "   Metrics: http://localhost:9091/metrics"

# Wait for exporter to be ready
sleep 2

# Start simulation stream in background
echo ""
echo "2️⃣  Starting stream-o4-simulation (O4 events → Kafka)"
RUST_LOG=info ./target/release/stream-o4-simulation \
    /Users/mcoughlin/Code/ORIGIN/observing-scenarios/runs/O4HL/bgp \
    --rate 2.0 \
    --max-events 100 \
    --limiting-magnitude 24.5 > /tmp/stream-o4-simulation.log 2>&1 &
SIMULATOR_PID=$!
echo "   PID: $SIMULATOR_PID"
echo "   Logs: tail -f /tmp/stream-o4-simulation.log"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Services Running!                                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 Open Grafana: http://localhost:3000"
echo "   Login: admin / admin"
echo "   Dashboard: O4 Multi-Messenger Correlations"
echo ""
echo "📈 Check metrics: curl http://localhost:9091/metrics | grep mm_"
echo ""
echo "📜 Monitor logs:"
echo "   Exporter:  tail -f /tmp/correlation-exporter.log"
echo "   Simulator: tail -f /tmp/stream-o4-simulation.log"
echo ""
echo "🛑 Stop all:"
echo "   kill $EXPORTER_PID $SIMULATOR_PID"
echo ""
echo "Waiting 5 seconds for data to start flowing..."
sleep 5

# Check if metrics are being produced
echo ""
echo "✅ Checking metrics..."
METRICS=$(curl -s http://localhost:9091/metrics 2>/dev/null | grep "mm_correlations_total" || echo "")
if [ -z "$METRICS" ]; then
    echo "⚠️  No metrics yet. Waiting 10 more seconds..."
    sleep 10
    METRICS=$(curl -s http://localhost:9091/metrics 2>/dev/null | grep "mm_correlations_total" || echo "")
fi

if [ -n "$METRICS" ]; then
    echo "✅ Metrics flowing!"
    echo "$METRICS"
else
    echo "❌ No metrics yet. Check logs:"
    echo "   tail -f /tmp/correlation-exporter.log"
    echo "   tail -f /tmp/stream-o4-simulation.log"
fi

echo ""
echo "🎉 All set! Go to http://localhost:3000"
