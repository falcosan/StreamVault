#!/usr/bin/env bash

main() {
  set -euo pipefail

  [[ "$(uname -s)" == Darwin ]] || { printf '\n  macOS required\n' >&2; exit 1; }

  local -r repo="https://github.com/falcosan/StreamVault.git"
  local -r bar_width=30 steps=6
  local -r required_cmds=(git curl unzip tar)
  local current=0 completed=false caller_dir="$PWD"
  local remote_install=false rust_installed_by_script=false
  local app="" dep_cache="" tmpdir_sv=""

  for cmd in "${required_cmds[@]}"; do
    command -v "$cmd" &>/dev/null || { printf '\n  %s is required\n' "$cmd" >&2; exit 1; }
  done

  progress() {
    current=$((current + 1))
    local filled=$((current * bar_width / steps))
    printf "\r  [%-${bar_width}s] %3d%%" "$(printf '%*s' "$filled" '' | tr ' ' '#')" "$((current * 100 / steps))"
    ((current == steps)) && printf "\n"
    return 0
  }

  remove_rust() {
    [[ "$rust_installed_by_script" == true ]] || return 0
    rustup self uninstall -y &>/dev/null || :
    rm -rf "$HOME/.rustup" "$HOME/.cargo" &>/dev/null || :
  }

  cleanup() {
    [[ "$completed" == true ]] && return 0
    printf "\n  Interrupted, cleaning up…\n" >&2
    rm -rf "${app:+$app}" "${dep_cache:+$dep_cache}" "${tmpdir_sv:+$tmpdir_sv}" &>/dev/null || :
    remove_rust
  }

  trap cleanup EXIT INT TERM HUP PIPE

  local p
  if [[ -z "${BASH_SOURCE[0]:-}" || "$(basename "${BASH_SOURCE[0]:-bash}")" == bash ]]; then
    remote_install=true
    tmpdir_sv="$(mktemp -d)" || { printf '\n  Failed to create temp directory\n' >&2; exit 1; }
    progress
    git clone --depth 1 --quiet "$repo" "$tmpdir_sv/StreamVault" 2>/dev/null \
      || { printf '\n  Failed to clone repository\n' >&2; exit 1; }
    p="$tmpdir_sv/StreamVault"
  else
    p="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    progress
  fi

  app="$p/dist/StreamVault.app"
  dep_cache="$p/.dep-cache"
  local -r contents="$app/Contents" bin_dir="$contents/Resources/bin"

  cd "$p"

  if cargo --version &>/dev/null; then
    progress
  elif command -v rustup &>/dev/null; then
    progress
    rustup default stable &>/dev/null \
      || { printf '\n  Failed to set Rust default toolchain\n' >&2; exit 1; }
    [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
  else
    progress
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --quiet \
      || { printf '\n  Rust installation failed\n' >&2; exit 1; }
    . "$HOME/.cargo/env"
    rust_installed_by_script=true
  fi

  cargo --version &>/dev/null || { printf '\n  cargo not found after setup\n' >&2; exit 1; }

  progress
  cargo build --release --quiet || { printf '\n  cargo build failed\n' >&2; exit 1; }

  rm -rf "$app" "$dep_cache/f" "$dep_cache/n"
  install -d "$contents/MacOS" "$bin_dir" "$dep_cache/f" "$dep_cache/n" "$contents/Resources"

  cp target/release/streamvault "$contents/MacOS/"
  [[ -f resources/Info.plist ]] && cp resources/Info.plist "$contents/"
  [[ -f resources/AppIcon.icns ]] && cp resources/AppIcon.icns "$contents/Resources/"

  local narch ffmpeg_url
  case "$(uname -m)" in
    arm64|aarch64) narch=arm64; ffmpeg_url="https://www.osxexperts.net/ffmpeg71arm.zip" ;;
    *)             narch=x64;   ffmpeg_url="https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip"  ;;
  esac

  local -r nurl="https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_osx-${narch}_20251029.tar.gz"
  local -r ffmpeg_zip="$dep_cache/f_${narch}.zip" ntar="$dep_cache/n_${narch}.tgz"

  progress

  dl() { [[ -f "$2" ]] && return 0; curl -fsSL --retry 3 --retry-delay 2 "$1" -o "$2.tmp" && mv "$2.tmp" "$2"; }

  local dl_pids=()
  dl "$ffmpeg_url" "$ffmpeg_zip" & dl_pids+=($!)
  dl "$nurl" "$ntar" & dl_pids+=($!)

  local pid fail=false
  for pid in "${dl_pids[@]}"; do
    wait "$pid" || fail=true
  done
  [[ "$fail" == false ]] || { printf '\n  Download failed\n' >&2; exit 1; }

  unzip -tq "$ffmpeg_zip" &>/dev/null || { printf '\n  Corrupt ffmpeg archive\n' >&2; rm -f "$ffmpeg_zip"; exit 1; }
  tar tzf "$ntar" &>/dev/null          || { printf '\n  Corrupt N_m3u8DL-RE archive\n' >&2; rm -f "$ntar"; exit 1; }

  unzip -qo "$ffmpeg_zip" -d "$dep_cache/f"
  tar xzf "$ntar" -C "$dep_cache/n"

  local ffmpeg_bin nbin
  ffmpeg_bin="$(find "$dep_cache/f" -type f -name ffmpeg -perm +111 2>/dev/null | head -n1)"
  nbin="$(find "$dep_cache/n" -type f -name N_m3u8DL-RE -perm +111 2>/dev/null | head -n1)"
  [[ -n "$ffmpeg_bin" ]] || { printf '\n  ffmpeg binary not found\n' >&2; exit 1; }
  [[ -n "$nbin"       ]] || { printf '\n  N_m3u8DL-RE binary not found\n' >&2; exit 1; }

  install -m 755 "$ffmpeg_bin" "$bin_dir/ffmpeg"
  install -m 755 "$nbin" "$bin_dir/N_m3u8DL-RE"

  progress
  local bin
  for bin in "$bin_dir/ffmpeg" "$bin_dir/N_m3u8DL-RE"; do
    xattr -cr "$bin" &>/dev/null || :
    codesign --force --sign - "$bin" &>/dev/null || :
  done
  codesign --force --deep --sign - "$app" &>/dev/null || :
  xattr -cr "$app" &>/dev/null || :

  if [[ "$remote_install" == true ]]; then
    rm -rf "$caller_dir/StreamVault.app"
    cp -R "$app" "$caller_dir/"
  fi

  completed=true
  remove_rust
  [[ -n "$tmpdir_sv" ]] && rm -rf "$tmpdir_sv" &>/dev/null || :
  progress
}

main "$@"