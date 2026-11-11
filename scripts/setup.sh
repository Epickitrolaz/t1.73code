#!/bin/sh
# Setup and installation script for t1.73code (Ultra Low-Memory T3 Code CLI in Rust)
#
# Usage:
#   ./scripts/setup.sh                         # Install pre-compiled binary (~7MB) if available, or build from source
#   ./scripts/setup.sh --binary                # Install pre-compiled release binary directly (no Rust needed)
#   ./scripts/setup.sh --build-from-source     # Compile release binaries from source via Cargo
#   or:
#   curl -fsSL https://raw.githubusercontent.com/Epickitrolaz/t1.73code/main/scripts/setup.sh | sh
#
# Environment variables:
#   BIN_DIR            Destination directory for installed binaries (default: ~/.local/bin)
#   T3CODE_HOME        Data directory (default: ~/.t3)
#   BUILD_FROM_SOURCE  Set to 1 to force compiling from source with Cargo
#   INSTALL_BINARY     Set to 1 to force installing pre-compiled binary
#   NO_DOCTOR          Set to 1 to skip running 't1.73code doctor' after installation

set -eu

REPO_URL="https://github.com/Epickitrolaz/t1.73code.git"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
T3_HOME="${T3CODE_HOME:-$HOME/.t3}"

MODE="auto"
if [ "${BUILD_FROM_SOURCE:-0}" = "1" ]; then
  MODE="source"
elif [ "${INSTALL_BINARY:-0}" = "1" ]; then
  MODE="binary"
fi

for arg in "$@"; do
  case "$arg" in
    --binary|-b)
      MODE="binary"
      ;;
    --source|--build-from-source|-s)
      MODE="source"
      ;;
    --help|-h)
      echo "Usage: ./scripts/setup.sh [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  -b, --binary             Install pre-compiled release binary (~7MB, no Rust toolchain needed)"
      echo "  -s, --build-from-source  Build and compile from source using Rust/Cargo"
      echo "  -h, --help               Show this help message"
      echo ""
      echo "Environment variables:"
      echo "  BIN_DIR                  Destination directory for binaries (default: ~/.local/bin)"
      echo "  T3CODE_HOME              Data directory (default: ~/.t3)"
      echo "  BUILD_FROM_SOURCE        Force compile from source (set to 1)"
      echo "  INSTALL_BINARY           Force install pre-compiled binary (set to 1)"
      echo "  NO_DOCTOR                Set to 1 to skip running 't1.73code doctor'"
      exit 0
      ;;
    *)
      ;;
  esac
done

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

try_install_binary() {
  step "Checking for pre-compiled release binary (~7 MB static executable)..."
  info "Pre-compiled binaries run directly with ~10 MB RSS and require no Rust toolchain."

  case "$(uname -s)" in
    Darwin) platform="darwin" ;;
    Linux) platform="linux" ;;
    *) return 1 ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) arch="arm64" ;;
    x86_64 | amd64) arch="x64" ;;
    *) return 1 ;;
  esac

  # If install.sh exists locally beside setup.sh, use it
  SETUP_DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)"
  if [ -n "$SETUP_DIR" ] && [ -f "${SETUP_DIR}/install.sh" ]; then
    if T3CODE_INSTALL_BIN_DIR="${BIN_DIR}" T3CODE_HOME="${T3_HOME}" NO_DOCTOR=1 "${SETUP_DIR}/install.sh"; then
      return 0
    fi
  fi

  mkdir -p "${BIN_DIR}"
  VERSION="0.1.73"
  RELEASE_URL="https://github.com/Epickitrolaz/t1.73code/releases/download/v${VERSION}"
  LATEST_URL="https://github.com/Epickitrolaz/t1.73code/releases/latest/download"

  TMP_DL="$(mktemp -d)"
  DL_OK=false
  ARCHIVE="t1.73code-${VERSION}-${platform}-${arch}.tar.gz"

  if command -v curl >/dev/null 2>&1; then
    if curl -fsSL "${RELEASE_URL}/${ARCHIVE}" -o "${TMP_DL}/${ARCHIVE}" 2>/dev/null || \
       curl -fsSL "${LATEST_URL}/${ARCHIVE}" -o "${TMP_DL}/${ARCHIVE}" 2>/dev/null; then
      tar -xzf "${TMP_DL}/${ARCHIVE}" -C "${TMP_DL}" 2>/dev/null || tar -xzf "${TMP_DL}/${ARCHIVE}" -C "${TMP_DL}" --strip-components=1 2>/dev/null
      DL_OK=true
    elif curl -fsSL "${RELEASE_URL}/t1.73code-${platform}-${arch}" -o "${TMP_DL}/t1.73code" 2>/dev/null || \
         curl -fsSL "${LATEST_URL}/t1.73code-${platform}-${arch}" -o "${TMP_DL}/t1.73code" 2>/dev/null; then
      DL_OK=true
    fi
  elif command -v wget >/dev/null 2>&1; then
    if wget -q "${RELEASE_URL}/${ARCHIVE}" -O "${TMP_DL}/${ARCHIVE}" 2>/dev/null || \
       wget -q "${LATEST_URL}/${ARCHIVE}" -O "${TMP_DL}/${ARCHIVE}" 2>/dev/null; then
      tar -xzf "${TMP_DL}/${ARCHIVE}" -C "${TMP_DL}" 2>/dev/null || tar -xzf "${TMP_DL}/${ARCHIVE}" -C "${TMP_DL}" --strip-components=1 2>/dev/null
      DL_OK=true
    elif wget -q "${RELEASE_URL}/t1.73code-${platform}-${arch}" -O "${TMP_DL}/t1.73code" 2>/dev/null || \
         wget -q "${LATEST_URL}/t1.73code-${platform}-${arch}" -O "${TMP_DL}/t1.73code" 2>/dev/null; then
      DL_OK=true
    fi
  fi

  if [ "$DL_OK" = true ]; then
    TARGET_BIN=""
    if [ -f "${TMP_DL}/t1.73code" ]; then
      TARGET_BIN="${TMP_DL}/t1.73code"
    elif [ -f "${TMP_DL}/t3" ]; then
      TARGET_BIN="${TMP_DL}/t3"
    fi
    if [ -n "$TARGET_BIN" ]; then
      cp -f "${TARGET_BIN}" "${BIN_DIR}/t1.73code"
      cp -f "${TARGET_BIN}" "${BIN_DIR}/t1_73code"
      cp -f "${TARGET_BIN}" "${BIN_DIR}/t3"
      chmod 755 "${BIN_DIR}/t1.73code" "${BIN_DIR}/t1_73code" "${BIN_DIR}/t3"
      rm -rf "${TMP_DL}"
      success "Installed pre-compiled binary to ${BIN_DIR}/t1.73code"
      return 0
    fi
  fi

  rm -rf "${TMP_DL}"
  return 1
}

INSTALLED=false

if [ "$MODE" = "binary" ]; then
  if try_install_binary; then
    INSTALLED=true
  else
    fail "Pre-compiled release binary could not be found or downloaded for this platform.\n  To build from source using Cargo, run: ./scripts/setup.sh --build-from-source"
  fi
elif [ "$MODE" = "auto" ]; then
  if try_install_binary; then
    INSTALLED=true
  else
    info "Pre-compiled binary was not found or could not be downloaded."
    info "Proceeding with compilation from source via Cargo..."
  fi
fi

if [ "$INSTALLED" = false ]; then
  # 1. Warn if RAM is constrained for compilation
  if [ -f /proc/meminfo ]; then
    mem_kb="$(grep MemTotal /proc/meminfo | awk '{print $2}' || true)"
    if [ -n "$mem_kb" ] && [ "$mem_kb" -lt 1048576 ]; then
      mem_mb=$((mem_kb / 1024))
      warn "Detected ${mem_mb} MB RAM. Compiling from source may exhaust memory on systems < 1GB RAM."
      info "If cargo is killed by OOM, consider configuring swap or using a pre-compiled binary."
    fi
  fi

  # 2. Check or install Rust / Cargo
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

  # 3. Determine source directory or clone if piped
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

  # 4. Build release binaries
  step "Building t1.73code release binaries (Rust optimized profile)..."
  (
    cd "${BUILD_DIR}"
    cargo build --release -p t1_73code
  ) || fail "Compilation failed"

  # 5. Install binaries
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
fi

success "Installed executables:"
info "  - ${BIN_DIR}/t1.73code (primary)"
info "  - ${BIN_DIR}/t3        (alias)"

# Check PATH
case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    warn "${BIN_DIR} is not currently in your PATH."
    info "Add it to your shell configuration (e.g. ~/.bashrc or ~/.zshrc):"
    info "  export PATH=\"\$PATH:${BIN_DIR}\""
    ;;
esac

# Run diagnostic check
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
