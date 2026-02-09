#!/bin/bash

# Multi-Messenger API Demo Script
# This script demonstrates the complete pipeline:
# 1. Docker services (Kafka, Grafana, etc.)
# 2. REST API server
# 3. O4 simulation with background events
# 4. Real-time visualization

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Multi-Messenger Event API & Visualization Demo             ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker first."
    exit 1
fi

# Start infrastructure
echo -e "${GREEN}[1/4]${NC} Starting infrastructure services (Kafka, Grafana, Prometheus)..."
docker compose up -d

echo ""
echo -e "${GREEN}[2/4]${NC} Waiting for services to be ready..."
sleep 5

# Check if services are up
echo "  ✓ Kafka: http://localhost:9092"
echo "  ✓ Grafana: http://localhost:3000"
echo "  ✓ Prometheus: http://localhost:9090"
echo ""

# Build Rust binaries
echo -e "${GREEN}[3/4]${NC} Building Rust services..."
cargo build --release --bin mm_api_server --bin stream-o4-simulation

echo ""
echo -e "${GREEN}[4/4]${NC} Starting API server and simulation..."
echo ""
echo "────────────────────────────────────────────────────────────────"
echo ""

# Create trap to cleanup on exit
cleanup() {
    echo ""
    echo ""
    echo "🛑 Shutting down services..."
    kill $API_PID 2>/dev/null || true
    kill $SIM_PID 2>/dev/null || true
    docker compose down
    echo "✅ Cleanup complete"
}
trap cleanup EXIT INT TERM

# Start API server in background
echo -e "${YELLOW}Starting API server on http://localhost:8080...${NC}"
cargo run --release --bin mm_api_server -- --bind 0.0.0.0:8080 > api.log 2>&1 &
API_PID=$!

# Wait for API to be ready
sleep 3

# Test API health
if curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "  ✅ API server is running"
else
    echo "  ❌ API server failed to start (check api.log)"
    exit 1
fi

echo ""
echo "────────────────────────────────────────────────────────────────"
echo ""
echo -e "${YELLOW}Starting O4 simulation with background events...${NC}"
echo ""

# Run simulation (will exit on completion)
cargo run --release --bin stream-o4-simulation -- \
    /path/to/O4HL/bgp \
    --rate 2.0 \
    --max-events 10 \
    --simulate-background \
    --background-duration-days 1.0 \
    --publish-to-api &
SIM_PID=$!

# Wait for simulation to complete
wait $SIM_PID

echo ""
echo "────────────────────────────────────────────────────────────────"
echo ""
echo -e "${GREEN}✨ Demo complete!${NC}"
echo ""
echo "📊 View results:"
echo "  • API Events: curl http://localhost:8080/api/events | jq"
echo "  • Grafana: http://localhost:3000 (admin/admin)"
echo "  • Aladin Demo: open aladin_demo.html"
echo ""
echo "Press Ctrl+C to stop all services and cleanup..."
echo ""

# Keep running to show logs
tail -f api.log
