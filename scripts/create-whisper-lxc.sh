#!/usr/bin/env bash
# create-whisper-lxc.sh — Create a Proxmox LXC for faster-whisper-server with NVIDIA GPU passthrough
# Run from the Proxmox host shell: bash scripts/create-whisper-lxc.sh
set -euo pipefail

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
check_proxmox

if ! command -v nvidia-smi &>/dev/null; then
    err "nvidia-smi not found. NVIDIA drivers must be installed on the Proxmox host."
    err "Install with: apt-get install -y nvidia-driver firmware-misc-nonfree"
    exit 1
fi

DRIVER_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 || true)
if [[ -z "$DRIVER_VERSION" ]]; then
    err "nvidia-smi is present but failed to query driver version. Check GPU status."
    exit 1
fi
msg "NVIDIA driver version: ${DRIVER_VERSION}"

# Detect UVM cgroup major number dynamically (kernel assigns it at boot)
NVIDIA_UVM_MAJOR=$(grep nvidia-uvm /proc/devices 2>/dev/null | awk '{print $1}' || true)
if [[ -z "$NVIDIA_UVM_MAJOR" ]]; then
    err "nvidia-uvm device not found in /proc/devices."
    err "Load the module with: modprobe nvidia-uvm"
    exit 1
fi
msg "Detected nvidia-uvm cgroup major: ${NVIDIA_UVM_MAJOR}"

# ---------------------------------------------------------------------------
# Prompt for configuration
# ---------------------------------------------------------------------------
header "Whisper STT LXC Configuration"

DEFAULT_CTID=$(next_ctid)
prompt CTID       "Container ID"       "$DEFAULT_CTID"
prompt STORAGE    "Storage location"   "local-lvm"
prompt_ip NET_CONFIG "whisper-stt"

# ---------------------------------------------------------------------------
# Create LXC
# ---------------------------------------------------------------------------
ensure_template "$STORAGE"

msg "Creating Whisper STT LXC..."
create_lxc "$CTID" "whisper-stt" "$STORAGE" "$NET_CONFIG" 4 4096 512 16 "nesting=1"

# ---------------------------------------------------------------------------
# Append GPU passthrough config to LXC conf
# ---------------------------------------------------------------------------
LXC_CONF="/etc/pve/lxc/${CTID}.conf"
msg "Appending GPU passthrough config to ${LXC_CONF}..."

cat >> "$LXC_CONF" << EOF

# NVIDIA GPU passthrough
lxc.cgroup2.devices.allow: c 195:* rwm
lxc.cgroup2.devices.allow: c ${NVIDIA_UVM_MAJOR}:* rwm
lxc.mount.entry: /dev/nvidia0 dev/nvidia0 none bind,optional,create=file
lxc.mount.entry: /dev/nvidiactl dev/nvidiactl none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm dev/nvidia-uvm none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm-tools dev/nvidia-uvm-tools none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-caps dev/nvidia-caps none bind,optional,create=dir
EOF

# ---------------------------------------------------------------------------
# Auto-detect and bind-mount host NVIDIA libraries
# ---------------------------------------------------------------------------
msg "Detecting and bind-mounting host NVIDIA libraries..."

MULTIARCH=$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || echo 'x86_64-linux-gnu')
LIB_DIR="/usr/lib/${MULTIARCH}"

# Collect candidate library paths (may include symlinks)
declare -a NVIDIA_LIBS=()
while IFS= read -r -d '' lib; do
    NVIDIA_LIBS+=("$lib")
done < <(find "$LIB_DIR" -maxdepth 1 -name "libnvidia-*.so*" -print0 2>/dev/null)

for extra_lib in \
    "${LIB_DIR}/libcuda.so.1" \
    "${LIB_DIR}/libnvcuvid.so.1" \
    "${LIB_DIR}/libnvidia-encode.so.1" \
    "/usr/bin/nvidia-smi"; do
    if [[ -f "$extra_lib" ]]; then
        NVIDIA_LIBS+=("$extra_lib")
    fi
done

BIND_COUNT=0
for lib in "${NVIDIA_LIBS[@]}"; do
    # Resolve symlinks: mount the real file at the original (expected) path inside the container.
    # LXC bind-mount of a symlink is unreliable — use the resolved target as the source.
    real_lib=$(readlink -f "$lib" 2>/dev/null || true)
    if [[ -z "$real_lib" ]] || [[ ! -f "$real_lib" ]]; then
        continue
    fi
    container_path="${lib#/}"
    echo "lxc.mount.entry: ${real_lib} ${container_path} none bind,optional,create=file" >> "$LXC_CONF"
    BIND_COUNT=$(( BIND_COUNT + 1 ))
done

msg "Bind-mounted ${BIND_COUNT} NVIDIA library/binary entries."

# ---------------------------------------------------------------------------
# Append Docker GPU lib directory to LXC conf
# ---------------------------------------------------------------------------
# Docker containers expect NVIDIA libs at /usr/local/nvidia/lib64.
# We create /opt/nvidia-libs inside the LXC (after start) and mount
# it there. This avoids the segfault caused by bind-mounting the entire
# system lib dir into the Docker container.

# ---------------------------------------------------------------------------
# Start LXC and wait for IP
# ---------------------------------------------------------------------------
msg "Starting LXC container ${CTID}..."
pct start "$CTID"

LXC_IP=$(get_lxc_ip "$CTID")
msg "Container IP: ${LXC_IP}"

# ---------------------------------------------------------------------------
# Install Docker CE inside the LXC (official Debian Bookworm repo)
# ---------------------------------------------------------------------------
msg "Installing Docker CE inside the container..."

pct exec "$CTID" -- bash -c "apt-get update -qq"
pct exec "$CTID" -- bash -c "apt-get install -y ca-certificates curl gnupg"
pct exec "$CTID" -- bash -c "install -m 0755 -d /etc/apt/keyrings"
pct exec "$CTID" -- bash -c "curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg && chmod a+r /etc/apt/keyrings/docker.gpg"
DOCKER_ARCH=$(dpkg --print-architecture 2>/dev/null || echo 'amd64')
pct exec "$CTID" -- bash -c "echo 'deb [arch=${DOCKER_ARCH} signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian bookworm stable' > /etc/apt/sources.list.d/docker.list"
pct exec "$CTID" -- bash -c "apt-get update -qq && apt-get install -y docker-ce docker-ce-cli containerd.io"
pct exec "$CTID" -- bash -c "systemctl enable docker && systemctl start docker"

msg "Docker CE installed successfully."

# ---------------------------------------------------------------------------
# Build /opt/nvidia-libs: targeted NVIDIA driver libs for Docker GPU access
# ---------------------------------------------------------------------------
# Docker containers expect NVIDIA driver libs at /usr/local/nvidia/lib64
# (set via LD_LIBRARY_PATH in the image). We create a small directory with
# only the needed driver libs (NOT the full system lib dir, which causes
# segfaults by overwriting the container's own shared libraries).
msg "Building /opt/nvidia-libs for Docker GPU passthrough..."
pct exec "$CTID" -- bash -c "
  mkdir -p /opt/nvidia-libs
  LIB_DIR='/usr/lib/${MULTIARCH}'
  cp -L \"\${LIB_DIR}/libcuda.so.1\" /opt/nvidia-libs/ 2>/dev/null || true
  cp -L \"\${LIB_DIR}/libnvidia-ml.so.1\" /opt/nvidia-libs/ 2>/dev/null || true
  cp -L \$(ls \"\${LIB_DIR}/libnvidia-ptxjitcompiler.so\"* 2>/dev/null | head -1) /opt/nvidia-libs/ 2>/dev/null || true
  cp -L \$(ls \"\${LIB_DIR}/libnvidia-nvvm.so\"* 2>/dev/null | head -1) /opt/nvidia-libs/ 2>/dev/null || true
  echo \"Libs in /opt/nvidia-libs:\"; ls /opt/nvidia-libs/
"

# ---------------------------------------------------------------------------
# Verify nvidia-smi inside the LXC
# ---------------------------------------------------------------------------
msg "Verifying nvidia-smi inside container..."
pct exec "$CTID" -- bash -c "nvidia-smi" || warn "nvidia-smi failed inside LXC — check GPU passthrough config. See spec for troubleshooting steps."

# ---------------------------------------------------------------------------
# Run faster-whisper-server container
# ---------------------------------------------------------------------------
msg "Starting faster-whisper-server Docker container..."

pct exec "$CTID" -- bash -c "docker run -d \
  --name whisper \
  --restart unless-stopped \
  --device /dev/nvidia0 \
  --device /dev/nvidiactl \
  --device /dev/nvidia-uvm \
  --device /dev/nvidia-uvm-tools \
  -p 8000:8000 \
  -v whisper-models:/root/.cache/huggingface \
  -v /opt/nvidia-libs:/usr/local/nvidia/lib64 \
  -e WHISPER__MODEL=deepdml/faster-whisper-large-v3-turbo-ct2 \
  -e WHISPER__DEVICE=cuda \
  fedirz/faster-whisper-server:latest-cuda"

# ---------------------------------------------------------------------------
# Pre-download the Whisper model
# ---------------------------------------------------------------------------
msg "Pre-downloading Whisper model (deepdml/faster-whisper-large-v3-turbo-ct2)..."
msg "This may take several minutes on first run..."

pct exec "$CTID" -- bash -c \
  "docker exec whisper /root/faster-whisper-server/.venv/bin/python3 -c \"from faster_whisper import WhisperModel; WhisperModel('deepdml/faster-whisper-large-v3-turbo-ct2', device='cuda')\"" \
  || warn "Model pre-download failed — model will download on first request."

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
header "Whisper STT LXC Ready"
printf "\n"
msg "CT ID:      ${CTID}"
msg "IP Address: ${LXC_IP}"
msg "Port:       8000"
msg "Model:      deepdml/faster-whisper-large-v3-turbo-ct2"
printf "\n"
msg "Test command:"
printf "  curl -X POST http://%s:8000/v1/audio/transcriptions \\\\\n" "$LXC_IP"
printf "    -F \"file=@test.ogg\" \\\\\n"
printf "    -F \"model=whisper-1\"\n"
printf "\n"
msg "Add to bot .env:"
printf "  WHISPER_URL=http://%s:8000\n" "$LXC_IP"
printf "\n"
