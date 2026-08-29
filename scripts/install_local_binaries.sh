#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

mode="${1:-}"
case "$mode" in
  "" | --dev-only) ;;
  *)
    printf 'usage: %s [--dev-only]\n' "$0" >&2
    printf '%s\n' 'error: this script only installs gardn-dev. Install production gardn from a GitHub release.' >&2
    exit 2
    ;;
esac

command -v cargo-zigbuild >/dev/null 2>&1 || {
  printf '%s\n' 'error: cargo-zigbuild is required to build Linux development workers' >&2
  exit 1
}

cargo build --profile debugging --package gardn --locked

linux_targets=(
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
)
for target in "${linux_targets[@]}"; do
  rustup target add "$target"
  cargo zigbuild --release --target "$target" --package gardn --locked
done

debug_binary="$root/target/debugging/gardn"
bin_dir="${HOME}/.local/bin"
install -d -m 755 "$bin_dir"
install -m 755 "$debug_binary" "$bin_dir/gardn-dev"

debug_cohort=$(
  "$debug_binary" execution-worker --build-info |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["build_cohort"])'
)
data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
worker_dir="$data_home/gardn-dev/workers/$debug_cohort"
install -d -m 755 "$worker_dir"
install -m 755 "$root/target/x86_64-unknown-linux-musl/release/gardn" \
  "$worker_dir/gardn-linux-x86_64"
install -m 755 "$root/target/aarch64-unknown-linux-musl/release/gardn" \
  "$worker_dir/gardn-linux-aarch64"

if [[ "$(uname -s)" == Darwin ]]; then
  codesign --force --sign - "$bin_dir/gardn-dev"
fi

env -u GARDN_SOCKET_PATH -u GARDN_CLIENT_SOCKET_PATH "$bin_dir/gardn-dev" server stop >/dev/null 2>&1 || true
cargo clean

printf 'installed gardn-dev with worker cohort %s\n' "$debug_cohort"
