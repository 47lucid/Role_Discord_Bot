#!/usr/bin/env python3
"""
Keep-Alive Script for Render Deployment
Pings the /health endpoint every 14 minutes to prevent the service from spinning down.

Usage:
    python3 keep_alive.py <url>
    Example: python3 keep_alive.py https://my-bot.onrender.com

Install requirements:
    pip install requests
"""

import requests
import time
import sys
import logging
from datetime import datetime

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

def ping_health_check(url: str, interval_minutes: int = 14) -> None:
    """
    Continuously ping the health check endpoint.
    
    Args:
        url: The base URL of the bot (e.g., https://my-bot.onrender.com)
        interval_minutes: How often to ping in minutes (default: 14 to stay under Render's 15 min limit)
    """
    base_url = url.rstrip('/')
    health_endpoint = f"{base_url}/health"
    interval_seconds = interval_minutes * 60
    
    logger.info(f"Starting keep-alive pings to {health_endpoint}")
    logger.info(f"Ping interval: {interval_minutes} minutes")
    
    failure_count = 0
    max_failures = 5
    
    while True:
        try:
            response = requests.get(health_endpoint, timeout=10)
            
            if response.status_code == 200:
                logger.info(f"✓ Health check passed: {response.json()}")
                failure_count = 0
            else:
                logger.warning(f"✗ Health check returned status {response.status_code}")
                failure_count += 1
                
        except requests.exceptions.RequestException as e:
            logger.error(f"✗ Failed to ping health endpoint: {e}")
            failure_count += 1
            
            if failure_count >= max_failures:
                logger.error(f"Too many failures ({failure_count}). Stopping.")
                sys.exit(1)
        
        logger.info(f"Next ping in {interval_minutes} minutes at {datetime.now().strftime('%H:%M:%S')}")
        time.sleep(interval_seconds)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 keep_alive.py <url> [interval_minutes]")
        print("Example: python3 keep_alive.py https://my-bot.onrender.com")
        print("Example with custom interval: python3 keep_alive.py https://my-bot.onrender.com 10")
        sys.exit(1)
    
    url = sys.argv[1]
    interval = int(sys.argv[2]) if len(sys.argv) > 2 else 14
    
    try:
        ping_health_check(url, interval)
    except KeyboardInterrupt:
        logger.info("Keep-alive script stopped by user")
        sys.exit(0)
