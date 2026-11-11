#!/bin/sh
# Setup and installation script for t1.73code (Ultra Low-Memory T3 Code CLI in Rust)
#
# Usage:
#   ./scripts/setup.sh
#   or:
#   curl -fsSL https://raw.githubusercontent.com/Epickitrolaz/t1.73code/main/scripts/setup.sh | sh
#
# Environment variables:
#   BIN_DIR         Destination directory for installed binaries (default: ~/.local/bin)
#   T3CODE_HOME     Data directory (default: ~/.t3)
#   NO_DOCTOR       Set to 1 to skip running 't1.73code doctor' after installation

set -eu

REPO_URL="https://github.com/Epickitrolaz/t1.73code.git"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
T3_HOME="${T3CODE_HOME:-$HOME/.t3}"

# Terminal color codes if stderr is interactive
interactive=false
if [ -t 2 ] && [ "${TERM:-}" != dumb ]; then interactive=true; fi
reset= bold= muted= accent= green= yellow= red=
if "$interactive" && [ -z "${NO_COLOR:-}" ]; then
  reset="$(printf '\033[0m')"
  bold="$(printf '\033[1m')"
  muted="$(printf '\033[2m')"
  accent="$(printf '\033[94m')"
  green="$(printf '\033[32m')"
  yellow="$(printf '\033[33m')"
  red="$(printf '\033[31m')"
fi

step() {
  printf '  %s%s%s%s\n' "$accent" "==>" "$reset " "$1" >&2
}

info() {
  printf '  %s%s%s\n' "$muted" "$1" "$reset" >&2
}

success() {
  printf '  %s%s %s%s\n' "$green" "✓" "$1" "$reset" >&2
}

warn() {
  printf '  %s%s %s%s\n' "$yellow" "!" "$1" "$reset" >&2
}

fail() {
  printf '  %s%s %s%s\n' "$red" "✗" "$1" "$reset" >&2
  exit 1
}

printf '\n%s' "$bold" >&2
printf '  ============================================================\n' >&2
printf '                     t1.73code Installer                      \n' >&2
printf '     Ultra Low-Memory T3 Code Engine in Rust (Pi Agent)       \n' >&2
printf '  ============================================================\n' >&2
printf '%s\n' "$reset" >&2

# 1. Check or install Rust / Cargo
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  step "Rust not detected. Installing Rust via rustup..."
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  else
    fail "Neither curl nor wget is available to install Rust. Install Rust manually: https://rustup.rs"
  fi
  if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
  fi
fi

command -v cargo >/dev/null 2>&1 || fail "Cargo is required but could not be located."
command -v rustc >/dev/null 2>&1 || fail "Rust compiler is required but could not be located."

RUST_VERSION="$(rustc --version)"
success "Rust toolchain ready: ${RUST_VERSION}"

# 2. Determine source directory or clone if piped
CLEANUP_BUILD_DIR=false
BUILD_DIR=""

if [ -f "Cargo.toml" ] && grep -q "t1_73code" "Cargo.toml" 2>/dev/null; then
  BUILD_DIR="$(pwd)"
elif [ -f "../Cargo.toml" ] && grep -q "t1_73code" "../Cargo.toml" 2>/dev/null; then
  BUILD_DIR="$(cd .. && pwd)"
else
  step "Cloning repository from ${REPO_URL}..."
  TMP_DIR="$(mktemp -d)"
  git clone --depth 1 "${REPO_URL}" "${TMP_DIR}" || fail "Failed to clone repository"
  BUILD_DIR="${TMP_DIR}"
  CLEANUP_BUILD_DIR=true
fi

# 3. Build release binaries
step "Building t1.73code release binaries (Rust optimized profile)..."
(
  cd "${BUILD_DIR}"
  cargo build --release -p t1_73code
) || fail "Compilation failed"

# 4. Install binaries
step "Installing binaries to ${BIN_DIR}..."
mkdir -p "${BIN_DIR}"

SRC_T1="${BUILD_DIR}/target/release/t1_73code"
SRC_T3="${BUILD_DIR}/target/release/t3"

[ -f "${SRC_T1}" ] || fail "Built binary missing: ${SRC_T1}"
[ -f "${SRC_T3}" ] || fail "Built binary missing: ${SRC_T3}"

cp -f "${SRC_T1}" "${BIN_DIR}/t1.73code"
cp -f "${SRC_T1}" "${BIN_DIR}/t1_73code"
cp -f "${SRC_T3}" "${BIN_DIR}/t3"
chmod 755 "${BIN_DIR}/t1.73code" "${BIN_DIR}/t1_73code" "${BIN_DIR}/t3"

if [ "${CLEANUP_BUILD_DIR}" = true ]; then
  rm -rf "${BUILD_DIR}"
fi

success "Installed executables:"
info "  - ${BIN_DIR}/t1.73code (primary)"
info "  - ${BIN_DIR}/t3        (alias)"

# 5. Check PATH
case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    warn "${BIN_DIR} is not currently in your PATH."
    info "Add it to your shell configuration (e.g. ~/.bashrc or ~/.zshrc):"
    info "  export PATH=\"\$PATH:${BIN_DIR}\""
    ;;
esac

# 6. Run diagnostic check
if [ "${NO_DOCTOR:-0}" != "1" ]; then
  printf '\n' >&2
  step "Running health & prerequisite check..."
  "${BIN_DIR}/t1.73code" doctor || true
fi

printf '\n%s' "$green$bold" >&2
printf '  ============================================================\n' >&2
printf '                 t1.73code Installation Complete!             \n' >&2
printf '  ============================================================\n' >&2
printf '%s\n' "$reset" >&2
info "Next steps:"
info "  1. Sign in to T3 Connect with your GitHub account:"
info "     t1.73code connect login"
info ""
info "  2. Start the ultra low-memory server:"
info "     t1.73code up --connect          # with T3 Connect tunnel"
info "     t1.73code up --share            # with Tailscale sharing"
info ""
info "  3. Pair your mobile, desktop, or web app:"
info "     t1.73code pair"
info ""
