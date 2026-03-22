# Keep-Alive Setup for Render

This guide explains how to keep your Discord bot running on Render and prevent service spin-down.

## The Problem

Render's free tier automatically spins down services that don't receive HTTP requests every 15 minutes. Since Discord bots don't generate external HTTP traffic, they'll go offline after inactivity.

## The Solution

The bot now includes a built-in HTTP health check endpoint that can be pinged to keep the service alive.

## How It Works

1. **Health Check Endpoint**: The bot runs an HTTP server on port 8080 with two endpoints:
   - `GET /health` - Returns current health status
   - `GET /ready` - Alternative ready check endpoint

2. **Keep-Alive Detection**: Render will detect the HTTP traffic from pings and keep the service alive

## Setup Options

### Option 1: Use Uptime Robot (Free & Easiest)

Uptime Robot is a free service that can ping your endpoint every 5 minutes.

1. Go to [uptimerobot.com](https://uptimerobot.com)
2. Sign up for a free account
3. Click "Add Monitor" (New Monitor)
4. Configure:
   - **Monitor Type**: HTTP(s)
   - **URL**: `https://your-bot-name.onrender.com/health`
   - **Friendly Name**: "Discord Bot Keep-Alive"
   - **Check Interval**: 5 minutes
   - **Alert Contacts**: (optional) Set up alerts if the bot goes down
5. Click "Create Monitor"

Your bot will now stay alive when pinged every 5 minutes!

#### Why Uptime Robot?
- ✅ Free (up to 50 monitors)
- ✅ No setup required on your server
- ✅ Email notifications if bot goes down
- ✅ Status page available
- ✅ No cost, no limits (unlike paid alternatives)

### Option 2: Run Local Keep-Alive Script

If you want to run a keep-alive script locally or on another server:

#### Python Version

1. Install requirements:
   ```bash
   pip install requests
   ```

2. Run the script:
   ```bash
   python3 keep_alive.py https://your-bot-name.onrender.com
   ```

3. (Optional) Run in background:
   ```bash
   # Linux/MacOS
   nohup python3 keep_alive.py https://your-bot-name.onrender.com &
   
   # Windows (in a separate terminal)
   START /B python keep_alive.py https://your-bot-name.onrender.com
   ```

#### Bash Version

1. Make executable:
   ```bash
   chmod +x keep_alive.sh
   ```

2. Run:
   ```bash
   ./keep_alive.sh https://your-bot-name.onrender.com
   ```

### Option 3: Use a Cron Job (Linux/MacOS)

Add to your crontab to run every 10 minutes:

```bash
# Edit crontab
crontab -e

# Add this line
*/10 * * * * curl -s https://your-bot-name.onrender.com/health > /dev/null
```

### Option 4: Use GitHub Actions (Free)

Create a `.github/workflows/keep-alive.yml` file:

```yaml
name: Keep Render Bot Alive

on:
  schedule:
    - cron: '*/14 * * * *'  # Run every 14 minutes
  workflow_dispatch:

jobs:
  ping:
    runs-on: ubuntu-latest
    steps:
      - name: Ping health endpoint
        run: curl https://your-bot-name.onrender.com/health
```

## Testing the Setup

Test that the health endpoint is working:

```bash
curl https://your-bot-name.onrender.com/health
```

You should see a JSON response:
```json
{
  "status": "ok",
  "timestamp": "2024-03-22T10:15:30Z",
  "uptime_check": "v1"
}
```

## Monitoring

To check if the keep-alive is working, you can:

1. **Check Render logs**: Visit your Render dashboard and view the bot's logs for HTTP requests
2. **Use Uptime Robot**: View the monitor status and history
3. **Manual test**: Run `curl https://your-bot-name.onrender.com/health` periodically

## Important Notes

- ⚠️ **Port**: The HTTP server runs on port 8080. Render automatically exposes this, but it's not necessary to expose it as a public service
- ⚠️ **Interval**: Ping at least once every 14 minutes to stay under Render's 15-minute spin-down threshold
- ⚠️ **URL**: Replace `your-bot-name` with your actual Render service name
- ✅ The health endpoint is read-only and logs activity, so it's safe to call frequently

## Troubleshooting

### Health endpoint returns 503 or connection refused
- Bot may not have started yet
- Check Render logs for startup errors
- Restart the bot from the Render dashboard

### Bot still spins down
- Verify the keep-alive service is running (check Uptime Robot dashboard or cron logs)
- Make sure you're pinging the correct URL
- Check Render logs to see if HTTP requests are being received

### How to find your bot's public URL
1. Open [render.com](https://render.com) dashboard
2. Click on your bot service
3. Copy the URL from the top (format: `https://service-name-xxxxx.onrender.com`)

## Rendering the Health Endpoint Public

If for some reason you need to explicitly expose the health endpoint:

1. In Render dashboard, go to your service settings
2. Scroll to "Environment"
3. Render automatically exposes port 8080 for external requests
4. No additional configuration needed!

## Summary

**Recommended Setup**: Use Uptime Robot (free, reliable, no maintenance)
- Set up takes 2 minutes
- Completely free
- Works reliably
- No code changes needed

The health check endpoint is now part of your bot and will work with any HTTP monitoring service.
