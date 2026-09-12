#!/bin/sh
set -eu

# Meno installer — POSIX shell
# curl -fsSL https://boringinfra.company/meno/install.sh | sh
#
# Env overrides:
#   MENO_VERSION   git tag / branch / rev to install (default: default branch HEAD)

REPO="BoringInfraCo/Meno"
CRATE_BIN="meno"
CARGO_BIN_DIR="${HOME}/.cargo/bin"

# ----- helpers -----

RED=''
GREEN=''
NC=''
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  NC='\033[0m'
fi

die() {
  printf "%sError:%s %s\n" "${RED}" "${NC}" "$1" >&2
  exit 1
}

info() {
  printf "%s%s%s\n" "${GREEN}" "$1" "${NC}"
}

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}" in
    Darwin) os="darwin" ;;
    Linux)  os="linux" ;;
    *)      die "Unsupported OS: ${os}. Meno currently supports macOS and Linux." ;;
  esac

  case "${arch}" in
    x86_64|amd64)  arch="x64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) die "Unsupported architecture: ${arch}. Meno supports x86_64 and arm64." ;;
  esac

  echo "${os}-${arch}"
}

ensure_cargo() {
  if command -v cargo > /dev/null 2>&1; then
    return 0
  fi

  printf "cargo not found. Installing Rust toolchain via rustup...\n"
  if ! command -v curl > /dev/null 2>&1; then
    die "curl is required to install Rust. Install curl and retry."
  fi
  if ! curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable; then
    die "rustup installation failed."
  fi
  export PATH="${CARGO_BIN_DIR}:${PATH}"
  if ! command -v cargo > /dev/null 2>&1; then
    die "cargo still not found after rustup install. Add ${CARGO_BIN_DIR} to your PATH and retry."
  fi
}

# ----- install -----

PLATFORM="$(detect_platform)"
printf "Detected %s.\n" "${PLATFORM}"

ensure_cargo

VERSION="${MENO_VERSION:-}"
if [ -n "${VERSION}" ]; then
  info "Installing Meno ${VERSION} from source..."
  # shellcheck disable=SC2086
  if ! cargo install --git "https://github.com/${REPO}" --bin "${CRATE_BIN}" --rev "${VERSION}" --locked; then
    die "cargo install failed for ${REPO} at ${VERSION}."
  fi
else
  info "Installing Meno (latest) from source..."
  if ! cargo install --git "https://github.com/${REPO}" --bin "${CRATE_BIN}" --locked; then
    die "cargo install failed for ${REPO}."
  fi
fi

# ----- verify install -----

if ! command -v "${CRATE_BIN}" > /dev/null 2>&1; then
  if [ -x "${CARGO_BIN_DIR}/${CRATE_BIN}" ]; then
    printf "Installed to %s, but it is not on your PATH:\n" "${CARGO_BIN_DIR}/${CRATE_BIN}"
    printf '  export PATH="%s:$PATH"\n' "${CARGO_BIN_DIR}"
    printf "To make this permanent, add that line to ~/.bashrc, ~/.zshrc, or ~/.profile.\n"
    exit 0
  fi
  die "Install completed but ${CRATE_BIN} was not found. Check cargo output above."
fi

INSTALLED_VERSION="$(${CRATE_BIN} --version 2>/dev/null || echo "${CRATE_BIN} (version unknown)")"
printf "\n%s\n\n" "${INSTALLED_VERSION}"
info "Installed Meno."

# ----- check PATH -----

case ":${PATH}:" in
  *:"${CARGO_BIN_DIR}":*) ;;
  *)
    printf "Add %s to your PATH:\n" "${CARGO_BIN_DIR}"
    printf '  export PATH="%s:$PATH"\n' "${CARGO_BIN_DIR}"
    printf "To make this permanent, add that line to ~/.bashrc, ~/.zshrc, or ~/.profile.\n"
    ;;
esac

printf "\nNext:\n\n"
printf "  meno init\n"
printf "  meno verify\n"
printf "  meno status\n\n"
printf "Docs: https://github.com/BoringInfraCo/Meno\n"
