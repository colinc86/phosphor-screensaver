#!/usr/bin/env bash
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${GREEN}[✓]${NC} $*"; }
warn()    { echo -e "${YELLOW}[!]${NC} $*"; }
error()   { echo -e "${RED}[✗]${NC} $*"; }
heading() { echo -e "\n${BOLD}$*${NC}"; }

PREFIX="/usr/local/bin"
PURGE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --purge) PURGE=true; shift ;;
        -h|--help)
            echo "Usage: ./uninstall.sh [--purge]"
            echo "  --purge  Also remove config and data directories"
            exit 0
            ;;
        *) error "Unknown option: $1"; exit 1 ;;
    esac
done

heading "Termsaver Uninstaller"

SUDO=""
if [[ ! -w "$PREFIX" ]]; then
    SUDO="sudo"
fi

OS="$(uname -s)"

# --- Remove binaries ---

heading "Removing binaries..."

REMOVED=0
for bin in "$PREFIX"/termsaver "$PREFIX"/termsaver-saver-*; do
    if [[ -f "$bin" ]]; then
        $SUDO rm "$bin"
        REMOVED=$((REMOVED + 1))
    fi
done

if [[ $REMOVED -gt 0 ]]; then
    info "Removed ${REMOVED} binaries from ${PREFIX}/"
else
    warn "No termsaver binaries found in ${PREFIX}/"
fi

# --- Remove sysctl (Linux only) ---

if [[ "$OS" == "Linux" ]]; then
    heading "Removing sysctl configuration..."

    SYSCTL_CONF="/etc/sysctl.d/99-termsaver.conf"
    if [[ -f "$SYSCTL_CONF" ]]; then
        $SUDO rm -f "$SYSCTL_CONF"
        if sysctl dev.tty.legacy_tiocsti &>/dev/null; then
            $SUDO sysctl -w dev.tty.legacy_tiocsti=0 >/dev/null
        fi
        info "Removed sysctl configuration and disabled TIOCSTI"
    else
        info "No sysctl configuration found"
    fi
fi

# --- Remove shell integration ---

heading "Removing shell integration..."

GUARD_START="# >>> termsaver >>>"
GUARD_END="# <<< termsaver <<<"

remove_shell_integration() {
    local rc_file="$1"

    if [[ ! -f "$rc_file" ]]; then
        return
    fi

    if ! grep -qF "$GUARD_START" "$rc_file"; then
        return
    fi

    # Remove the guard block (start marker through end marker)
    sed -i "/${GUARD_START//\//\\/}/,/${GUARD_END//\//\\/}/d" "$rc_file"
    # Remove trailing blank line left behind
    sed -i -e :a -e '/^\n*$/{$d;N;ba' -e '}' "$rc_file"
    info "Removed shell integration from ${rc_file}"
}

remove_shell_integration "$HOME/.bashrc"
remove_shell_integration "$HOME/.zshrc"

# --- Config and data ---

heading "Configuration and data..."

CONFIG_DIR="$HOME/.config/termsaver"
DATA_DIR="$HOME/.local/share/termsaver"

HAS_DATA=false
[[ -d "$CONFIG_DIR" ]] && HAS_DATA=true
[[ -d "$DATA_DIR" ]] && HAS_DATA=true

if [[ "$HAS_DATA" == true ]]; then
    if [[ "$PURGE" == true ]]; then
        rm -rf "$CONFIG_DIR" "$DATA_DIR"
        info "Removed config and data directories"
    else
        echo -n "Remove config and data directories? [y/N] "
        read -r REPLY
        if [[ "$REPLY" =~ ^[Yy]$ ]]; then
            rm -rf "$CONFIG_DIR" "$DATA_DIR"
            info "Removed config and data directories"
        else
            info "Kept config and data directories"
        fi
    fi
else
    info "No config or data directories found"
fi

# --- Done ---

heading "Uninstall complete!"
