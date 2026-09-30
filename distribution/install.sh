#!/bin/sh
# selfhost installer — https://cli.selfhost.dev
#
# Usage:
#   curl -fsSL https://cli.selfhost.dev/install.sh | sh
#
# Environment overrides:
#   SELFHOST_INSTALL_DIR   install directory (default: ~/.local/bin)
#   SELFHOST_MANIFEST_URL  manifest location (default: https://cli.selfhost.dev/latest.json)
#
# POSIX sh; needs curl and one of sha256sum/shasum/openssl.

set -eu

BIN="selfhost"
MANIFEST_URL="${SELFHOST_MANIFEST_URL:-https://cli.selfhost.dev/latest.json}"
INSTALL_DIR="${SELFHOST_INSTALL_DIR:-$HOME/.local/bin}"

log()  { printf '  \033[32m>\033[0m %s\n' "$1"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$1"; }
err()  { printf '  \033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "requires '$1' — install it first, or download a binary manually from https://github.com/selfhost-dev/selfhost-cli/releases"
    fi
}

# Extract a top-level JSON string field with awk: the manifest is small and
# flat, and awk ships everywhere while jq does not. `depth` guards against a
# nested object with the same key shadowing the real one: only lines at the
# given indentation are considered.
manifest_field() {
    _mf_manifest="$1"; _mf_key="$2"; _mf_depth="$3"
    printf '%s\n' "$_mf_manifest" | awk -v key="\"${_mf_key}\"" -v depth="${_mf_depth}" '
        {
            # count leading spaces; depth 0 = top level (no indent)
            indent = 0
            while (substr($0, indent + 1, 1) == " ") indent++
            if (indent != depth * 2) next
            if (index($0, key ":") != 0) {
                sub(/^.*:[[:space:]]*"/, "")
                sub(/".*$/, "")
                print
                exit
            }
        }
    '
}

main() {
    echo ""
    echo "       selfhost installer"
    echo "       cli.selfhost.dev"
    echo ""

    # detect platform
    OS="$(uname -s)"
    case "$OS" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      err "unsupported OS: $OS" ;;
    esac

    if [ "$OS" = "Linux" ] && [ "$(uname -o 2>/dev/null || true)" = "Android" ]; then
        err "Android/Termux is not supported by the release binaries. Download the source from https://github.com/selfhost-dev/selfhost-cli instead."
    fi

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              err "unsupported architecture: $ARCH" ;;
    esac

    log "detected ${os}/${arch}"

    need curl
    need awk

    TARGET="${os}-${arch}"

    log "fetching release manifest..."
    MANIFEST="$(curl -fsSL --retry 3 --connect-timeout 10 --max-time 20 "$MANIFEST_URL")" \
        || err "can't reach ${MANIFEST_URL}. Please try again later."

    URL="$(manifest_field "$MANIFEST" assets 1)"
    # assets is an object of platform -> url; pick our platform's line
    URL="$(printf '%s\n' "$MANIFEST" | awk -v target="\"${TARGET}\"" '
        /^[[:space:]]*"assets"[[:space:]]*:/ { in_assets = 1; next }
        in_assets && /^[[:space:]]*}/ { exit }
        in_assets && index($0, target) {
            sub(/^.*:[[:space:]]*"/, "")
            sub(/".*$/, "")
            print
            exit
        }
    ')"
    SHA256="$(printf '%s\n' "$MANIFEST" | awk -v target="\"${TARGET}\"" '
        /^[[:space:]]*"sha256"[[:space:]]*:/ { in_sha256 = 1; next }
        in_sha256 && /^[[:space:]]*}/ { exit }
        in_sha256 && index($0, target) {
            sub(/^.*:[[:space:]]*"/, "")
            sub(/".*$/, "")
            print
            exit
        }
    ')"
    VERSION="$(manifest_field "$MANIFEST" version 0)"

    if [ -z "$URL" ]; then
        err "release manifest does not include a binary for ${TARGET}"
    fi
    # Origin pin: the manifest decides *which* release, never *where from*.
    # A tampered manifest must not redirect the download off cli.selfhost.dev
    # with a self-consistent checksum.
    case "$URL" in
        https://cli.selfhost.dev/*) ;;
        *) err "manifest points at an unexpected location: ${URL}" ;;
    esac
    if [ "${#SHA256}" -ne 64 ]; then
        err "release manifest does not include a valid SHA-256 checksum for ${TARGET}"
    fi
    if ! printf '%s\n' "$SHA256" | awk '/[^0-9A-Fa-f]/ { exit 1 }'; then
        err "release manifest does not include a valid SHA-256 checksum for ${TARGET}"
    fi
    SHA256="$(printf '%s\n' "$SHA256" | awk '{ print tolower($0) }')"

    if command -v sha256sum >/dev/null 2>&1; then
        SHA256_TOOL="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        SHA256_TOOL="shasum"
    elif command -v openssl >/dev/null 2>&1; then
        SHA256_TOOL="openssl"
    else
        err "SHA-256 verification requires sha256sum, shasum, or openssl"
    fi

    if [ -n "$VERSION" ]; then
        log "downloading v${VERSION}..."
    else
        log "downloading latest release..."
    fi
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    if ! curl -fsSL --retry 3 --connect-timeout 10 --max-time 120 "$URL" -o "${TMP}/${BIN}"; then
        err "download failed from ${URL}"
    fi

    case "$SHA256_TOOL" in
        sha256sum) ACTUAL_SHA256="$(sha256sum < "${TMP}/${BIN}" | awk '{ print $1 }')" ;;
        shasum)    ACTUAL_SHA256="$(shasum -a 256 < "${TMP}/${BIN}" | awk '{ print $1 }')" ;;
        openssl)   ACTUAL_SHA256="$(openssl dgst -sha256 < "${TMP}/${BIN}" | awk '{ print $NF }')" ;;
    esac
    if [ "$ACTUAL_SHA256" != "$SHA256" ]; then
        err "downloaded selfhost checksum did not match"
    fi
    log "checksum verified"

    # install
    mkdir -p "$INSTALL_DIR"
    mv "${TMP}/${BIN}" "${INSTALL_DIR}/${BIN}"
    chmod 0755 "${INSTALL_DIR}/${BIN}"

    log "installed ${BIN} to ${INSTALL_DIR}/${BIN}"

    # check PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            warn "${INSTALL_DIR} is not in your PATH"
            echo "  add it to your shell config:"
            echo ""
            echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
            echo ""
            ;;
    esac

    # verify
    if command -v "$BIN" >/dev/null 2>&1; then
        echo ""
        log "ready. run 'selfhost' to get started."
    fi

    echo ""
}

main "$@"
