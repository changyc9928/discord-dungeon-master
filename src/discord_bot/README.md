# Discord Bot Module (Poise Framework)

This module implements the Discord bot functionality using the Poise command framework, which provides a high-level abstraction over Serenity for easier slash command development.

## Setup

### 1. Add Dependencies to Cargo.toml

Add the following to your `Cargo.toml`:

```toml
poise = "0.6"
serenity = { version = "0.12", features = ["client", "gateway", "model"] }
tokio = { version = "1", features = ["full"] }
```

### 2. Environment Variables

Set your Discord bot token as an environment variable:

```bash
export DISCORD_TOKEN="your_bot_token_here"
```

Or add it to your `.env` file:

```
DISCORD_TOKEN=your_bot_token_here
```

### 3. Discord Bot Setup

1. Go to [Discord Developer Portal](https://discord.com/developers/applications)
2. Create a new application
3. Go to "Bot" section and create a bot
4. Copy the token and set it as `DISCORD_TOKEN`
5. Under "OAuth2" > "URL Generator", select scopes:
   - `applications.commands`
   - `bot`
6. Select permissions:
   - `Send Messages`
   - `Read Messages/View Channels`
7. Use the generated URL to invite the bot to your server

### 4. Start the Bot

Update `main.rs` to start the Discord bot:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let discord_token = std::env::var("DISCORD_TOKEN")?;
    discord_bot::start_bot(&discord_token).await?;
    Ok(())
}
```

## Available Commands

### Ping
- **Command**: `/ping`
- **Description**: Responds with "Pong! 🏓" - Use this to check if the bot is alive
- **Usage**: Type `/ping` in any Discord channel where the bot has access

## Adding New Commands

With Poise, adding new commands is very simple using the `#[poise::command]` macro:

1. Create a new file in `src/discord_bot/commands/` (e.g., `hello.rs`)
2. Implement the command using the macro:

```rust
use crate::discord_bot::handler::{Context, Error};

/// Greets the user
#[poise::command(slash_command)]
pub async fn hello(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Hello, {}!", ctx.author().name)).await?;
    Ok(())
}
```

3. Add to `src/discord_bot/commands/mod.rs`:
   - Import: `pub mod hello;`
   - Export: `pub use hello::hello;`
4. Add to the commands vector in `src/discord_bot/handler.rs`:
   ```rust
   commands: vec![
       commands::ping::ping(),
       commands::hello::hello(),
   ],
   ```

## Command Parameters

Poise makes it easy to add typed parameters:

```rust
/// Echo command with a parameter
#[poise::command(slash_command)]
pub async fn echo(
    ctx: Context<'_>,
    #[description = "Text to echo"]
    text: String,
) -> Result<(), Error> {
    ctx.say(text).await?;
    Ok(())
}
```

Poise automatically handles:
- Type validation
- User/role mentions
- Option parameters
- Number ranges
- Enum choices

