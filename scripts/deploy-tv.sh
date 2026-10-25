#!/usr/bin/env bash
set -euo pipefail

readonly DX_VERSION=0.7.10
readonly ANDROID_API=36
readonly ANDROID_BUILD_TOOLS=36.0.0
readonly ANDROID_MIN_API=21
readonly WEBVIEW_MIN=96
readonly CMDLINE_TOOLS=https://dl.google.com/android/repository/commandlinetools-mac-15641748_latest.zip
readonly TIZEN_STUDIO=https://download.tizen.org/sdk/Installer/tizen-studio_6.1/web-ide_Tizen_Studio_6.1_macos-64.dmg
readonly SAMSUNG_PORT=4794
readonly SAMSUNG_LOGIN="https://account.samsung.com/mobile/account/check.do?serviceID=v285zxnl3h&actionID=StartOAuth2&accessToken=Y&redirect_uri=http://localhost:$SAMSUNG_PORT/signin/callback"
readonly SAMSUNG_API=https://svdca.samsungqbe.com/apis/v3
readonly SCAN_MASK=0xfffffc00
readonly PROBE_JOBS=64
readonly BACKGROUND=151515
readonly SOURCE=https://github.com/falcosan/StreamVault/archive/refs/heads/main.tar.gz

cache="$HOME/Library/Caches/StreamVault"
if [[ ! -f "${BASH_SOURCE[0]:-}" ]]; then
  mkdir -p "$cache/source"
  find "$cache/source" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +
  curl -fsSL "$SOURCE" | tar xzm -C "$cache/source" --strip-components 1
  exec bash "$cache/source/scripts/deploy-tv.sh" "$@" </dev/tty
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
resources="$root/resources/tv"
logo="$root/assets/img/vector/logo.svg"
public="$root/target/dx/streamvault/release/web/public"
out="$root/target/tv"
log="$out/deploy.log"
tizen_home="${TIZEN_HOME:-$HOME/tizen-studio}"
android_home="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
adb="$android_home/platform-tools/adb"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
arch="$(uname -m | sed 's/arm64/aarch64/')"

say() { printf '  %s\n' "$*"; }
die() { printf '\n  %s\n' "$*" >&2; exit 1; }
run() { "$@" >>"$log" 2>&1 || { tail -n 20 "$log" >&2; die "Failed: $* (log: $log)"; }; }
raster() { run sips -s format png --resampleHeightWidth "$1" "$1" "$logo" --out "$2"; }
download() { [[ -s "$2" ]] || { curl -fsSL --retry 3 "$1" -o "$2.part" && mv "$2.part" "$2"; }; }
port() { nc -z -G 1 "$1" "$2" &>/dev/null; }
field() { plutil -extract "$1" raw -o - - 2>/dev/null | sed 's/&quot;/"/g; s/&amp;/\&/g'; }
tizen_info() { curl -fsS --connect-timeout 1 -m 3 "http://$1:8001/api/v2/" 2>/dev/null; }

wait_until() {
  local message=$1
  shift
  "$@" && return 0
  say "$message"
  until "$@"; do sleep 3; done
}

probe() {
  local ip=$1 info
  if info="$(tizen_info "$ip")" &&
    [[ "$(field device.OS <<<"$info")" == Tizen ]]; then
    printf 'tizen\t%s\tSamsung %s (%s)\n' "$ip" "$(field device.name <<<"$info")" "$(field device.modelName <<<"$info")"
  elif port "$ip" 5555 || port "$ip" 6466; then
    printf 'android\t%s\tAndroid TV / Google TV / Fire TV\n' "$ip"
  elif info="$(curl -fsS --connect-timeout 1 -m 3 "http://$ip:8060/query/device-info" 2>/dev/null)"; then
    printf 'roku\t%s\tRoku %s\n' "$ip" "$(sed -n 's:.*<user-device-name>\(.*\)</user-device-name>.*:\1:p' <<<"$info")"
  elif port "$ip" 3000 && port "$ip" 3001; then
    printf 'webos\t%s\tLG webOS TV\n' "$ip"
  elif port "$ip" 36669; then
    printf 'vidaa\t%s\tHisense VIDAA TV\n' "$ip"
  fi
}

hosts() {
  ifconfig | awk '/inet / { for (i = 1; i < NF; i++) if ($i == "netmask") print $2, $(i + 1) }' |
    while read -r ip mask; do
      [[ "$ip" == 127.* || "$ip" == 169.254.* ]] && continue
      IFS=. read -r a b c d <<<"$ip"
      local m=$((mask > SCAN_MASK ? mask : SCAN_MASK)) host
      local net=$((((a << 24) | (b << 16) | (c << 8) | d) & m)) size=$((~m & 0xffffffff))
      for ((host = net + 1; host < net + size; host++)); do
        printf '%d.%d.%d.%d\n' $((host >> 24 & 255)) $((host >> 16 & 255)) $((host >> 8 & 255)) $((host & 255))
      done
    done | sort -u
}

discover() {
  local probes=0
  if [[ -x "$adb" ]]; then
    "$adb" devices -l 2>/dev/null | awk '$2 == "device" && $1 !~ /:/ {
      model = $1; for (i = 3; i <= NF; i++) if ($i ~ /^model:/) model = substr($i, 7)
      printf "android\t%s\t%s\n", $1, model }'
  fi
  hosts | xargs -P 256 -n 1 ping -c 1 -t 1 -q &>/dev/null || true
  arp -an | awk '$4 ~ /:/ { gsub(/[()]/, "", $2); print $2 }' | sort -u | {
    while read -r ip; do
      probe "$ip" &
      ((++probes % PROBE_JOBS)) || wait
    done
    wait
  }
}

choose() {
  local -a tvs=() labels=()
  local line kind address name
  while IFS=$'\t' read -r kind address name; do
    case "$kind" in
      tizen | android) tvs+=("$kind"$'\t'"$address") labels+=("$name · $address") ;;
      roku) say "Skipping $name ($address): Roku runs only BrightScript channels, it has no web engine for StreamVault." ;;
      webos) say "Skipping $name ($address): webOS blocks module scripts and cross-site requests in installed apps." ;;
      vidaa) say "Skipping $name ($address): VIDAA does not allow installing apps outside its store." ;;
    esac
  done < <(discover)
  ((${#tvs[@]})) || die "No supported TV found. Turn on the TV; on Android TV, Google TV or Fire TV enable debugging in Developer options first."
  if ((${#tvs[@]} == 1)); then
    say "Found ${labels[0]}"
    choice="${tvs[0]}"
    return
  fi
  [[ -t 0 ]] || die "Several TVs found; run it in a terminal to choose one."
  PS3="  TV number: "
  select line in "${labels[@]}"; do
    [[ -n "$line" ]] && { choice="${tvs[REPLY - 1]}"; return; }
  done
  exit 1
}

rust_tools() {
  if ! xcrun --find cc &>/dev/null; then
    say "Installing the Command Line Tools…"
    local placeholder=/tmp/.com.apple.dt.CommandLineTools.installondemand.in-progress package
    touch "$placeholder"
    package="$(softwareupdate --list 2>/dev/null | grep -o 'Command Line Tools[^*]*' | sort -V | tail -n1 | xargs)" || true
    [[ -n "$package" ]] || { rm -f "$placeholder"; die "Command Line Tools package not found."; }
    run softwareupdate --install "$package" --agree-to-license
    rm -f "$placeholder"
  fi
  [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
  if ! command -v rustup &>/dev/null; then
    export RUSTUP_HOME="$cache/rustup" CARGO_HOME="$cache/cargo" PATH="$cache/cargo/bin:$PATH"
    if command -v rustup &>/dev/null; then
      run rustup update stable --no-self-update
    else
      say "Installing Rust…"
      download https://sh.rustup.rs "$cache/rustup-init.sh"
      run sh "$cache/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain stable
    fi
  fi
  run rustup target add wasm32-unknown-unknown
  export PATH="$cache/bin:$PATH"
  if [[ "$(dx --version 2>/dev/null)" != "dioxus $DX_VERSION "* ]]; then
    say "Installing dx ${DX_VERSION}…"
    mkdir -p "$cache/bin"
    download "https://github.com/DioxusLabs/dioxus/releases/download/v$DX_VERSION/dx-$arch-apple-darwin.tar.gz" "$cache/dx-$DX_VERSION.tar.gz"
    run tar xzf "$cache/dx-$DX_VERSION.tar.gz" -C "$cache/bin" dx
  fi
}

java_tools() {
  local home
  for home in "${JAVA_HOME:-}" "$(/usr/libexec/java_home 2>/dev/null || true)" "$cache/jdk/Contents/Home"; do
    if [[ -n "$home" ]] && "$home/bin/javac" --release 17 -version &>/dev/null; then
      export JAVA_HOME="$home" PATH="$home/bin:$PATH"
      return
    fi
  done
  say "Installing a Java runtime…"
  download "https://api.adoptium.net/v3/binary/latest/21/ga/mac/$arch/jdk/hotspot/normal/eclipse" "$cache/jdk.tar.gz"
  rm -rf "$cache/jdk" && mkdir -p "$cache/jdk"
  run tar xzf "$cache/jdk.tar.gz" -C "$cache/jdk" --strip-components 1
  export JAVA_HOME="$cache/jdk/Contents/Home" PATH="$cache/jdk/Contents/Home/bin:$PATH"
}

android_tools() {
  java_tools
  local sdkmanager="$android_home/cmdline-tools/latest/bin/sdkmanager"
  if [[ ! -x "$sdkmanager" ]]; then
    say "Installing the Android SDK tools…"
    local archive="$cache/${CMDLINE_TOOLS##*/}"
    download "$CMDLINE_TOOLS" "$archive"
    rm -rf "$android_home/cmdline-tools/latest" "$cache/cmdline-tools"
    run unzip -q "$archive" -d "$cache"
    mkdir -p "$android_home/cmdline-tools" && mv "$cache/cmdline-tools" "$android_home/cmdline-tools/latest"
  fi
  if [[ ! -x "$android_home/build-tools/$ANDROID_BUILD_TOOLS/apksigner" || ! -f "$android_home/platforms/android-$ANDROID_API/android.jar" || ! -x "$adb" ]]; then
    say "Installing the Android build tools…"
    yes | "$sdkmanager" --sdk_root="$android_home" --licenses &>/dev/null || true
    run "$sdkmanager" --sdk_root="$android_home" "platform-tools" "build-tools;$ANDROID_BUILD_TOOLS" "platforms;android-$ANDROID_API"
  fi
}

tizen_tools() {
  if [[ ! -x "$tizen_home/tools/ide/bin/tizen" ]]; then
    say "Installing Tizen Studio (about 3 GB, once)…"
    local mount java="$HOME/.package-manager/jdk/Contents/Home/bin/java" image="$cache/${TIZEN_STUDIO##*/}"
    download "$TIZEN_STUDIO" "$image"
    mount="$(mktemp -d)"
    run hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$image"
    trap 'hdiutil detach -quiet "$mount"' EXIT
    local installer="$mount/installer.app/Contents/Resources/Java"
    run unzip -qq -o "$installer/tizen-sdk.zip" "jdk/*" -d "$HOME/.package-manager"
    run "$java" -cp "$installer/installer.jar" org.tizen.installer.cli.CLIMain --accept-license --no-java-check "$tizen_home"
    run hdiutil detach "$mount"
    trap - EXIT
    run "$tizen_home/package-manager/package-manager-cli.bin" install --accept-license cert-add-on TV-SAMSUNG-Extension-Tools
  fi
  tizen="$tizen_home/tools/ide/bin/tizen"
  sdb="$tizen_home/tools/sdb"
  profiles="$(sed -n 's/^TIZEN_SDK_DATA_PATH=//p' "$tizen_home/sdk.info")/profile/profiles.xml"
}

load_sync() {
  local sync
  for sync in "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Contents/Video/StreamVault/sync.json" \
    "$HOME/Library/Application Support/StreamVault/sync.json"; do
    if [[ -f "$sync" && "$(plutil -extract enabled raw -o - "$sync" 2>/dev/null)" == true ]]; then
      export STREAMVAULT_SYNC_URL="${STREAMVAULT_SYNC_URL:-$(plutil -extract endpoint raw -o - "$sync")}"
      export STREAMVAULT_SYNC_TOKEN="${STREAMVAULT_SYNC_TOKEN:-$(plutil -extract token raw -o - "$sync")}"
      return
    fi
  done
}

build_web() {
  say "Building StreamVault ${version}…"
  rm -rf "$public"
  (cd "$root" && run dx build --release --platform web --debug-symbols false)
}

developer_mode() {
  local info
  info="$(tizen_info "$1")" &&
    [[ "$(field device.developerMode <<<"$info")" == 1 && "$(field device.developerIP <<<"$info")" == "$2" ]]
}

sdb_online() {
  "$sdb" connect "$1" &>/dev/null || true
  [[ "$("$sdb" -s "$1" get-state 2>/dev/null)" == device ]]
}

samsung_login() {
  local reply='Signed in: go back to the terminal.' request="" source code
  until [[ "$request" == *code=* ]]; do
    request="$(printf 'HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: %d\r\nConnection: close\r\n\r\n%s' \
      "${#reply}" "$reply" | nc -l 127.0.0.1 "$SAMSUNG_PORT")" || die "Port $SAMSUNG_PORT is in use; free it and run this again."
  done
  source="${request##*$'\n'}"
  [[ "$request" == GET* ]] && source="${request%%$'\r'*}"
  code="${source#*code=}"
  code="${code%%[& ]*}"
  code="${code//+/ }"
  printf '%b' "${code//%/\\x}"
}

samsung_api() {
  local endpoint=$1 output=$2 token=$3 user=$4 status
  shift 4
  status="$(printf 'form = "access_token=%s"\nform = "user_id=%s"\n' "$token" "$user" |
    curl -sS -m 60 -K - -o "$output" -w '%{http_code}' -F platform=VD "$@" "$SAMSUNG_API/$endpoint")" || true
  [[ "$status" == 200 ]] ||
    die "Samsung refused the certificate: $(plutil -extract error.description raw -o - "$output" 2>/dev/null || head -c 300 "$output")"
}

samsung_certificate() {
  local serial=$1 duid=$2 dir="$HOME/SamsungCertificate/streamvault-$2" login token user email password role
  local plugins="$tizen_home/tools/certificate-manager/Certificate-manager.app/Contents/Eclipse/plugins"
  say "Sign in to your Samsung account in the browser: Samsung issues the certificate for this TV ($duid)."
  open "$SAMSUNG_LOGIN"
  login="$(samsung_login)"
  token="$(field access_token <<<"$login")"
  user="$(field userId <<<"$login")"
  email="$(field inputEmailID <<<"$login")"
  [[ -n "$token" && -n "$user" ]] || die "The Samsung sign-in returned no access token."
  mkdir -p "$dir" && chmod 700 "$dir"
  run /usr/bin/openssl req -new -newkey rsa:2048 -nodes -sha512 -subj "/CN=StreamVault" \
    -keyout "$dir/author.pri" -out "$dir/author.csr"
  run /usr/bin/openssl req -new -newkey rsa:2048 -nodes -sha512 -subj "/CN=TizenSDK/emailAddress=$email" \
    -addext "subjectAltName=URI:URN:tizen:packageid=,URI:URN:tizen:deviceid=$duid" \
    -keyout "$dir/distributor.pri" -out "$dir/distributor.csr"
  samsung_api authors "$dir/author.crt" "$token" "$user" -F "csr=@$dir/author.csr;type=application/octet-stream"
  samsung_api distributors "$dir/device-profile.xml" "$token" "$user" -F privilege_level=Public \
    -F developer_type=Individual -F "csr=@$dir/distributor.csr;type=application/octet-stream"
  if grep -q '<Certificate>' "$dir/device-profile.xml"; then
    sed -n 's:.*<Certificate>\([^<]*\)</Certificate>.*:\1:p' "$dir/device-profile.xml" | base64 -D |
      /usr/bin/openssl x509 -inform DER -out "$dir/distributor.crt"
  else
    mv "$dir/device-profile.xml" "$dir/distributor.crt"
  fi
  unzip -p "$plugins"/org.tizen.common.cert_*.jar res/ca/vd_tizen_dev_author_ca.cer >"$dir/author-ca.crt"
  unzip -p "$plugins"/org.tizen.common.cert_*.jar res/ca/vd_tizen_dev_public2.crt >"$dir/distributor-ca.crt"
  password="$(/usr/bin/openssl rand -hex 16)"
  for role in author distributor; do
    run /usr/bin/openssl pkcs12 -export -name UserCertificate -inkey "$dir/$role.pri" -in "$dir/$role.crt" \
      -certfile "$dir/$role-ca.crt" -passout "pass:$password" -out "$dir/$role.p12"
  done
  rm -f "$dir"/*.pri
  run "$tizen" security-profiles add -n "streamvault-$duid" -a "$dir/author.p12" -p "$password" \
    -d "$dir/distributor.p12" -dp "$password"
  if [[ -f "$dir/device-profile.xml" ]]; then
    run "$sdb" -s "$serial" push "$dir/device-profile.xml" /home/owner/share/tmp/sdk_tools/device-profile.xml
  fi
}

install_wgt() {
  local stage="$out/tizen"
  [[ -n "$2" ]] || return 1
  run rm -rf "$stage"
  run cp -R "$public" "$stage"
  sed "s/<widget /<widget version=\"$version\" /" "$resources/tizen/config.xml" >"$stage/config.xml" ||
    die "Cannot write $stage/config.xml."
  raster 512 "$stage/icon.png"
  run "$tizen" package -t wgt -s "$2" -o "$out" -- "$stage"
  result="$("$tizen" install -s "$1" -n "$out/StreamVault.wgt" 2>&1)" || true
  printf '%s\n' "$result" >>"$log"
  [[ "$result" == *"successfully installed"* ]]
}

refused() {
  die "The TV refused the app: $(grep -i -m 1 -E 'fail|error' <<<"$result" || printf '%s' "$result")"
}

deploy_tizen() {
  local ip=$1 serial="$1:26101" host duid profile app result=""
  host="$(ipconfig getifaddr "$(route -n get "$ip" | awk '/interface:/ { print $2 }')")"
  tizen_tools
  wait_until "On the TV: Apps, type 1 2 3 4 5, set Developer mode On with Host PC IP $host, then restart the TV." \
    developer_mode "$ip" "$host"
  wait_until "Waiting for the TV to accept the connection (restart it if this lasts)…" sdb_online "$serial"
  duid="$("$sdb" -s "$serial" shell 0 getduid | tr -d '\r\n')"
  profile="streamvault-$duid"
  grep -q "<profile name=\"$profile\">" "$profiles" 2>/dev/null ||
    profile="$(sed -n 's/.*<profiles active="\([^"]*\)".*/\1/p' "$profiles" 2>/dev/null || true)"
  build_web
  app="$(sed -n 's/.*<tizen:application id="\([^"]*\)".*/\1/p' "$resources/tizen/config.xml")"
  "$sdb" -s "$serial" shell 0 was_kill "$app" &>/dev/null || true
  if ! install_wgt "$serial" "$profile"; then
    [[ "$profile" != "streamvault-$duid" && (-z "$profile" || "$result" == *ertificate*) ]] || refused
    samsung_certificate "$serial" "$duid"
    install_wgt "$serial" "streamvault-$duid" || refused
  fi
  "$sdb" -s "$serial" shell 0 was_execute "$app" &>/dev/null
}

adb_service() {
  "$adb" mdns services 2>/dev/null | awk -v ip="$1:" -v type="_adb-tls-$2" '$2 ~ type && index($3, ip) == 1 { print $3; exit }'
}

adb_online() {
  local target=$1 candidate pairing code
  for candidate in "$target" "$target:5555" $(adb_service "$target" connect); do
    [[ "$candidate" == *.*.*.*:* ]] && "$adb" connect "$candidate" &>/dev/null
    if [[ "$("$adb" -s "$candidate" get-state 2>/dev/null)" == device ]]; then
      serial="$candidate"
      return 0
    fi
  done
  pairing="$(adb_service "$target" pairing)"
  if [[ -n "$pairing" ]] && read -rp "  Pairing code shown on the TV: " code </dev/tty; then
    "$adb" pair "$pairing" "$code" &>/dev/null || true
  fi
  return 1
}

build_apk() {
  local package=$1 work="$out/android" jar="$android_home/platforms/android-$ANDROID_API/android.jar"
  local tools="$android_home/build-tools/$ANDROID_BUILD_TOOLS" keystore="$HOME/.android/debug.keystore" major minor patch
  IFS=. read -r major minor patch <<<"$version"
  mkdir -p "$work/res/drawable" "$work/classes" "$work/dex" "$work/assets" "$HOME/.android"
  cp -R "$public" "$work/assets/www"
  raster 512 "$work/res/drawable/icon.png"
  raster 180 "$work/res/drawable/banner.png"
  run sips -p 180 320 --padColor "$BACKGROUND" "$work/res/drawable/banner.png"
  [[ -f "$keystore" ]] || run keytool -genkeypair -keystore "$keystore" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
  run "$tools/aapt2" compile --dir "$work/res" -o "$work/res.zip"
  run "$tools/aapt2" link -o "$work/unsigned.apk" -I "$jar" -R "$work/res.zip" -A "$work/assets" \
    --manifest "$resources/android/AndroidManifest.xml" --min-sdk-version "$ANDROID_MIN_API" \
    --target-sdk-version "$ANDROID_API" --version-code $((major * 10000 + minor * 100 + patch)) --version-name "$version"
  run javac --release 17 -classpath "$jar" -d "$work/classes" "$resources/android/MainActivity.java"
  run "$tools/d8" --release --min-api "$ANDROID_MIN_API" --lib "$jar" --output "$work/dex" "$work/classes/${package//.//}"/*.class
  (cd "$work/dex" && run zip -q "$work/unsigned.apk" classes.dex)
  run "$tools/zipalign" -f 4 "$work/unsigned.apk" "$work/aligned.apk"
  run "$tools/apksigner" sign --ks "$keystore" --ks-pass pass:android --out "$out/StreamVault.apk" "$work/aligned.apk"
}

deploy_android() {
  local app webview
  app="$(sed -n 's/.*<manifest .*package="\([^"]*\)".*/\1/p' "$resources/android/AndroidManifest.xml")"
  android_tools
  wait_until "On the TV: enable Developer options (tap Build 7 times in About), turn on USB, network or wireless debugging (Fire TV: ADB debugging), and allow this Mac when asked." \
    adb_online "$1"
  webview="$("$adb" -s "$serial" shell dumpsys webviewupdate 2>/dev/null |
    sed -n 's/.*Current WebView package (name, version): (.*, \([0-9]*\)\..*/\1/p')"
  if [[ -n "$webview" ]] && ((webview < WEBVIEW_MIN)); then
    say "This TV has WebView $webview; StreamVault needs $WEBVIEW_MIN or later: update Android System WebView from the Play Store."
  fi
  build_web
  build_apk "$app"
  run "$adb" -s "$serial" install -r "$out/StreamVault.apk"
  run "$adb" -s "$serial" shell am start -n "$app/.MainActivity"
}

main() {
  rm -rf "$out"
  mkdir -p "$out" "$cache"
  local choice kind address
  if [[ -n "${TV:-}" ]]; then
    choice="$(discover | awk -F '\t' -v tv="$TV" '$2 == tv { print $1 "\t" $2; exit }')"
    [[ -n "$choice" ]] || die "TV $TV not found."
  else
    say "Looking for TVs…"
    choose
  fi
  IFS=$'\t' read -r kind address <<<"$choice"
  rust_tools
  load_sync
  "deploy_$kind" "$address"
  say "StreamVault $version installed on $address (sync $([[ -n "${STREAMVAULT_SYNC_URL:-}" ]] && echo on || echo off))."
}

main "$@"
