#!/bin/bash
# Keep-Alive Script for Render Deployment (Bash version)
# This script uses curl to ping the health endpoint periodically

URL="${1:-https://localhost:8080}"
INTERVAL="${2:-14}"  # minutes, default 14 (under Render's 15 min spin-down)

if [ -z "$1" ]; then
    echo "Usage: ./keep_alive.sh <url> [interval_minutes]"
    echo "Example: ./keep_alive.sh https://my-bot.onrender.com"
    echo "Example: ./keep_alive.sh https://my-bot.onrender.com 10"
    exit 1
fi

HEALTH_ENDPOINT="${URL%/}/health"
INTERVAL_SECONDS=$((INTERVAL * 60))

echo "Starting keep-alive pings to $HEALTH_ENDPOINT"
echo "Ping interval: $INTERVAL minutes"

while true; do
    RESPONSE=$(curl -s -w "\n%{http_code}" "$HEALTH_ENDPOINT" -m 10)
    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-1)
    
    if [ "$HTTP_CODE" = "200" ]; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] ✓ Health check passed: $BODY"
    else
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] ✗ Health check failed with status $HTTP_CODE"
    fi
    
    echo "Next ping in $INTERVAL minutes..."
    sleep "$INTERVAL_SECONDS"
done
