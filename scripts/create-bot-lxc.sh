#!/usr/bin/env bash
# create-bot-lxc.sh — Create a Proxmox LXC for home-llm-bot (Rust binary + systemd)
# Run from the Proxmox host shell: bash scripts/create-bot-lxc.sh
set -euo pipefail

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
check_proxmox

# ---------------------------------------------------------------------------
# Prompt for LXC configuration
# ---------------------------------------------------------------------------
header "Home LLM Bot LXC Configuration"

DEFAULT_CTID=$(next_ctid)
prompt CTID       "Container ID"       "$DEFAULT_CTID"
prompt STORAGE    "Storage location"   "local-lvm"
prompt_ip NET_CONFIG "home-llm-bot"

# ---------------------------------------------------------------------------
# Prompt for bot configuration
# ---------------------------------------------------------------------------
header "Bot Configuration"

prompt TELEGRAM_TOKEN     "Telegram bot token"
prompt LM_STUDIO_URL      "LM Studio URL (e.g. http://192.168.1.100:1234)"
prompt LLM_MODEL          "LLM model name"           "qwen2.5-7b-instruct"
prompt HOME_ASSISTANT_URL "Home Assistant URL (e.g. http://192.168.1.100:8123)"
prompt HOME_ASSISTANT_TOKEN "Home Assistant long-lived access token"
prompt WHISPER_URL        "Whisper STT URL (e.g. http://192.168.1.101:8000)"

# DATABASE_URL is fixed — not prompted
DATABASE_URL="sqlite:///opt/home-llm-bot/data/bot.db"

# ---------------------------------------------------------------------------
# Create LXC
# ---------------------------------------------------------------------------
ensure_template "$STORAGE"

msg "Creating bot LXC..."
create_lxc "$CTID" "home-llm-bot" "$STORAGE" "$NET_CONFIG" 4 2048 1024 8

# ---------------------------------------------------------------------------
# Start LXC and wait for IP
# ---------------------------------------------------------------------------
msg "Starting LXC container ${CTID}..."
pct start "$CTID"

LXC_IP=$(get_lxc_ip "$CTID")
msg "Container IP: ${LXC_IP}"

# ---------------------------------------------------------------------------
# Install build dependencies
# ---------------------------------------------------------------------------
msg "Installing build dependencies..."
pct exec "$CTID" -- bash -c "apt-get update -qq && apt-get install -y build-essential pkg-config libssl-dev libsqlite3-dev git curl sudo"

# ---------------------------------------------------------------------------
# Create bot user and directories
# ---------------------------------------------------------------------------
msg "Creating bot user and base directory..."
# /opt/home-llm-bot must be empty for git clone to succeed — data/ is created after the build
pct exec "$CTID" -- bash -c "useradd -r -m -s /bin/bash bot && mkdir -p /opt/home-llm-bot && chown bot:bot /opt/home-llm-bot"

# ---------------------------------------------------------------------------
# Install Rust toolchain via rustup
# ---------------------------------------------------------------------------
msg "Installing Rust toolchain via rustup (as bot user)..."
pct exec "$CTID" -- bash -c "su - bot -c 'curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path'"

# ---------------------------------------------------------------------------
# Clone repo and build release binary
# ---------------------------------------------------------------------------
msg "Cloning repository and building release binary (this takes several minutes)..."
pct exec "$CTID" -- bash -c "su - bot -c 'git clone https://github.com/rafaelcarlosr/home-llm-bot.git /opt/home-llm-bot'"
pct exec "$CTID" -- bash -c "su - bot -c 'cd /opt/home-llm-bot && /home/bot/.cargo/bin/cargo build --release'"

msg "Build complete."

# Create data directory after clone (must not exist before git clone — non-empty dir would fail)
pct exec "$CTID" -- bash -c "mkdir -p /opt/home-llm-bot/data && chown -R bot:bot /opt/home-llm-bot"

# ---------------------------------------------------------------------------
# Write .env file
# ---------------------------------------------------------------------------
msg "Writing .env configuration file..."
# Use pct push to avoid shell injection — variables may contain single quotes or special chars
TMP_ENV=$(mktemp)
cat > "$TMP_ENV" << EOF
TELEGRAM_TOKEN=${TELEGRAM_TOKEN}
LM_STUDIO_URL=${LM_STUDIO_URL}
LLM_MODEL=${LLM_MODEL}
HOME_ASSISTANT_URL=${HOME_ASSISTANT_URL}
HOME_ASSISTANT_TOKEN=${HOME_ASSISTANT_TOKEN}
WHISPER_URL=${WHISPER_URL}
DATABASE_URL=${DATABASE_URL}
EOF
pct push "$CTID" "$TMP_ENV" /opt/home-llm-bot/.env --perms 600 --user bot --group bot
rm -f "$TMP_ENV"

# ---------------------------------------------------------------------------
# Write systemd service file
# ---------------------------------------------------------------------------
msg "Writing systemd service file..."
pct exec "$CTID" -- bash -c "printf '[Unit]\nDescription=Home LLM Bot - Telegram Bot\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser=bot\nWorkingDirectory=/opt/home-llm-bot\nExecStart=/opt/home-llm-bot/target/release/home-llm-bot\nRestart=on-failure\nRestartSec=5\nStartLimitBurst=5\nStartLimitIntervalSec=60\nEnvironmentFile=/opt/home-llm-bot/.env\n\n[Install]\nWantedBy=multi-user.target\n' > /etc/systemd/system/home-llm-bot.service"

# ---------------------------------------------------------------------------
# Enable and start the service
# ---------------------------------------------------------------------------
msg "Enabling and starting home-llm-bot service..."
pct exec "$CTID" -- bash -c "systemctl daemon-reload && systemctl enable home-llm-bot && systemctl start home-llm-bot"

# Verify service started
pct exec "$CTID" -- bash -c "systemctl is-active home-llm-bot" \
  || warn "Service may not have started cleanly — check logs inside the container."

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
header "Home LLM Bot LXC Ready"
printf "\n"
msg "CT ID:      ${CTID}"
msg "IP Address: ${LXC_IP}"
printf "\n"
msg "Check service status:"
printf "  pct exec %s -- systemctl status home-llm-bot\n" "$CTID"
printf "\n"
msg "View logs:"
printf "  pct exec %s -- journalctl -u home-llm-bot -f\n" "$CTID"
printf "\n"
msg "Update bot:"
printf "  pct exec %s -- bash -c 'sudo -u bot bash -c \"cd /opt/home-llm-bot && git pull && /home/bot/.cargo/bin/cargo build --release\" && systemctl restart home-llm-bot'\n" "$CTID"
printf "\n"
