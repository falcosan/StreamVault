#!/usr/bin/env bash
set -euo pipefail

REPO="https://github.com/falcosan/StreamVault.git"
REMOTE_INSTALL=false
RUST_INSTALLED_BY_SCRIPT=false

if [[ "${BASH_SOURCE[0]:-}" == "" || "$(basename "${BASH_SOURCE[0]:-bash}")" == "bash" ]]; then
  REMOTE_INSTALL=true
  TMPDIR_SV="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR_SV"' EXIT
  git clone --depth 1 "$REPO" "$TMPDIR_SV/StreamVault"
  P="$TMPDIR_SV/StreamVault"
else
  P="$(cd "$(dirname "$0")/.." && pwd)"
fi

APP="$P/dist/StreamVault.app"
C="$APP/Contents"
B="$C/Resources/bin"
D="$P/.dep-cache"

cd "$P"

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet
  source "$HOME/.cargo/env"
  RUST_INSTALLED_BY_SCRIPT=true
fi

cargo build --release

rm -rf "$APP" "$D/f" "$D/n"
mkdir -p "$C/MacOS" "$B" "$D/f" "$D/n" "$C/Resources"

cp target/release/streamvault "$C/MacOS/"
cp resources/Info.plist "$C/"
cp resources/AppIcon.icns "$C/Resources/" 2>/dev/null || true

if [[ "$(uname -m)" == "arm64" ]]; then
  F_U="https://www.osxexperts.net/ffmpeg71arm.zip"
  N_A="arm64"
else
  F_U="https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip"
  N_A="x64"
fi

N_U="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_osx-${N_A}_20251029.tar.gz"

[[ -f "$D/f_$N_A.zip" ]] || curl -fsSL "$F_U" -o "$D/f_$N_A.zip"
[[ -f "$D/n_$N_A.tgz" ]] || curl -fsSL "$N_U" -o "$D/n_$N_A.tgz"

unzip -qo "$D/f_$N_A.zip" -d "$D/f"
tar xzf "$D/n_$N_A.tgz" -C "$D/n"

F_BIN="$(find "$D/f" -type f -name ffmpeg -perm -111 | head -n1)"
N_BIN="$(find "$D/n" -type f -name N_m3u8DL-RE -perm -111 | head -n1)"

[[ -n "$F_BIN" ]] || { printf 'ffmpeg binary not found\n' >&2; exit 1; }
[[ -n "$N_BIN" ]] || { printf 'N_m3u8DL-RE binary not found\n' >&2; exit 1; }

cp "$F_BIN" "$B/ffmpeg"
cp "$N_BIN" "$B/N_m3u8DL-RE"
chmod +x "$B"/*

xattr -cr "$B/ffmpeg" "$B/N_m3u8DL-RE" 2>/dev/null || true
codesign --force --sign - "$B/ffmpeg" 2>/dev/null || true
codesign --force --sign - "$B/N_m3u8DL-RE" 2>/dev/null || true
codesign --force --deep --sign - "$APP" 2>/dev/null || true
xattr -cr "$APP" 2>/dev/null || true

if [[ "$REMOTE_INSTALL" == true ]]; then
  mkdir -p "$HOME/Applications"
  cp -R "$APP" "$HOME/Applications/"
fi

if [[ "$RUST_INSTALLED_BY_SCRIPT" == true ]]; then
  rustup self uninstall -y
fi