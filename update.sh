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

heading "Phosphor Updater"

# --- Preflight ---

if [[ ! -f "Cargo.toml" ]] || ! grep -q '\[workspace\]' Cargo.toml; then
    error "Not in the phosphor workspace root. Run from the project directory."
    exit 1
fi

# --- Build ---

heading "Building workspace..."
cargo build --workspace --release
info "Build complete"

# --- Reinstall binaries ---

heading "Installing binaries to ${PREFIX}..."

BINARIES=()
while IFS= read -r bin; do
    BINARIES+=("$bin")
done < <(find target/release -maxdepth 1 \( -name 'phosphor' -o -name 'phosphor-saver-*' \) -type f -executable | grep -v '\.d$')

if [[ ${#BINARIES[@]} -eq 0 ]]; then
    error "No binaries found in target/release/"
    exit 1
fi

SUDO=""
if [[ ! -w "$PREFIX" ]]; then
    SUDO="sudo"
fi

for bin in "${BINARIES[@]}"; do
    $SUDO install -m 755 "$bin" "$PREFIX/"
done

# --- Done ---

heading "Update complete!"
info "${#BINARIES[@]} binaries installed to ${PREFIX}/"
