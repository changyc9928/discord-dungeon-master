# Discord Dungeon Master

A D&D rules validator and game integrity auditor Discord bot powered by Google Gemini LLM. The bot listens to Discord channels and validates D&D game rulings, character actions, and provides rules explanations.

## Overview

This project combines:
- **Discord Bot**: Poise framework for slash commands and message listening
- **LLM Integration**: Google Gemini API for D&D rules validation
- **Character Management**: PostgreSQL-backed character sheet system
- **Rules Engine**: Automatic proficiency bonus calculation, spell slot tracking, and D&D mechanics validation

## Prerequisites

Before running the project, ensure you have:

1. **Rust** (1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Docker** and **Docker Compose**
   - [Install Docker Desktop](https://www.docker.com/products/docker-desktop)

3. **API Keys**
   - Google Gemini API key: [Get it here](https://makersuite.google.com/app/apikey)
   - Discord Bot token: [Create bot on Discord Developer Portal](https://discord.com/developers/applications)

## Configuration

### 1. Set Environment Variables

```bash
export GEMINI_API_KEY="your-gemini-api-key"
export DISCORD_TOKEN="your-discord-bot-token"
```

### 2. Update `config/config.yaml`

Create or update the configuration file with your settings:

```yaml
service-name: ai-dm
server:
  host: 0.0.0.0
  port: 8001
  enable-swagger-ui: false
  cors-origins:
    - http://localhost:3000

database:
  host: localhost
  port: 5432
  db-name: ai_dm
  username: postgres
  password: ""
  max-open-conns: 5
  conn-max-lifetime-secs: 1800

gemini-model: gemini-2.0-flash
discord-token: null  # Optional, can use env var instead
```

## Running the Project

### Option 1: Docker Compose (Recommended)

```bash
# Start all services (bot, PostgreSQL)
docker compose up --build

# Stop services
docker compose down
```

The bot will automatically:
- Initialize PostgreSQL
- Run migrations
- Connect to Discord
- Start listening to messages

### Option 2: Local Development

1. **Start PostgreSQL** (if not using Docker):
   ```bash
   # Make sure PostgreSQL is running on localhost:5432
   ```

2. **Run the application**:
   ```bash
   cargo run --release
   ```

3. **Watch mode** (auto-rebuild on changes):
   ```bash
   cargo watch -x run
   ```

## Usage

### Discord Bot Commands

#### `/ping`
Simple health check command to verify the bot is online.

```
/ping
→ Pong! 🏓
```

### Message Validation

Simply post a message in any channel where the bot has access:

**DM Message (will be validated):**
```
Player casts Fireball twice in one turn
```

**Bot Response:**
```
❌ Invalid

Reason:
- Casting Fireball twice violates action economy rules
- Only one leveled spell per turn is allowed

Suggestion:
- Limit to one Fireball per turn
- Consider bonus action spells alternatively
```

**Player Question (will be explained):**
```
Can I cast two spells in one turn?
```

**Bot Response:**
```
[RULE EXPLANATION]

- You can only cast one leveled spell per turn unless a specific feature allows otherwise
- You may cast a cantrip alongside a bonus action spell if rules permit
```

## Project Structure

```
.
├── src/
│   ├── main.rs                 # Application entry point
│   ├── config.rs               # Configuration handling
│   ├── error.rs                # Global error types
│   ├── pg_pool.rs              # Database pool management
│   ├── character/              # Character sheet module
│   │   ├── entity.rs           # D&D character models
│   │   ├── service.rs          # Character business logic
│   │   ├── repository.rs       # Database access
│   │   └── error.rs            # Character-specific errors
│   ├── llm/                    # LLM integration module
│   │   ├── mod.rs              # LLM trait definition
│   │   ├── gemini.rs           # Google Gemini implementation
│   │   ├── types.rs            # LLM request types
│   │   └── error.rs            # LLM-specific errors
│   └── discord_bot/            # Discord bot module
│       ├── handler.rs          # Bot initialization & event handler
│       ├── commands/           # Slash commands
│       │   ├── mod.rs
│       │   └── ping.rs         # /ping command
│       ├── mod.rs
│       └── error.rs            # Bot-specific errors
├── config/
│   └── config.yaml             # Configuration file
├── migrations/                 # SQL migrations
│   └── 20260323063002_create_character_sheet_table.sql
├── Cargo.toml                  # Rust dependencies
├── Dockerfile                  # Container build
├── docker-compose.yaml         # Multi-service orchestration
└── README.md                   # This file
```

## Key Features

### Character Sheet Management
- Store character data in PostgreSQL with JSONB columns
- Query characters by Discord ID or character name
- Automatic duplicate detection for character names
- Performance-optimized with database indexes

### LLM-Powered Validation
- All 9 character update tools integrated with Gemini
- Tool calling with context preservation
- Two-stage validation: character query → rules validation
- Error handling with detailed explanations

### D&D Rules Enforcement
- Proficiency bonus auto-validation (formula: 2 + ((level - 1) / 4))
- Action economy checks
- Spell slot tracking
- Inventory and equipment management
- HP and level progression

### Discord Integration
- Message listening and auto-response
- Slash commands with type safety
- Error reporting in Discord
- User-specific message processing

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test character::service::test

# Run with output
cargo test -- --nocapture
```

### Building for Production

```bash
# Release build
cargo build --release

# Output location: target/release/discord-dungeon-master
```

## Troubleshooting

### Bot not responding to messages
- Ensure `MESSAGE_CONTENT` intent is enabled in Discord Developer Portal
- Check that the bot has message read permissions in the channel
- Verify `DISCORD_TOKEN` environment variable is set

### Database connection errors
- Confirm PostgreSQL is running (Docker: `docker compose ps`)
- Check database credentials in `config/config.yaml`
- Verify migrations ran: `docker compose logs ai-dm`

### Gemini API errors
- Verify `GEMINI_API_KEY` environment variable is set
- Check API key is valid at [makersuite.google.com](https://makersuite.google.com)
- Ensure model name matches available Gemini models

### Port conflicts
- Change ports in `docker-compose.yaml`:
  - Bot: `38001:8001`
  - PostgreSQL: `35432:5432`

## Architecture

### Message Flow
```
1. User sends message in Discord channel
2. Bot receives message via FullEvent handler
3. LLM validates D&D rules & character actions
4. Available tools called: get_character, add_item, update_hp, etc.
5. Character sheet updated in PostgreSQL
6. Validation result posted back to Discord
```

### Service Layers
- **Discord Layer**: Poise framework handles Discord API
- **LLM Layer**: Gemini integration with tool dispatch
- **Character Layer**: Service/Repository pattern for CRUD
- **Database Layer**: SQLx with async PostgreSQL driver

## Configuration Details

### Database Config
- `host`: PostgreSQL server address
- `port`: PostgreSQL port (default: 5432)
- `db-name`: Database name (default: ai_dm)
- `username`: PostgreSQL user
- `password`: PostgreSQL password
- `max-open-conns`: Connection pool size
- `conn-max-lifetime-secs`: Connection lifetime

### Gemini Config
- `gemini-model`: Model to use (e.g., `gemini-2.0-flash`, `gemini-pro`)
- Must match available models from [Google AI Studio](https://aistudio.google.com)

### Discord Config
- `discord-token`: Bot token (can be set via `DISCORD_TOKEN` env var)

## Contributing

To add new features:

1. **Add slash commands**: Create new file in `src/discord_bot/commands/`
2. **Add character operations**: Extend `src/character/service.rs`
3. **Add LLM tools**: Update `src/llm/types.rs` and `dispatch()` in `src/llm/gemini.rs`
4. **Update tests**: Add tests alongside implementations

## License

[Add your license here]

## Support

For issues or questions:
- Check the troubleshooting section above
- Review Discord Developer documentation
- Consult Google Gemini API docs
- Open an issue in the repository
