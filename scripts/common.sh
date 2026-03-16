#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# Color codes
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------
msg() {
    printf "${GREEN}[INFO]${NC} %s\n" "$*"
}

warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$*"
}

err() {
    printf "${RED}[ERROR]${NC} %s\n" "$*" >&2
}

header() {
    printf "${CYAN}═══════════════════════════════════════════════════════════════${NC}\n"
    printf "${CYAN}  %s${NC}\n" "$*"
    printf "${CYAN}═══════════════════════════════════════════════════════════════${NC}\n"
}

# ---------------------------------------------------------------------------
# Proxmox checks
# ---------------------------------------------------------------------------
check_proxmox() {
    if ! command -v pct &>/dev/null; then
        err "pct command not found. This script must be run on a Proxmox host."
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# CT ID detection
# ---------------------------------------------------------------------------
next_ctid() {
    local id=100
    while pct status "$id" &>/dev/null; do
        id=$(( id + 1 ))
    done
    echo "$id"
}

# ---------------------------------------------------------------------------
# Template management
# ---------------------------------------------------------------------------
TEMPLATE_NAME="debian-12-standard_12.7-1_amd64.tar.zst"

ensure_template() {
    local storage="$1"
    if ! pvesm path "${storage}:vztmpl/${TEMPLATE_NAME}" &>/dev/null; then
        msg "Template not found. Updating appliance list..."
        pveam update
        msg "Downloading template ${TEMPLATE_NAME}..."
        pveam download "$storage" "$TEMPLATE_NAME"
    else
        msg "Template ${TEMPLATE_NAME} already present on ${storage}."
    fi
}

# ---------------------------------------------------------------------------
# User prompts
# ---------------------------------------------------------------------------
prompt() {
    local var_name="$1"
    local prompt_text="$2"
    local default="${3:-}"
    local value=""

    while true; do
        if [[ -n "$default" ]]; then
            read -r -p "${prompt_text} [${default}]: " value
            if [[ -z "$value" ]]; then
                value="$default"
            fi
            break
        else
            read -r -p "${prompt_text}: " value
            if [[ -n "$value" ]]; then
                break
            fi
            warn "A value is required. Please try again."
        fi
    done

    eval "${var_name}=\"\${value}\""
}

prompt_ip() {
    local var_name="$1"
    local hostname="$2"
    local choice=""

    printf "\nNetwork configuration for %s:\n" "$hostname"
    printf "  1) DHCP\n"
    printf "  2) Static IP\n"

    while true; do
        read -r -p "Choose [1]: " choice
        if [[ -z "$choice" ]]; then
            choice="1"
        fi
        case "$choice" in
            1)
                eval "${var_name}=\"ip=dhcp\""
                return
                ;;
            2)
                local cidr=""
                local gw=""
                prompt cidr "Enter IP address with CIDR (e.g. 192.168.1.10/24)" ""
                prompt gw   "Enter gateway (e.g. 192.168.1.1)" ""
                eval "${var_name}=\"ip=${cidr},gw=${gw}\""
                return
                ;;
            *)
                warn "Invalid choice. Please enter 1 or 2."
                ;;
        esac
    done
}

# ---------------------------------------------------------------------------
# LXC creation
# ---------------------------------------------------------------------------
create_lxc() {
    local ctid="$1"
    local hostname="$2"
    local storage="$3"
    local net_config="$4"
    local cores="$5"
    local memory="$6"
    local swap="$7"
    local disk_gb="$8"
    local features="${9:-}"

    local template_path
    template_path="${storage}:vztmpl/${TEMPLATE_NAME}"

    local pct_args=(
        "$ctid" "$template_path"
        --hostname   "$hostname"
        --cores      "$cores"
        --memory     "$memory"
        --swap       "$swap"
        --rootfs     "${storage}:${disk_gb}"
        --net0       "name=eth0,bridge=vmbr0,${net_config}"
        --unprivileged 1
        --onboot     1
    )

    if [[ -n "$features" ]]; then
        pct_args+=(--features "$features")
    fi

    msg "Creating LXC container ${ctid} (${hostname})..."
    pct create "${pct_args[@]}"
}

# ---------------------------------------------------------------------------
# Execution helpers
# ---------------------------------------------------------------------------
lxc_exec() {
    local ctid="$1"
    shift
    pct exec "$ctid" -- bash -c "$*"
}

get_lxc_ip() {
    local ctid="$1"
    local elapsed=0
    local ip=""

    msg "Waiting for container ${ctid} to obtain an IP address..."
    while (( elapsed < 30 )); do
        ip=$(pct exec "$ctid" -- bash -c "hostname -I" 2>/dev/null | awk '{print $1}' || true)
        if [[ -n "$ip" ]]; then
            echo "$ip"
            return 0
        fi
        sleep 2
        elapsed=$(( elapsed + 2 ))
    done

    err "Timed out waiting for container ${ctid} to get an IP address."
    return 1
}
