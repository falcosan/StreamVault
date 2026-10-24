#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tizen="${TIZEN:-$HOME/tizen-studio/tools/ide/bin/tizen}"
sdb="${SDB:-$HOME/tizen-studio/tools/sdb}"
profile="${PROFILE:-streamvault-samsung}"
tv_mac="${TV_MAC:-e0:3:6b:70:4e:bd}"
public="$root/target/dx/streamvault/release/web/public"
widget="$root/resources/tv"
out="$root/target/tv"
stage="$out/stage"
app="$(sed -n 's/.*<tizen:application id="\([^"]*\)".*/\1/p' "$widget/config.xml")"

for sync in "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Contents/Video/StreamVault/sync.json" \
  "$HOME/Library/Application Support/StreamVault/sync.json"; do
  if [[ -f "$sync" && "$(plutil -extract enabled raw -o - "$sync" 2>/dev/null)" == true ]]; then
    export STREAMVAULT_SYNC_URL="${STREAMVAULT_SYNC_URL:-$(plutil -extract endpoint raw -o - "$sync")}"
    export STREAMVAULT_SYNC_TOKEN="${STREAMVAULT_SYNC_TOKEN:-$(plutil -extract token raw -o - "$sync")}"
    break
  fi
done

serial="${TV_IP:+$TV_IP:26101}"
serial="${serial:-$("$sdb" devices | awk '$2 == "device" { print $1; exit }')}"
if [[ -z "$serial" ]]; then
  ip="$(arp -a | grep -i "$tv_mac" | grep -oE '([0-9]+\.){3}[0-9]+' | head -1 || true)"
  [[ -n "$ip" ]] || { printf 'TV not found: turn it on or set TV_IP\n' >&2; exit 1; }
  serial="$ip:26101"
fi
"$sdb" connect "$serial" >/dev/null

rm -rf "$public" "$out"
(cd "$root" && dx build --release --platform web --debug-symbols false)
mkdir -p "$stage"
cp -R "$public"/. "$stage"/
cp "$widget/config.xml" "$widget/icon.png" "$stage"/
"$tizen" package -t wgt -s "$profile" -o "$out" -- "$stage"

"$sdb" -s "$serial" shell 0 was_kill "$app" >/dev/null || true
"$tizen" install -s "$serial" -n "$out/StreamVault.wgt"
"$sdb" -s "$serial" shell 0 was_execute "$app" >/dev/null
printf 'Installed on %s (sync %s)\n' "$serial" \
  "$([[ -n "${STREAMVAULT_SYNC_URL:-}" ]] && echo on || echo off)"
