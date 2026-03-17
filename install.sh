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
NO_PAM=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-pam)  NO_PAM=true; shift ;;
        --prefix)  PREFIX="$2"; shift 2 ;;
        --prefix=*) PREFIX="${1#*=}"; shift ;;
        -h|--help)
            echo "Usage: ./install.sh [--no-pam] [--prefix PATH]"
            echo "  --no-pam       Build without PAM support"
            echo "  --prefix PATH  Install directory (default: /usr/local/bin)"
            exit 0
            ;;
        *) error "Unknown option: $1"; exit 1 ;;
    esac
done

heading "Termsaver Installer"

# --- Preflight checks ---

heading "Checking prerequisites..."

if ! command -v cargo &>/dev/null; then
    error "cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi
info "cargo found: $(cargo --version)"

OS="$(uname -s)"
if [[ "$OS" == "Linux" ]] && [[ "$NO_PAM" == false ]]; then
    if ! dpkg -s libpam0g-dev &>/dev/null 2>&1 && ! rpm -q pam-devel &>/dev/null 2>&1; then
        warn "libpam-dev not detected. PAM authentication may fail to build."
        warn "Install it (e.g. sudo apt install libpam0g-dev) or use --no-pam"
    fi
fi

# --- Build ---

heading "Building workspace..."

if [[ "$NO_PAM" == true ]]; then
    # Build host without PAM, then build all savers
    cargo build --release -p termsaver --no-default-features
    cargo build --release --workspace --exclude termsaver
else
    cargo build --workspace --release
fi

info "Build complete"

# --- Install binaries ---

heading "Installing binaries to ${PREFIX}..."

BINARIES=()
while IFS= read -r bin; do
    BINARIES+=("$bin")
done < <(find target/release -maxdepth 1 \( -name 'termsaver' -o -name 'termsaver-saver-*' \) -type f -executable | grep -v '\.d$')

if [[ ${#BINARIES[@]} -eq 0 ]]; then
    error "No binaries found in target/release/"
    exit 1
fi

SUDO=""
if [[ ! -w "$PREFIX" ]]; then
    SUDO="sudo"
fi

$SUDO mkdir -p "$PREFIX"
for bin in "${BINARIES[@]}"; do
    $SUDO install -m 755 "$bin" "$PREFIX/"
done

info "Installed ${#BINARIES[@]} binaries to ${PREFIX}/"

# --- Sysctl (Linux only) ---

if [[ "$OS" == "Linux" ]]; then
    heading "Configuring TIOCSTI..."

    if sysctl dev.tty.legacy_tiocsti &>/dev/null; then
        CURRENT=$(sysctl -n dev.tty.legacy_tiocsti)
        if [[ "$CURRENT" == "0" ]]; then
            $SUDO sysctl -w dev.tty.legacy_tiocsti=1 >/dev/null
            echo "# Enable TIOCSTI for termsaver bash idle detection" | $SUDO tee /etc/sysctl.d/99-termsaver.conf >/dev/null
            echo "dev.tty.legacy_tiocsti = 1" | $SUDO tee -a /etc/sysctl.d/99-termsaver.conf >/dev/null
            info "Enabled TIOCSTI (dev.tty.legacy_tiocsti=1)"
        else
            info "TIOCSTI already enabled"
        fi
    else
        info "Kernel does not gate TIOCSTI — no sysctl needed"
    fi
fi

# --- Shell integration ---

heading "Setting up shell integration..."

SHELL_NAME="$(basename "$SHELL")"
GUARD_START="# >>> termsaver >>>"
GUARD_END="# <<< termsaver <<<"

install_shell_integration() {
    local rc_file="$1"
    local shell_type="$2"

    if [[ ! -f "$rc_file" ]]; then
        touch "$rc_file"
    fi

    if grep -qF "$GUARD_START" "$rc_file"; then
        info "Shell integration already present in ${rc_file}"
        return
    fi

    {
        echo ""
        echo "$GUARD_START"
        echo "eval \"\$(termsaver init ${shell_type})\""
        echo "$GUARD_END"
    } >> "$rc_file"

    info "Added shell integration to ${rc_file}"
}

case "$SHELL_NAME" in
    bash) install_shell_integration "$HOME/.bashrc" "bash" ;;
    zsh)  install_shell_integration "$HOME/.zshrc" "zsh" ;;
    *)    warn "Unsupported shell: $SHELL_NAME. Add manually: eval \"\$(termsaver init bash)\"" ;;
esac

# --- Done ---

heading "Installation complete!"
echo ""
info "${#BINARIES[@]} binaries installed to ${PREFIX}/"
if [[ "$OS" == "Linux" ]]; then
    info "TIOCSTI configured"
fi
info "Shell integration added for ${SHELL_NAME}"
echo ""
echo -e "Restart your shell or run: ${BOLD}source ~/.${SHELL_NAME}rc${NC}"
