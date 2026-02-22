#!/usr/bin/env bash
set -euo pipefail

P="$(cd "$(dirname "$0")/.." && pwd)"
APP="$P/dist/StreamVault.app"
C="$APP/Contents"
B="$C/Resources/bin"
D="$P/.dep-cache"

cd "$P"
cargo build --release

rm -rf "$APP" "$D/f" "$D/n"
mkdir -p "$C/MacOS" "$B" "$D/f" "$D/n"

cp target/release/streamvault "$C/MacOS/"
cp resources/Info.plist "$C/"
cp resources/AppIcon.icns "$C/Resources/" 2>/dev/null || true

[[ "$(uname -m)" == "arm64" ]] && { F_U="https://www.osxexperts.net/ffmpeg71arm.zip"; N_A="arm64"; } || { F_U="https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip"; N_A="x64"; }
N_U="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_osx-${N_A}_20251029.tar.gz"

[[ -f "$D/f_$N_A.zip" ]] || curl -sSL "$F_U" -o "$D/f_$N_A.zip"
[[ -f "$D/n_$N_A.tgz" ]] || curl -sSL "$N_U" -o "$D/n_$N_A.tgz"

unzip -qo "$D/f_$N_A.zip" -d "$D/f"
tar xzf "$D/n_$N_A.tgz" -C "$D/n"

cp "$(find "$D/f" -type f -name ffmpeg | head -n1)" "$B/ffmpeg"
cp "$(find "$D/n" -type f -name N_m3u8DL-RE | head -n1)" "$B/N_m3u8DL-RE"
chmod +x "$B"/*

codesign --force --deep --sign - "$APP" 2>/dev/null || true
