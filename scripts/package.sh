#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
APP_DIR="$DIST_DIR/StreamVault.app"
CONTENTS="$APP_DIR/Contents"
BIN_DIR="$CONTENTS/Resources/bin"
CACHE_DIR="$PROJECT_DIR/.dep-cache"

FFMPEG_VERSION="7.1"
N_M3U8DL_RE_VERSION="v0.5.1-beta"
N_M3U8DL_RE_DATE="20251029"

ARCH="$(uname -m)"

download_cached() {
    local url="$1"
    local dest="$2"
    if [[ -f "$dest" ]]; then
        echo "  ✓ cached  $(basename "$dest")"
    else
        echo "  ↓ downloading $(basename "$dest") …"
        mkdir -p "$(dirname "$dest")"
        curl -fSL "$url" -o "$dest"
    fi
}

echo "═══  StreamVault packager  ═══"
echo ""

echo "▸ Building release binary …"
cd "$PROJECT_DIR"
cargo build --release
echo "  ✓ release build complete"
echo ""

echo "▸ Creating app bundle …"
rm -rf "$APP_DIR"
mkdir -p "$CONTENTS/MacOS"
mkdir -p "$BIN_DIR"

cp "$PROJECT_DIR/target/release/streamvault" "$CONTENTS/MacOS/streamvault"
cp "$PROJECT_DIR/resources/Info.plist"        "$CONTENTS/Info.plist"

if [[ -f "$PROJECT_DIR/resources/AppIcon.icns" ]]; then
    mkdir -p "$CONTENTS/Resources"
    cp "$PROJECT_DIR/resources/AppIcon.icns" "$CONTENTS/Resources/AppIcon.icns"
fi
echo "  ✓ bundle skeleton ready"
echo ""

echo "▸ Bundling ffmpeg …"

FFMPEG_VER_COMPACT="${FFMPEG_VERSION/./}"
if [[ "$ARCH" == "arm64" ]]; then
    FFMPEG_URL="https://www.osxexperts.net/ffmpeg${FFMPEG_VER_COMPACT}arm.zip"
    FFMPEG_CACHED="$CACHE_DIR/ffmpeg-${FFMPEG_VERSION}-arm64.zip"
else
    FFMPEG_URL="https://evermeet.cx/ffmpeg/ffmpeg-${FFMPEG_VERSION}.zip"
    FFMPEG_CACHED="$CACHE_DIR/ffmpeg-${FFMPEG_VERSION}-x86_64.zip"
fi

download_cached "$FFMPEG_URL" "$FFMPEG_CACHED"

FFMPEG_TMP="$CACHE_DIR/ffmpeg-extract"
rm -rf "$FFMPEG_TMP"
mkdir -p "$FFMPEG_TMP"
unzip -qo "$FFMPEG_CACHED" -d "$FFMPEG_TMP"

FFMPEG_BIN="$(find "$FFMPEG_TMP" -type f -name 'ffmpeg' | head -1)"
if [[ -z "$FFMPEG_BIN" ]]; then
    echo "  ✗  Could not locate ffmpeg binary inside the archive."
    echo "     Please place a static ffmpeg binary manually at:"
    echo "       $BIN_DIR/ffmpeg"
    echo ""
    echo "     You can download one from https://evermeet.cx/ffmpeg/ (x86_64)"
    echo "     or https://www.osxexperts.net/ (arm64)."
else
    cp "$FFMPEG_BIN" "$BIN_DIR/ffmpeg"
    chmod +x "$BIN_DIR/ffmpeg"
    echo "  ✓ ffmpeg ${FFMPEG_VERSION} bundled"
fi
echo ""

echo "▸ Bundling N_m3u8DL-RE …"

if [[ "$ARCH" == "arm64" ]]; then
    NM3_ASSET="N_m3u8DL-RE_${N_M3U8DL_RE_VERSION}_osx-arm64_${N_M3U8DL_RE_DATE}.tar.gz"
else
    NM3_ASSET="N_m3u8DL-RE_${N_M3U8DL_RE_VERSION}_osx-x64_${N_M3U8DL_RE_DATE}.tar.gz"
fi

NM3_URL="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/${N_M3U8DL_RE_VERSION}/${NM3_ASSET}"
NM3_CACHED="$CACHE_DIR/${NM3_ASSET}"

download_cached "$NM3_URL" "$NM3_CACHED"

NM3_TMP="$CACHE_DIR/nm3-extract"
rm -rf "$NM3_TMP"
mkdir -p "$NM3_TMP"
tar xzf "$NM3_CACHED" -C "$NM3_TMP"

NM3_BIN="$(find "$NM3_TMP" -type f -name 'N_m3u8DL-RE' | head -1)"
if [[ -z "$NM3_BIN" ]]; then
    echo "  ✗  Could not locate N_m3u8DL-RE binary inside the archive."
    echo "     Please place it manually at:"
    echo "       $BIN_DIR/N_m3u8DL-RE"
else
    cp "$NM3_BIN" "$BIN_DIR/N_m3u8DL-RE"
    chmod +x "$BIN_DIR/N_m3u8DL-RE"
    echo "  ✓ N_m3u8DL-RE ${N_M3U8DL_RE_VERSION} bundled"
fi
echo ""

echo "▸ Signing app bundle …"
codesign --force --deep --sign - "$APP_DIR" 2>/dev/null || true
echo "  ✓ ad-hoc signed"
echo ""

echo "═══  Done!  ═══"
echo ""
echo "  App bundle:  $APP_DIR"
echo ""
echo "  To run:"
echo "    open dist/StreamVault.app"
echo ""
echo "  To distribute, zip the .app:"
echo "    cd dist && zip -r StreamVault.zip StreamVault.app"
echo ""
