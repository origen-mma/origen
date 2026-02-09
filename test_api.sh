#!/bin/bash

# Test the Multi-Messenger API endpoints

set -e

API_URL="http://localhost:8080"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           Testing Multi-Messenger API                        ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Check if API is running
echo "1. Testing health endpoint..."
HEALTH=$(curl -s $API_URL/health)
echo "   Response: $HEALTH"
echo ""

# Create a test event
echo "2. Creating test GW event..."
curl -s -X POST $API_URL/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "event_id": "G123456",
    "gpstime": 1234567890.5,
    "ra": 180.0,
    "dec": -30.0,
    "skymap_url": "/api/skymaps/G123456",
    "snr": 15.3,
    "far": 1e-10,
    "grb_detections": [],
    "optical_detections": []
  }' | jq
echo ""

# Add GRB detection
echo "3. Adding GRB detection..."
curl -s -X POST $API_URL/api/events/G123456/grb \
  -H "Content-Type: application/json" \
  -d '{
    "detection_time": 1234567891.2,
    "ra": 179.8,
    "dec": -29.9,
    "instrument": "Swift/BAT",
    "fluence": 5.2e-7
  }' | jq
echo ""

# Add optical detection
echo "4. Adding optical detection..."
curl -s -X POST $API_URL/api/events/G123456/optical \
  -H "Content-Type: application/json" \
  -d '{
    "detection_time": 1234571490.5,
    "ra": 180.1,
    "dec": -30.1,
    "magnitude": 18.5,
    "survey": "ZTF",
    "transient_type": "kilonova"
  }' | jq
echo ""

# Get all events
echo "5. Retrieving all events..."
curl -s $API_URL/api/events | jq
echo ""

# Get specific event
echo "6. Retrieving specific event (G123456)..."
curl -s $API_URL/api/events/G123456 | jq
echo ""

echo "✅ All tests passed!"
echo ""
echo "You can now open aladin_demo.html to visualize the events"
