#!/bin/sh
# Installer for the fast-yaml CLI (fy).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
#
# Environment variables:
#   FASTYAML_VERSION      Pin to a specific release tag (e.g. "v0.7.0"). Defaults to latest.
#   FASTYAML_INSTALL_DIR   Install directory. Defaults to "$HOME/.local/bin".
set -eu

REPO="bug-ops/fast-yaml"
INSTALL_DIR="${FASTYAML_INSTALL_DIR:-$HOME/.local/bin}"

log() {
    printf '%s\n' "$*" >&2
}

fail() {
    log "error: $*"
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "required command '$1' not found"
}

need_cmd curl
need_cmd tar
need_cmd mktemp

is_musl() {
    # Alpine/musl systems expose a musl dynamic loader at this path; check it
    # first since it works even when ldd is a busybox applet with quirky
    # output. Fall back to grepping `ldd --version`, whose exit code is
    # unreliable across musl versions (busybox ldd exits 1 even on success),
    # so the output text is checked instead of the exit status.
    [ -e /lib/ld-musl-"$(uname -m)".so.1 ] && return 0
    ldd --version 2>&1 | grep -qi musl
}

detect_os() {
    case "$(uname -s)" in
        Linux) echo "linux" ;;
        Darwin) echo "darwin" ;;
        *) fail "unsupported operating system: $(uname -s). See skills/fast-yaml-cli/SKILL.md for manual installation options, or download a prebuilt archive directly from https://github.com/${REPO}/releases/latest" ;;
    esac
}

detect_arch() {
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
        *) fail "unsupported architecture: $arch" ;;
    esac
}

target_triple() {
    os="$1"
    arch="$2"
    libc="$3"
    case "$os" in
        linux)
            case "$libc" in
                musl)
                    case "$arch" in
                        x86_64) echo "x86_64-unknown-linux-musl" ;;
                        *) fail "no prebuilt fy binary is published for musl libc on ${arch} yet (only x86_64-unknown-linux-musl is available). Build from source instead: cargo install fast-yaml-cli (or cargo build -p fast-yaml-cli --release from a checkout)" ;;
                    esac
                    ;;
                *) echo "${arch}-unknown-linux-gnu" ;;
            esac
            ;;
        darwin) echo "${arch}-apple-darwin" ;;
    esac
}

resolve_version() {
    if [ -n "${FASTYAML_VERSION:-}" ]; then
        echo "$FASTYAML_VERSION"
        return
    fi
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' \
        | head -n1 \
        | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
}

checksum_cmd() {
    if command -v sha256sum >/dev/null 2>&1; then
        echo "sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        echo "shasum -a 256"
    else
        fail "neither sha256sum nor shasum is available; refusing to install without checksum verification"
    fi
}

main() {
    os="$(detect_os)"
    arch="$(detect_arch)"

    libc="gnu"
    if [ "$os" = "linux" ] && is_musl; then
        libc="musl"
    fi
    target="$(target_triple "$os" "$arch" "$libc")"

    version="$(resolve_version)"
    [ -n "$version" ] || fail "could not resolve latest release version"
    version_no_v="${version#v}"

    archive="fy-${version}-${target}.tar.gz"
    base_url="https://github.com/${REPO}/releases/download/${version}"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT INT TERM

    log "Downloading fy ${version} (${target})..."
    curl -fsSL -o "${tmpdir}/${archive}" "${base_url}/${archive}" \
        || fail "failed to download ${archive} from release ${version}"
    curl -fsSL -o "${tmpdir}/${archive}.sha256" "${base_url}/${archive}.sha256" \
        || fail "failed to download checksum for ${archive}"

    log "Verifying checksum..."
    sum_tool="$(checksum_cmd)"
    (cd "$tmpdir" && $sum_tool -c "${archive}.sha256") \
        || fail "checksum verification failed for ${archive}"

    log "Extracting..."
    tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

    pkg_dir="${tmpdir}/fy-${version}-${target}"
    [ -f "${pkg_dir}/fy" ] || fail "extracted archive did not contain the fy binary"

    mkdir -p "$INSTALL_DIR"
    cp "${pkg_dir}/fy" "${INSTALL_DIR}/fy"
    chmod +x "${INSTALL_DIR}/fy"

    log "Installed fy ${version_no_v} to ${INSTALL_DIR}/fy"

    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *) log "warning: ${INSTALL_DIR} is not on your PATH. Add it with:"
           log "  export PATH=\"${INSTALL_DIR}:\$PATH\""
           ;;
    esac
}

main
