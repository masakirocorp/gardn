#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

mode="${1:-all}"
case "$mode" in
  all) ;;
  --dev-only) ;;
  *)
    printf 'usage: %s [--dev-only]\n' "$0" >&2
    exit 2
    ;;
esac

command -v cargo-zigbuild >/dev/null 2>&1 || {
  printf '%s\n' 'error: cargo-zigbuild is required to build Linux development workers' >&2
  exit 1
}

if [[ "$mode" == all ]]; then
  cargo build --release --package omh --locked
fi
cargo build --profile debugging --package omh --locked

linux_targets=(
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
)
for target in "${linux_targets[@]}"; do
  rustup target add "$target"
  cargo zigbuild --release --target "$target" --package omh --locked
done

debug_binary="$root/target/debugging/omh"
bin_dir="${HOME}/.local/bin"
install -d -m 755 "$bin_dir"
if [[ "$mode" == all ]]; then
  release_binary="$root/target/release/omh"
  install -m 755 "$release_binary" "$bin_dir/omh"
fi
install -m 755 "$debug_binary" "$bin_dir/omh-dev"

debug_cohort=$(
  "$debug_binary" execution-worker --build-info |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["build_cohort"])'
)
data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
install_workers() {
  local app_dir=$1
  local cohort=$2
  local worker_dir="$data_home/$app_dir/workers/$cohort"
  install -d -m 755 "$worker_dir"
  install -m 755 "$root/target/x86_64-unknown-linux-musl/release/omh" \
    "$worker_dir/omh-linux-x86_64"
  install -m 755 "$root/target/aarch64-unknown-linux-musl/release/omh" \
    "$worker_dir/omh-linux-aarch64"
}
install_workers omh-dev "$debug_cohort"
if [[ "$mode" == all ]]; then
  release_cohort=$(
    "$release_binary" execution-worker --build-info |
      python3 -c 'import json,sys; print(json.load(sys.stdin)["build_cohort"])'
  )
  install_workers omh "$release_cohort"
fi

if [[ "$(uname -s)" == Darwin ]]; then
  if [[ "$mode" == all ]]; then
    codesign --force --sign - "$bin_dir/omh"
  fi
  codesign --force --sign - "$bin_dir/omh-dev"
fi

env -u OMH_SOCKET_PATH -u OMH_CLIENT_SOCKET_PATH "$bin_dir/omh-dev" server stop >/dev/null 2>&1 || true
cargo clean

printf 'installed omh-dev with worker cohort %s\n' "$debug_cohort"
if [[ "$mode" == all ]]; then
  printf 'installed omh with worker cohort %s\n' "$release_cohort"
fi
