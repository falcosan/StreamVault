#!/usr/bin/env bash

main() {
  set -euo pipefail

  local steps=6 current=0 bar_width=30
  local repo="https://github.com/falcosan/StreamVault.git"
  local completed=false caller_dir="$PWD" remote_install=false rust_installed_by_script=false

  progress() {
    current=$((current + 1))
    local filled=$((current * bar_width / steps)) bar
    printf -v bar '%*s' "$filled" ''
    printf "\r  [%-${bar_width}s] %3d%%" "${bar// /#}" "$((current * 100 / steps))"
    ((current == steps)) && printf "\n" || :
  }

  cleanup() {
    [[ "$completed" == true ]] && return
    printf "\n  Interrupted, cleaning up…\n" >&2
    rm -rf "${app:-}" "${dep_cache:-}" "${tmpdir_sv:-}" 2>/dev/null || :
    [[ "$rust_installed_by_script" == true ]] && rustup self uninstall -y 2>/dev/null || :
  }

  trap cleanup EXIT INT TERM HUP

  local p
  if [[ -z "${BASH_SOURCE[0]:-}" || "$(basename "${BASH_SOURCE[0]:-bash}")" == "bash" ]]; then
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

  local app="$p/dist/StreamVault.app" dep_cache="$p/.dep-cache"
  local contents="$app/Contents" bin_dir="$contents/Resources/bin"

  cd "$p"

  if cargo --version &>/dev/null; then
    progress
  elif command -v rustup &>/dev/null; then
    progress
    rustup default stable >/dev/null 2>&1 \
      || { printf '\n  Failed to set Rust default toolchain\n' >&2; exit 1; }
    . "$HOME/.cargo/env" 2>/dev/null || :
  else
    progress
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --quiet \
      || { printf '\n  Rust installation failed\n' >&2; exit 1; }
    . "$HOME/.cargo/env"
    rust_installed_by_script=true
  fi

  progress
  cargo build --release --quiet || { printf '\n  cargo build failed\n' >&2; exit 1; }

  rm -rf "$app" "$dep_cache/f" "$dep_cache/n"
  mkdir -p "$contents/MacOS" "$bin_dir" "$dep_cache/"{f,n} "$contents/Resources"

  cp target/release/streamvault "$contents/MacOS/"
  cp resources/Info.plist "$contents/" 2>/dev/null || :
  cp resources/AppIcon.icns "$contents/Resources/" 2>/dev/null || :

  local arch narch ffmpeg_url
  arch="$(uname -m)"
  [[ "$arch" == arm64 ]] && narch=arm64 || narch=x64
  [[ "$narch" == arm64 ]] \
    && ffmpeg_url="https://www.osxexperts.net/ffmpeg71arm.zip" \
    || ffmpeg_url="https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip"

  local nurl="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_osx-${narch}_20251029.tar.gz"
  local ffmpeg_zip="$dep_cache/f_${narch}.zip" ntar="$dep_cache/n_${narch}.tgz"

  progress
  local dl_pids=()
  [[ -f "$ffmpeg_zip" ]] || { curl -fsSL "$ffmpeg_url" -o "$ffmpeg_zip" & dl_pids+=($!); }
  [[ -f "$ntar"       ]] || { curl -fsSL "$nurl"       -o "$ntar"       & dl_pids+=($!); }
  for pid in ${dl_pids[@]+"${dl_pids[@]}"}; do
    wait "$pid" || { printf '\n  Download failed\n' >&2; exit 1; }
  done

  unzip -qo "$ffmpeg_zip" -d "$dep_cache/f"
  tar xzf "$ntar" -C "$dep_cache/n"

  local ffmpeg_bin nbin
  ffmpeg_bin="$(find "$dep_cache/f" -type f -name ffmpeg -perm -111 | head -n1)"
  nbin="$(find "$dep_cache/n" -type f -name N_m3u8DL-RE -perm -111 | head -n1)"
  [[ -n "$ffmpeg_bin" ]] || { printf '\n  ffmpeg binary not found\n' >&2; exit 1; }
  [[ -n "$nbin"       ]] || { printf '\n  N_m3u8DL-RE binary not found\n' >&2; exit 1; }

  install -m 755 "$ffmpeg_bin" "$bin_dir/ffmpeg"
  install -m 755 "$nbin" "$bin_dir/N_m3u8DL-RE"

  progress
  for bin in "$bin_dir/ffmpeg" "$bin_dir/N_m3u8DL-RE"; do
    xattr -cr "$bin" 2>/dev/null || :
    codesign --force --sign - "$bin" 2>/dev/null || :
  done
  codesign --force --deep --sign - "$app" 2>/dev/null || :
  xattr -cr "$app" 2>/dev/null || :

  if [[ "$remote_install" == true ]]; then
    rm -rf "$caller_dir/StreamVault.app"
    cp -R "$app" "$caller_dir/"
  fi

  completed=true
  [[ -n "${tmpdir_sv:-}" ]] && rm -rf "$tmpdir_sv" 2>/dev/null || :
  progress
}

main "$@"
