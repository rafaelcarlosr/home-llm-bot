# Deployment Design: home-llm-bot on Proxmox

> **Date:** 2026-03-16
> **Status:** Draft
> **Environment:** Proxmox VE 9.1.5, Dual Xeon E5-2697 v2, RTX 2060 6GB, flat LAN

## Summary

Deploy three components on a Proxmox home server:

1. **Whisper STT LXC** — Debian 12 LXC with Docker, running `faster-whisper-server` (large-v3-turbo) with NVIDIA GPU passthrough
2. **home-llm-bot LXC** — Debian 12 LXC with compiled Rust binary, systemd service, SQLite persistence
3. **LM Studio** — Already running on desktop, no deployment needed (just enable "Serve on Local Network")

Both LXCs are created via automated bash scripts run from the Proxmox host shell.

**Breaking change from docker-compose setup:** The Whisper service changes from `onerahpurwanto/openai-whisper-api` on port 9000 to `faster-whisper-server` on port 8000. The `.env.example` in the repo will be updated to reflect this.

## Architecture

```
┌──────────────── Proxmox Host (PVE 9.1.5) ──────────────────────────┐
│  Dual Xeon E5-2697 v2 (24c/48t) │ RTX 2060 6GB │ Flat LAN         │
│                                                                      │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────────┐  │
│  │ LXC: whisper-stt  │  │ LXC: home-llm-bot│  │ LXC: HA (existing)│  │
│  │ Debian 12         │  │ Debian 12         │  │                   │  │
│  │                   │  │                   │  │                   │  │
│  │ Docker:           │  │ Rust binary       │  │ Home Assistant    │  │
│  │  faster-whisper   │  │ systemd service   │  │ :8123             │  │
│  │  large-v3-turbo   │  │ SQLite DB         │  │                   │  │
│  │  GPU passthrough  │  │ No GPU needed     │  │                   │  │
│  │  :8000            │  │ outbound only     │  │                   │  │
│  └────────┬──────────┘  └────────┬──────────┘  └────────┬──────────┘  │
│           └──────────────────────┴───────────────────────┘             │
│                           vmbr0 bridge (LAN)                          │
└───────────────────────────────┬───────────────────────────────────────┘
                                │ Flat LAN (2.5G / 10G)
                  ┌─────────────┴──────────────┐
                  │  Desktop (2.5G wired+wifi)  │
                  │  LM Studio :1234            │
                  └─────────────────────────────┘
```

## Prerequisites

### On the Proxmox Host (one-time setup)

NVIDIA drivers must be installed on the Proxmox host for GPU passthrough to LXC containers. This is a prerequisite shared with the existing Plex LXC.

```bash
# Check if NVIDIA driver is already installed
nvidia-smi
```

If not installed, the Whisper LXC script will check and guide you through installation.

### GPU Sharing Model

The RTX 2060 is shared between LXC containers (Plex + Whisper). This works because:

- LXC containers share the host kernel — GPU device nodes (`/dev/nvidia*`) are bind-mounted into each container
- Plex uses NVENC/NVDEC (dedicated hardware encoder/decoder on the GPU die) — does NOT consume VRAM
- Whisper uses CUDA compute + ~6GB VRAM — fits within the 6GB RTX 2060
- Both can run simultaneously without conflict

---

## Component 1: Whisper STT LXC

### LXC Specification

| Setting | Value |
|---------|-------|
| Template | Debian 12 (Bookworm) |
| Type | Unprivileged (with GPU device access) |
| Hostname | `whisper-stt` |
| CPU | 4 cores |
| RAM | 4096 MB |
| Swap | 512 MB |
| Disk | 16 GB (models are ~3GB + Docker images) |
| Network | vmbr0, DHCP or static IP |
| Features | `nesting=1` (required for Docker inside LXC) |
| Start on boot | Yes |

### GPU Passthrough Configuration

Added to `/etc/pve/lxc/<CTID>.conf`.

**Important:** The cgroup major number for `/dev/nvidia-uvm` is dynamically assigned by the kernel. The script detects it at runtime:

```bash
# Detect UVM major number dynamically (on the Proxmox host)
NVIDIA_UVM_MAJOR=$(grep nvidia-uvm /proc/devices | awk '{print $1}')
```

LXC config entries:

```conf
# Features
features: nesting=1

# NVIDIA GPU passthrough
lxc.cgroup2.devices.allow: c 195:* rwm
lxc.cgroup2.devices.allow: c ${NVIDIA_UVM_MAJOR}:* rwm
lxc.mount.entry: /dev/nvidia0 dev/nvidia0 none bind,optional,create=file
lxc.mount.entry: /dev/nvidiactl dev/nvidiactl none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm dev/nvidia-uvm none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm-tools dev/nvidia-uvm-tools none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-caps dev/nvidia-caps none bind,optional,create=dir
```

### NVIDIA Driver Libraries Inside the LXC

The container's userspace NVIDIA libraries **must exactly match** the driver version on the Proxmox host. A mismatch causes `nvidia-smi` and all CUDA workloads to fail.

**Approach: Bind-mount host libraries (recommended)**

This guarantees version parity and survives host driver upgrades automatically. Additional LXC config entries:

```conf
# Bind-mount host NVIDIA libraries and binaries into the container
lxc.mount.entry: /usr/lib/x86_64-linux-gnu/libnvidia-ml.so.1 usr/lib/x86_64-linux-gnu/libnvidia-ml.so.1 none bind,optional,create=file
lxc.mount.entry: /usr/lib/x86_64-linux-gnu/libcuda.so.1 usr/lib/x86_64-linux-gnu/libcuda.so.1 none bind,optional,create=file
lxc.mount.entry: /usr/lib/x86_64-linux-gnu/libnvcuvid.so.1 usr/lib/x86_64-linux-gnu/libnvcuvid.so.1 none bind,optional,create=file
lxc.mount.entry: /usr/lib/x86_64-linux-gnu/libnvidia-encode.so.1 usr/lib/x86_64-linux-gnu/libnvidia-encode.so.1 none bind,optional,create=file
lxc.mount.entry: /usr/bin/nvidia-smi usr/bin/nvidia-smi none bind,optional,create=file
```

The script will auto-detect all `libnvidia-*.so.*` files on the host and generate the appropriate mount entries.

### Docker GPU Access

Since the GPU device nodes and NVIDIA libraries are already bind-mounted into the LXC, the Docker container uses explicit device flags instead of `--gpus all` (avoids NVIDIA Container Toolkit cgroup nesting issues in unprivileged LXCs):

```bash
docker run -d \
  --name whisper \
  --restart unless-stopped \
  --device /dev/nvidia0 \
  --device /dev/nvidiactl \
  --device /dev/nvidia-uvm \
  -p 8000:8000 \
  -v whisper-models:/root/.cache/huggingface \
  -e WHISPER__MODEL=Systran/faster-whisper-large-v3-turbo \
  -e WHISPER__DEVICE=cuda \
  fedirz/faster-whisper-server:latest-cuda
```

This avoids the need for NVIDIA Container Toolkit entirely — the devices are already present in the LXC namespace and passed through to Docker directly.

### What Gets Installed Inside

1. **Docker Engine** (official Docker CE repository)
2. **faster-whisper-server** Docker container (as shown above)
3. Model pre-download as part of install (avoids slow first request):
   ```bash
   docker exec whisper python -c "from faster_whisper import WhisperModel; WhisperModel('Systran/faster-whisper-large-v3-turbo', device='cuda')"
   ```

### API

- **Endpoint:** `http://<whisper-ip>:8000/v1/audio/transcriptions`
- **Format:** OpenAI-compatible (drop-in replacement for bot's existing Whisper calls)
- **Note:** `faster-whisper-server` uses the model from `WHISPER__MODEL` env var regardless of what model name the client sends in the request. The bot's hardcoded `"whisper-1"` model name in the request body is harmless — it will be ignored.

### Verification

```bash
curl -X POST http://<whisper-ip>:8000/v1/audio/transcriptions \
  -F "file=@test.ogg" \
  -F "model=whisper-1"
```

### GPU Troubleshooting

If `nvidia-smi` fails inside the LXC:

1. **Check host driver:** Run `nvidia-smi` on the Proxmox host. If it fails there, the host driver needs reinstalling.
2. **Check device nodes:** Run `ls -la /dev/nvidia*` inside the LXC. All devices should be present.
3. **Check library versions:** Run `nvidia-smi` inside the LXC. If it shows a version mismatch, verify the bind-mounted libraries match the host.
4. **Check cgroup permissions:** The major numbers in `lxc.cgroup2.devices.allow` must match `/dev/nvidia*` and `/dev/nvidia-uvm`. Run `stat -c '%t' /dev/nvidia0` (should be `c3` = 195) and `stat -c '%t' /dev/nvidia-uvm` on the host.

---

## Component 2: home-llm-bot LXC

### LXC Specification

| Setting | Value |
|---------|-------|
| Template | Debian 12 (Bookworm) |
| Type | Unprivileged |
| Hostname | `home-llm-bot` |
| CPU | 4 cores |
| RAM | 2048 MB |
| Swap | 1024 MB |
| Disk | 8 GB |
| Network | vmbr0, DHCP or static IP |
| Start on boot | Yes |

**Note on resources:** `cargo build --release` with tokio, teloxide, reqwest, and sqlx can consume 1-2 GB of RAM during linking and 3-4 GB of disk for intermediate build artifacts. After the initial build, the container uses minimal resources at runtime (~50 MB RAM). The Rust toolchain and build cache can be removed post-build to reclaim disk space if desired.

### What Gets Installed Inside

1. **Build dependencies:** `build-essential`, `pkg-config`, `libssl-dev`, `libsqlite3-dev`, `git`, `curl`
2. **Rust toolchain** via rustup (stable)
3. **Dedicated user** `bot` (non-root) owns `/opt/home-llm-bot`
4. **Clone and build:**
   ```bash
   sudo -u bot git clone https://github.com/rafaelcarlosr/home-llm-bot.git /opt/home-llm-bot
   cd /opt/home-llm-bot
   sudo -u bot cargo build --release
   ```
5. **Environment file** at `/opt/home-llm-bot/.env` (permissions: `chmod 600`, owned by `bot`):
   ```env
   TELEGRAM_TOKEN=<prompted during install>
   LM_STUDIO_URL=http://<desktop-ip>:1234
   HOME_ASSISTANT_URL=http://<ha-ip>:8123
   HOME_ASSISTANT_TOKEN=<prompted during install>
   WHISPER_URL=http://<whisper-lxc-ip>:8000
   DATABASE_URL=sqlite:///opt/home-llm-bot/data/bot.db
   LLM_MODEL=qwen2.5-7b-instruct
   ```
6. **systemd service** at `/etc/systemd/system/home-llm-bot.service`:
   ```ini
   [Unit]
   Description=Home LLM Bot - Telegram Bot
   After=network-online.target
   Wants=network-online.target

   [Service]
   Type=simple
   User=bot
   WorkingDirectory=/opt/home-llm-bot
   ExecStart=/opt/home-llm-bot/target/release/home-llm-bot
   Restart=on-failure
   RestartSec=5
   StartLimitBurst=5
   StartLimitIntervalSec=60
   EnvironmentFile=/opt/home-llm-bot/.env

   [Install]
   WantedBy=multi-user.target
   ```

### Data Persistence

- SQLite database at `/opt/home-llm-bot/data/bot.db`
- Conversation history survives restarts
- The `data/` directory is created during install, owned by `bot`

### Updating the Bot

```bash
# SSH into the LXC or use Proxmox console
sudo -u bot bash -c 'cd /opt/home-llm-bot && git pull && cargo build --release'
sudo systemctl restart home-llm-bot
```

### Logs

```bash
journalctl -u home-llm-bot -f
```

---

## Component 3: LM Studio (Desktop)

No deployment script needed. Manual steps:

1. Open LM Studio on desktop
2. Load a model (e.g., Qwen 2.5 7B Instruct)
3. Go to **Developer** tab (or Server tab)
4. Enable **"Serve on Local Network"**
5. Note the URL shown (e.g., `http://192.168.1.100:1234`)

This URL goes into the bot's `LM_STUDIO_URL` environment variable.

---

## Script Design

### Script 1: `create-whisper-lxc.sh`

Run from the Proxmox host shell. **Review the script before running** — never blindly execute `curl | bash`.

```
# Download and inspect first:
curl -fsSL https://raw.githubusercontent.com/rafaelcarlosr/home-llm-bot/main/scripts/create-whisper-lxc.sh -o create-whisper-lxc.sh
less create-whisper-lxc.sh   # review
bash create-whisper-lxc.sh
```

**Flow:**

1. Check that `nvidia-smi` works on the host (exit with instructions if not)
2. Detect NVIDIA UVM cgroup major number dynamically from `/proc/devices`
3. Detect next available CT ID
4. Download Debian 12 template if not cached
5. Prompt for: static IP or DHCP, storage location
6. Create unprivileged LXC with specs above (including `nesting=1`)
7. Append GPU passthrough lines + NVIDIA library bind-mounts to LXC config
8. Start LXC
9. Inside LXC: install Docker CE
10. Run faster-whisper-server Docker container with explicit `--device` flags
11. Pre-download the whisper model inside the container
12. Print summary: IP, port, test command

### Script 2: `create-bot-lxc.sh`

```
# Download and inspect first:
curl -fsSL https://raw.githubusercontent.com/rafaelcarlosr/home-llm-bot/main/scripts/create-bot-lxc.sh -o create-bot-lxc.sh
less create-bot-lxc.sh   # review
bash create-bot-lxc.sh
```

**Flow:**

1. Detect next available CT ID
2. Download Debian 12 template if not cached
3. Prompt for: static IP or DHCP, storage location
4. Prompt for configuration values:
   - `TELEGRAM_TOKEN`
   - `LM_STUDIO_URL` (default: suggest desktop IP detection)
   - `HOME_ASSISTANT_URL`
   - `HOME_ASSISTANT_TOKEN`
   - `WHISPER_URL` (default: suggest whisper LXC IP if created first)
   - `LLM_MODEL` (default: `qwen2.5-7b-instruct`)
5. Create unprivileged LXC with specs above
6. Start LXC
7. Inside LXC: install build deps, Rust toolchain
8. Clone repo, build release as `bot` user
9. Create `bot` user, set up directories, write `.env` (chmod 600)
10. Install systemd service, enable and start
11. Print summary: service status, log command, update instructions

---

## Network Connectivity Matrix

| From | To | Protocol | Port |
|------|----|----------|------|
| home-llm-bot LXC | Telegram API | HTTPS | 443 (outbound) |
| home-llm-bot LXC | LM Studio (desktop) | HTTP | 1234 |
| home-llm-bot LXC | Whisper LXC | HTTP | 8000 |
| home-llm-bot LXC | Home Assistant LXC | HTTP | 8123 |
| Whisper LXC | HuggingFace (first run) | HTTPS | 443 (model download) |

All traffic is on the flat LAN. No firewall rules needed unless Proxmox firewall is enabled (disabled by default). If the Proxmox firewall is enabled, port 8000 must be opened on the Whisper LXC for inbound traffic from the bot LXC.

---

## Deployment Order

1. **First:** Create Whisper LXC (so we know its IP for the bot config)
2. **Second:** Create Bot LXC (uses Whisper IP during setup)
3. **Third:** Enable LM Studio network serving on desktop
4. **Verify:** Send a Telegram message to the bot

---

## Code Changes Required

The following changes to the bot codebase are needed for this deployment:

1. **Update `.env.example`** — Change `WHISPER_URL` port from 9000 to 8000, add `LLM_MODEL`
2. **Update `docker-compose.yml`** — Replace whisper image with `fedirz/faster-whisper-server:latest-cuda` on port 8000 (for developers still using docker-compose locally)

No changes needed to the Rust code — `faster-whisper-server` accepts any model name in the request body and uses its configured model.

---

## Rollback / Cleanup

Each LXC can be destroyed independently:

```bash
# Stop and destroy
pct stop <CTID>
pct destroy <CTID>
```

No shared state between containers. SQLite DB lives inside the bot LXC — back up `/opt/home-llm-bot/data/bot.db` if needed before destroying.

---

## Future Improvements

- **Backup:** Add Proxmox vzdump schedule for the bot LXC (conversation history)
- **Monitoring:** Add health check endpoint to the bot for uptime monitoring
- **Updates:** GitHub webhook or cron to auto-pull and rebuild the bot
- **Model switching:** Environment variable to change Whisper model without recreating the container
