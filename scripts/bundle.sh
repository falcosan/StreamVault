#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

die() { printf '%s\n' "$*" >&2; exit 1; }
join() { local IFS=,; printf '%s' "$*"; }

tv() {
  local usage="usage: TV_UPDATE_KEY=<private key PEM> scripts/bundle.sh tv <web build> <output dir>"
  local public="${1:?$usage}" out="${2:?$usage}" key="${TV_UPDATE_KEY:?$usage}"
  local version shell entry wasm file name signature files=() scripts=()

  version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
  shell="$(sed -n 's/^  const SHELL = \([0-9]*\);$/\1/p' "$root/assets/js/boot.js")"
  entry="$(sed -n 's:.*<script type="module" async src="/\./assets/\([^"]*\)"></script>.*:\1:p' "$public/index.html")"
  wasm=("$public"/assets/*.wasm)
  [[ -n "$version" && -n "$shell" && -n "$entry" && -f "$public/assets/$entry" && ${#wasm[@]} == 1 ]] ||
    die "$public is not a StreamVault web build."

  mkdir -p "$out"
  cp "$public/index.html" "$out/index.html"
  for file in "$public"/assets/*; do
    name="${file##*/}"
    cp "$file" "$out/$name"
    files+=("{\"name\":\"$name\",\"sha256\":\"$(openssl dgst -sha256 -r "$file" | cut -d ' ' -f 1)\"}")
    [[ "$name" == *.js && "$name" != "$entry" ]] && scripts+=("\"$name\"")
  done
  printf '{"version":"%s","shell":%s,"entry":"%s","wasm":"%s","scripts":[%s],"files":[%s]}' \
    "$version" "$shell" "$entry" "${wasm[0]##*/}" "$(join "${scripts[@]}")" "$(join "${files[@]}")" >"$out/tv.json"
  signature="$(openssl dgst -sha256 -sign <(printf '%s\n' "$key") "$out/tv.json" |
    openssl asn1parse -inform DER |
    awk -F: '/INTEGER/ { v = $NF; while (length(v) < 64) v = "0" v; for (i = length(v) - 63; i < length(v); i += 2) printf "\\x%s", substr(v, i, 2) }')"
  printf '%b' "$signature" >"$out/tv.sig"
  [[ "$(wc -c <"$out/tv.sig" | tr -d ' ')" == 64 ]] || die "Signing failed."
  printf 'StreamVault %s for TV, shell %s: %s\n' "$version" "$shell" "$out"
}

case "${1:-}" in
  tv) tv "${@:2}" ;;
  *) die "usage: scripts/bundle.sh tv <web build> <output dir>" ;;
esac
