# Deployment Scripts Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create two Proxmox LXC deployment scripts and update repo config files so the entire home-llm-bot stack can be deployed from scratch on a Proxmox host.

**Architecture:** Two bash scripts run from the Proxmox host shell. Script 1 creates a Whisper STT LXC with Docker + GPU passthrough. Script 2 creates the bot LXC with Rust binary + systemd. Both share a common helper library for LXC creation. Config files (.env.example, docker-compose.yml) are updated to match the new Whisper service.

**Tech Stack:** Bash, Proxmox pct CLI, Docker CE, NVIDIA GPU passthrough, systemd

**Spec:** docs/superpowers/specs/2026-03-16-deployment-design.md

---

## File Structure

```
scripts/
  common.sh                  # Shared helper functions (colors, prompts, LXC creation)
  create-whisper-lxc.sh      # Whisper STT LXC with Docker + GPU
  create-bot-lxc.sh          # Bot LXC with Rust binary + systemd
.env.example                   # Updated: port 8000, LLM_MODEL added
docker-compose.yml             # Updated: faster-whisper-server image
```

| File | Responsibility |
|------|----------------|
| scripts/common.sh | Color output, user prompts, Debian 12 template download, next CT ID detection, LXC creation via pct create |
| scripts/create-whisper-lxc.sh | NVIDIA checks, GPU passthrough config, Docker install, faster-whisper-server container, model pre-download |
| scripts/create-bot-lxc.sh | Env var prompts, Rust toolchain install, repo clone, cargo build, bot user, systemd service |
| .env.example | Reference config for developers - updated ports and vars |
| docker-compose.yml | Local dev setup - updated whisper image |

---

## Chunk 1: Config File Updates + Common Helpers

### Task 1: Update .env.example

**Files:**
- Modify: .env.example

- [ ] **Step 1: Update .env.example with new Whisper port and LLM_MODEL**

Change WHISPER_URL port from 9000 to 8000, add LLM_MODEL variable.

- [ ] **Step 2: Commit**

```bash
git add .env.example
git commit -m "chore: update .env.example for faster-whisper-server on port 8000"
```

---

### Task 2: Update docker-compose.yml

**Files:**
- Modify: docker-compose.yml

- [ ] **Step 1: Replace whisper service with faster-whisper-server**

Replace the onerahpurwanto/openai-whisper-api image with fedirz/faster-whisper-server:latest-cuda. Change port from 9000 to 8000. Add WHISPER__MODEL and WHISPER__DEVICE env vars. Add GPU reservation under deploy.resources. Add whisper-models volume. Update bot service to use WHISPER_URL=http://whisper:8000 and add LLM_MODEL env var.

- [ ] **Step 2: Commit**

```bash
git add docker-compose.yml
git commit -m "chore: switch to faster-whisper-server in docker-compose"
```

---

### Task 3: Create scripts/common.sh

**Files:**
- Create: scripts/common.sh

- [ ] **Step 1: Write the shared helper library**

Functions to include:
- msg(), warn(), err(), header() - colored output
- check_proxmox() - verify running on PVE host (pct exists)
- next_ctid() - find next available CT ID starting from 100
- ensure_template() - download Debian 12 template if not cached
- prompt() - read user input with optional default
- prompt_ip() - prompt for DHCP vs static IP with CIDR and gateway
- create_lxc() - wrapper around pct create with all standard params
- lxc_exec() - wrapper around pct exec for running commands inside LXC
- get_lxc_ip() - poll up to 30s for LXC to get an IP via hostname -I

- [ ] **Step 2: Make executable and commit**

```bash
chmod +x scripts/common.sh
git add scripts/common.sh
git commit -m "feat: add shared Proxmox LXC helper library"
```

---

## Chunk 2: Whisper LXC Script

### Task 4: Create scripts/create-whisper-lxc.sh

**Files:**
- Create: scripts/create-whisper-lxc.sh

- [ ] **Step 1: Write the Whisper LXC creation script**

Script flow:
1. Source common.sh
2. Pre-flight: check_proxmox, verify nvidia-smi, detect driver version
3. Detect NVIDIA UVM major number from /proc/devices
4. Prompt: CT ID (default: next_ctid), storage (default: local-lvm), network (DHCP/static)
5. ensure_template, create_lxc with nesting=1, 4 cores, 4096MB RAM, 512MB swap, 16GB disk
6. Append GPU passthrough to /etc/pve/lxc/CTID.conf:
   - cgroup2 devices allow for major 195 and detected UVM major
   - bind-mount /dev/nvidia0, nvidiactl, nvidia-uvm, nvidia-uvm-tools, nvidia-caps
7. Auto-detect and bind-mount all host NVIDIA libraries (libnvidia-*.so.1, libcuda.so.1, libnvcuvid.so.1) and nvidia-smi binary
8. Start LXC, wait for IP
9. Install Docker CE (official repo, Debian bookworm)
10. Verify nvidia-smi inside LXC
11. docker run faster-whisper-server with --device flags (not --gpus all), port 8000, WHISPER__MODEL, WHISPER__DEVICE=cuda, whisper-models volume
12. Pre-download model via docker exec python command
13. Print summary with IP, port, test curl command, WHISPER_URL for bot .env

- [ ] **Step 2: Make executable and commit**

```bash
chmod +x scripts/create-whisper-lxc.sh
git add scripts/create-whisper-lxc.sh
git commit -m "feat: add Whisper STT LXC creation script with GPU passthrough"
```

---

## Chunk 3: Bot LXC Script

### Task 5: Create scripts/create-bot-lxc.sh

**Files:**
- Create: scripts/create-bot-lxc.sh

- [ ] **Step 1: Write the bot LXC creation script**

Script flow:
1. Source common.sh
2. Pre-flight: check_proxmox
3. Prompt: CT ID, storage, network
4. Prompt all bot config values: TELEGRAM_TOKEN, LM_STUDIO_URL, LLM_MODEL, HOME_ASSISTANT_URL, HOME_ASSISTANT_TOKEN, WHISPER_URL
5. ensure_template, create_lxc with 4 cores, 2048MB RAM, 1024MB swap, 8GB disk
6. Start LXC, wait for IP
7. Install build deps: build-essential, pkg-config, libssl-dev, libsqlite3-dev, git, curl, sudo
8. Create bot user (useradd -r -m), create /opt/home-llm-bot/data
9. Install Rust via rustup as bot user
10. Clone repo and cargo build --release as bot user
11. Write .env file to /opt/home-llm-bot/.env with all prompted values, chmod 600, chown bot:bot
12. Write systemd service file to /etc/systemd/system/home-llm-bot.service with User=bot, Restart=on-failure, StartLimitBurst/Interval, EnvironmentFile
13. systemctl daemon-reload, enable, start
14. Check service is active, print summary with logs/status/update commands

- [ ] **Step 2: Make executable and commit**

```bash
chmod +x scripts/create-bot-lxc.sh
git add scripts/create-bot-lxc.sh
git commit -m "feat: add bot LXC creation script with Rust build and systemd"
```

---

## Chunk 4: Verification and Push

### Task 6: Verify and push

- [ ] **Step 1: Run bash syntax check on all scripts**

```bash
bash -n scripts/common.sh
bash -n scripts/create-whisper-lxc.sh
bash -n scripts/create-bot-lxc.sh
```

Expected: No output (clean syntax).

- [ ] **Step 2: Run existing Rust tests to ensure nothing is broken**

```bash
cargo test
```

Expected: 14 passed, 0 failed, 3 ignored.

- [ ] **Step 3: Push to GitHub**

```bash
git push origin main
```

---

## Deployment Checklist (manual, on Proxmox host)

After the scripts are pushed, deployment on the Proxmox host follows this order:

1. git clone https://github.com/rafaelcarlosr/home-llm-bot.git && cd home-llm-bot
2. bash scripts/create-whisper-lxc.sh - note the Whisper IP printed
3. bash scripts/create-bot-lxc.sh - enter the Whisper IP when prompted
4. Enable LM Studio "Serve on Local Network" on desktop
5. Send a Telegram message to the bot - verify response
