#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

command -v cargo-zigbuild >/dev/null 2>&1 || {
  printf '%s\n' 'error: cargo-zigbuild is required to build Linux development workers' >&2
  exit 1
}

cargo build --release --package omh --locked
cargo build --profile debugging --package omh --locked

linux_targets=(
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
)
for target in "${linux_targets[@]}"; do
  rustup target add "$target"
  cargo zigbuild --release --target "$target" --package omh --locked
done

release_binary="$root/target/release/omh"
debug_binary="$root/target/debugging/omh"
bin_dir="${HOME}/.local/bin"
install -d -m 755 "$bin_dir"
install -m 755 "$release_binary" "$bin_dir/omh"
install -m 755 "$debug_binary" "$bin_dir/omh-dev"

cohort=$(
  "$release_binary" execution-worker --build-info |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["build_cohort"])'
)
data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
for app_dir in omh omh-dev; do
  worker_dir="$data_home/$app_dir/workers/$cohort"
  install -d -m 755 "$worker_dir"
  install -m 755 "$root/target/x86_64-unknown-linux-musl/release/omh" \
    "$worker_dir/omh-linux-x86_64"
  install -m 755 "$root/target/aarch64-unknown-linux-musl/release/omh" \
    "$worker_dir/omh-linux-aarch64"
done

if [[ "$(uname -s)" == Darwin ]]; then
  codesign --force --sign - "$bin_dir/omh"
  codesign --force --sign - "$bin_dir/omh-dev"
fi

"$bin_dir/omh-dev" server stop >/dev/null 2>&1 || true
cargo clean

printf 'installed omh and omh-dev with worker cohort %s\n' "$cohort"
