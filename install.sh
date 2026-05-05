#!/bin/sh
# Install stipe — the Basidiocarp ecosystem manager.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/basidiocarp/stipe/main/install.sh | sh
#
# Override the install directory:
#   MYCELIUM_BIN_DIR=/usr/local/bin curl -fsSL ... | sh
set -eu

REPO="basidiocarp/stipe"
INSTALL_DIR="${MYCELIUM_BIN_DIR:-${HOME}/.local/bin}"

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------
OS=$(uname -s)
ARCH=$(uname -m)

case "${OS}-${ARCH}" in
  Darwin-arm64)   PLATFORM="aarch64-apple-darwin" ;;
  Darwin-x86_64)  PLATFORM="x86_64-apple-darwin" ;;
  Linux-x86_64)   PLATFORM="x86_64-unknown-linux-musl" ;;
  Linux-aarch64)  PLATFORM="aarch64-unknown-linux-musl" ;;
  *)
    echo "Unsupported platform: ${OS}-${ARCH}" >&2
    echo "Download a release manually from https://github.com/${REPO}/releases" >&2
    exit 1
    ;;
esac

# ---------------------------------------------------------------------------
# Fetch latest release version
# ---------------------------------------------------------------------------
printf 'Fetching latest release...\n'

API_HEADERS="-H 'Accept: application/vnd.github.v3+json'"
if [ -n "${GH_TOKEN:-}" ]; then
  API_HEADERS="${API_HEADERS} -H 'Authorization: Bearer ${GH_TOKEN}'"
elif [ -n "${GITHUB_TOKEN:-}" ]; then
  API_HEADERS="${API_HEADERS} -H 'Authorization: Bearer ${GITHUB_TOKEN}'"
fi

RELEASE_JSON=$(curl -fsSL \
  -H "Accept: application/vnd.github.v3+json" \
  ${GH_TOKEN:+-H "Authorization: Bearer ${GH_TOKEN}"} \
  ${GITHUB_TOKEN:+-H "Authorization: Bearer ${GITHUB_TOKEN}"} \
  "https://api.github.com/repos/${REPO}/releases/latest")

VERSION=$(printf '%s' "$RELEASE_JSON" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$VERSION" ]; then
  printf 'Failed to determine latest release version.\n' >&2
  printf 'Check https://github.com/%s/releases for the latest tag.\n' "$REPO" >&2
  exit 1
fi

printf 'Latest: %s\n' "$VERSION"

# ---------------------------------------------------------------------------
# Download
# ---------------------------------------------------------------------------
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
ARCHIVE="stipe-${PLATFORM}.tar.gz"

TMP=$(mktemp -d)
# shellcheck disable=SC2064
trap "rm -rf '${TMP}'" EXIT

printf 'Downloading %s...\n' "$ARCHIVE"
curl -fsSL --progress-bar -o "${TMP}/${ARCHIVE}" "${BASE_URL}/${ARCHIVE}"
curl -fsSL -o "${TMP}/SHA256SUMS" "${BASE_URL}/SHA256SUMS"

# ---------------------------------------------------------------------------
# Checksum verification
# ---------------------------------------------------------------------------
printf 'Verifying checksum...\n'

EXPECTED=$(grep "${ARCHIVE}" "${TMP}/SHA256SUMS" | awk '{print $1}')
if [ -z "$EXPECTED" ]; then
  printf 'No checksum entry found for %s in SHA256SUMS.\n' "$ARCHIVE" >&2
  exit 1
fi

if command -v sha256sum > /dev/null 2>&1; then
  ACTUAL=$(sha256sum "${TMP}/${ARCHIVE}" | awk '{print $1}')
else
  ACTUAL=$(shasum -a 256 "${TMP}/${ARCHIVE}" | awk '{print $1}')
fi

if [ "$ACTUAL" != "$EXPECTED" ]; then
  printf 'Checksum mismatch!\n' >&2
  printf '  expected: %s\n' "$EXPECTED" >&2
  printf '  actual:   %s\n' "$ACTUAL" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Extract and install
# ---------------------------------------------------------------------------
printf 'Extracting...\n'
tar -xzf "${TMP}/${ARCHIVE}" -C "$TMP"

mkdir -p "$INSTALL_DIR"
chmod +x "${TMP}/stipe"
cp "${TMP}/stipe" "${INSTALL_DIR}/stipe"

# On macOS, clear the Gatekeeper quarantine attribute added to downloaded files.
if [ "$OS" = "Darwin" ] && command -v xattr > /dev/null 2>&1; then
  xattr -d com.apple.quarantine "${INSTALL_DIR}/stipe" 2>/dev/null || true
fi

printf '\nstipe %s installed to %s/stipe\n' "$VERSION" "$INSTALL_DIR"

# ---------------------------------------------------------------------------
# PATH hint
# ---------------------------------------------------------------------------
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf '\n%s is not in your PATH.\n' "$INSTALL_DIR"
    printf 'Add this to your shell profile (~/.zshrc, ~/.bashrc, etc.):\n\n'
    printf '  export PATH="%s:$PATH"\n\n' "$INSTALL_DIR"
    printf 'Then open a new terminal, or run:\n\n'
    printf '  source ~/.zshrc\n\n'
    ;;
esac

printf 'Run '\''stipe setup'\'' to install ecosystem tools and configure your AI host.\n'
