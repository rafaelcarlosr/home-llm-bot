# home-llm-bot

A local Telegram bot that orchestrates Home Assistant and LM Studio with speech-to-text.

## Architecture

- **Bot**: Rust service running in LXC container
- **LM Studio**: Local LLM (Qwen 3.5 35B) on desktop/remote
- **Home Assistant**: Home automation hub
- **Whisper**: Local speech-to-text service

## Setup

### Prerequisites

- LM Studio running with a model loaded
- Home Assistant instance
- Whisper service (local or remote)
- Telegram bot token from BotFather

### Local Development

1. Clone repo and create `.env`:

```bash
cp .env.example .env
# Edit .env with your values
```

2. Run with docker-compose:

```bash
docker-compose up --build
```

3. Or run locally with Rust:

```bash
cargo run
```

### Production (Proxmox LXC)

1. Create LXC container from Debian image
2. Install Rust and dependencies
3. Copy repo and run:

```bash
cargo build --release
./target/release/home-llm-bot
```

Or use the Dockerfile to create an image.

## Environment Variables

See `.env.example` for all required variables.

## Features

- ✅ Telegram interface with stateful conversations
- ✅ Function calling with local LM Studio
- ✅ Home Assistant integration
- ✅ Speech-to-text with Whisper
- ✅ Family conversation context
- 🚧 Plugin system for easy extensions
