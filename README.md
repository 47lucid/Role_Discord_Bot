# Discord Role Restore Bot

A powerful Discord bot that automatically restores user roles when members rejoin your server. Never lose role assignments again!

## 🎯 Features

- **Automatic Role Restoration** - Saves member roles when they leave and automatically restores them upon rejoin
- **Safe Roles** - Configure specific roles to always be restored
- **Avoid Roles** - Designate roles to exclude from automatic restoration
- **Admin Security** - Option to prevent admin roles from being auto-restored for security purposes
- **Log Channel** - Track all role restoration events in a dedicated channel
- **Admin-Only** - Restricted to server owners and administrators
- **Easy Setup** - Single `/setup` command with intuitive dropdown menus
- **Smart Components** - Auto-disabling components after 5 minutes of inactivity
- **Mutually Exclusive Lists** - Roles can't be in both safe and avoid lists

## 📋 Requirements

- Rust 1.70+ (for building from source)
- Discord bot token from [Discord Developer Portal](https://discord.com/developers/applications)
- Python 3.8+ (if using the provided Python build scripts, optional)

## ⚙️ Installation

### Option 1: Build from Source

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/discord-role-restore.git
   cd discord-role-restore
   ```

2. **Install Rust** (if not already installed)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. **Build the project**
   ```bash
   cargo build --release
   ```

4. **Run the bot**
   ```bash
   $env:DISCORD_TOKEN="your_bot_token_here"
   ./target/release/discord-role-restore.exe
   ```

### Option 2: Pre-built Binaries
Download the latest release from the [Releases](https://github.com/yourusername/discord-role-restore/releases) page.

## 🔧 Configuration

### Step 1: Create a Discord Bot

1. Go to [Discord Developer Portal](https://discord.com/developers/applications)
2. Click "New Application" and name your bot
3. Go to the "Bot" tab and click "Add Bot"
4. Copy the token (keep this secret!)
5. Enable these Gateway Intents:
   - **Presence Intent**
   - **Server Members Intent**
   - **Message Content Intent**

### Step 2: Set Bot Permissions

Invite URL: `https://discord.com/api/oauth2/authorize?client_id=YOUR_CLIENT_ID&permissions=268435456&scope=bot`

Required permissions:
- Manage Roles
- Send Messages
- View Channels
- Read Message History

### Step 3: Set Environment Variable

**Windows (PowerShell):**
```powershell
$env:DISCORD_TOKEN="your_bot_token_here"
```

**Windows (Command Prompt):**
```cmd
set DISCORD_TOKEN=your_bot_token_here
```

**Linux/Mac:**
```bash
export DISCORD_TOKEN="your_bot_token_here"
```

## 📖 Usage

### Available Commands

#### `/setup`
Configure the bot for your server. This command opens an interactive setup panel with:
- **Safe Roles Dropdown** - Select roles that should always be restored
- **Avoid Roles Dropdown** - Select roles to exclude from restoration
- **Log Channel Dropdown** - Choose a channel for logging role restorations
- **Admin Filter Toggle** - Enable/disable auto-restoration of admin roles

**Permissions:** Server Owner or Administrator

### How It Works

1. **Member Leaves**: When a member leaves your server, their current roles are saved to the database
2. **Member Rejoins**: When they rejoin, the bot automatically checks the saved roles
3. **Roles Restored**: 
   - Restored roles: Roles in the "Safe Roles" list
   - Skipped roles: Roles in the "Avoid Roles" list or (optionally) admin roles
4. **Logging**: If a log channel is configured, the bot posts updates there

## 💾 Database

The bot uses SQLite for persistence. The `discord_roles.db` file will be created automatically on first run.

**Configuration stored per server:**
- Safe roles list
- Avoid roles list
- Log channel ID
- Admin filter setting

## 📊 Architecture

```
src/
├── main.rs      - Bot initialization and event handlers
├── commands.rs  - Slash command handlers and interactive components
└── db.rs        - Database operations and role persistence
```

### Key Components

- **Event Handler** - Listens to member join/leave events
- **Command Handler** - Processes `/setup` command
- **Component Handler** - Handles dropdown selections and button clicks
- **Database** - SQLite-based role storage and retrieval

## 🔒 Security

- Only server owners and administrators can modify bot settings
- Admin roles are optionally filtered to prevent privilege escalation
- Role restoration respects current role hierarchy
- No sensitive data is stored or logged

## 🚀 Deployment

### Local Development
```bash
cargo run
```

### Production (systemd on Linux)
Create `/etc/systemd/system/discord-role-restore.service`:
```ini
[Unit]
Description=Discord Role Restore Bot
After=network.target

[Service]
Type=simple
User=discord
WorkingDirectory=/opt/discord-role-restore
Environment="DISCORD_TOKEN=your_token"
ExecStart=/opt/discord-role-restore/discord-role-restore
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Then enable:
```bash
sudo systemctl enable discord-role-restore
sudo systemctl start discord-role-restore
```

## 🐛 Troubleshooting

### Bot not responding to commands
- Ensure the bot has "Manage Roles" permission
- Check that the bot is higher in the role hierarchy than the roles being managed
- Verify the bot has the correct gateway intents enabled

### Roles not being restored
- Check if roles are in the "Avoid Roles" list
- Verify the bot has permission to assign those roles
- Check the log channel for error details

### Database lock errors
- Close any other instances of the bot
- Delete `discord_roles.db` if corrupted (roles will need to be reconfigured)

## 📝 Logs

The bot outputs logs to stdout for debugging:
- Role save/restore operations
- Permission failures
- Database errors
- Component interaction failures

## 🤝 Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🆘 Support

For issues, questions, or suggestions:
- Open an [Issue](https://github.com/yourusername/discord-role-restore/issues)
- Contact the developers on Discord

## 🎉 Credits

Built with:
- [Serenity](https://github.com/serenity-rs/serenity) - Discord bot framework
- [Tokio](https://tokio.rs/) - Async runtime
- [SQLite](https://www.sqlite.org/) - Database

---

**Made with ❤️ for Discord server administrators**
