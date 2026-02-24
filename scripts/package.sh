#!/usr/bin/env bash

main() {
  set -euo pipefail

  local completed=false
  local caller_dir="$(pwd)"
  local remote_install=false
  local rust_installed_by_script=false
  local steps=6 current=0 bar_width=30
  local repo="https://github.com/falcosan/StreamVault.git"

  progress() {
    current=$((current + 1))
    local pct=$((current * 100 / steps))
    local filled=$((current * bar_width / steps))
    printf "\r  [%-${bar_width}s] %3d%%" \
      "$(printf '%*s' "$filled" '' | tr ' ' '#')" "$pct"
    ((current == steps)) && printf "\n" || true
  }

  cleanup() {
    [[ "$completed" == true ]] && return
    printf "\n  Interrupted, cleaning up…\n" >&2
    [[ -n "${app:-}"       ]] && rm -rf "$app"       2>/dev/null || true
    [[ -n "${dep_cache:-}" ]] && rm -rf "$dep_cache" 2>/dev/null || true
    [[ -n "${tmpdir_sv:-}" ]] && rm -rf "$tmpdir_sv" 2>/dev/null || true
    [[ "$rust_installed_by_script" == true ]] && \
      rustup self uninstall -y 2>/dev/null || true
  }

  trap cleanup EXIT INT TERM HUP

  local p
  if [[ -z "${BASH_SOURCE[0]:-}" || \
        "$(basename "${BASH_SOURCE[0]:-bash}")" == "bash" ]]; then
    remote_install=true
    tmpdir_sv="$(mktemp -d)"
    progress
    git clone --depth 1 --quiet "$repo" "$tmpdir_sv/StreamVault" 2>/dev/null \
      || { printf '\n  Failed to clone repository\n' >&2; exit 1; }
    p="$tmpdir_sv/StreamVault"
  else
    p="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    progress
  fi

  local app="$p/dist/StreamVault.app"
  local contents="$app/Contents"
  local bin_dir="$contents/Resources/bin"
  local dep_cache="$p/.dep-cache"

  cd "$p"

  if cargo --version &>/dev/null; then
    progress
  elif command -v rustup &>/dev/null; then
    progress
    rustup default stable >/dev/null 2>&1 \
      || { printf '\n  Failed to set Rust default toolchain\n' >&2; exit 1; }
    . "$HOME/.cargo/env" 2>/dev/null || true
  else
    progress
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --quiet \
      || { printf '\n  Rust installation failed\n' >&2; exit 1; }
    . "$HOME/.cargo/env"
    rust_installed_by_script=true
  fi

  progress
  cargo build --release --quiet \
    || { printf '\n  cargo build failed\n' >&2; exit 1; }

  rm -rf "$app" "$dep_cache/f" "$dep_cache/n"
  mkdir -p "$contents/MacOS" "$bin_dir" "$dep_cache/f" "$dep_cache/n" \
           "$contents/Resources"

  cp target/release/streamvault "$contents/MacOS/"
  cp resources/Info.plist "$contents/" 2>/dev/null || true
  cp resources/AppIcon.icns "$contents/Resources/" 2>/dev/null || true

  local arch ffmpeg_url narch
  arch="$(uname -m)"
  if [[ "$arch" == "arm64" ]]; then
    ffmpeg_url="https://www.osxexperts.net/ffmpeg71arm.zip"
    narch="arm64"
  else
    ffmpeg_url="https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip"
    narch="x64"
  fi

  local nurl="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_osx-${narch}_20251029.tar.gz"
  local ffmpeg_zip="$dep_cache/f_${narch}.zip"
  local ntar="$dep_cache/n_${narch}.tgz"

  progress
  [[ -f "$ffmpeg_zip" ]] || curl -fsSL "$ffmpeg_url" -o "$ffmpeg_zip"
  [[ -f "$ntar"       ]] || curl -fsSL "$nurl"        -o "$ntar"

  unzip -qo "$ffmpeg_zip" -d "$dep_cache/f"
  tar xzf   "$ntar"       -C "$dep_cache/n"

  local ffmpeg_bin nbin
  ffmpeg_bin="$(find "$dep_cache/f" -type f -name ffmpeg -perm -111 | head -n1)"
  nbin="$(find "$dep_cache/n" -type f -name N_m3u8DL-RE -perm -111 | head -n1)"

  [[ -n "$ffmpeg_bin" ]] || { printf '\n  ffmpeg binary not found\n'      >&2; exit 1; }
  [[ -n "$nbin"       ]] || { printf '\n  N_m3u8DL-RE binary not found\n' >&2; exit 1; }

  cp "$ffmpeg_bin" "$bin_dir/ffmpeg"
  cp "$nbin"       "$bin_dir/N_m3u8DL-RE"
  chmod +x "$bin_dir"/*

  progress
  xattr    -cr           "$bin_dir/ffmpeg" "$bin_dir/N_m3u8DL-RE" 2>/dev/null || true
  codesign --force --sign - "$bin_dir/ffmpeg"        2>/dev/null || true
  codesign --force --sign - "$bin_dir/N_m3u8DL-RE"  2>/dev/null || true
  codesign --force --deep --sign - "$app"            2>/dev/null || true
  xattr    -cr "$app"                                2>/dev/null || true

  if [[ "$remote_install" == true ]]; then
    rm -rf "$caller_dir/StreamVault.app"
    cp -R  "$app" "$caller_dir/"
  fi

  completed=true
  [[ -n "${tmpdir_sv:-}" ]] && rm -rf "$tmpdir_sv" 2>/dev/null || true
  progress
}

main "$@"
