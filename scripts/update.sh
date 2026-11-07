#!/usr/bin/env bash

set -euo pipefail

command -v cargo-upgrade >/dev/null || cargo install cargo-edit --locked

cargo upgrade -i allow --exclude x509-cert

cargo update
