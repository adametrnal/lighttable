#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [[ ! -x "$project_dir/target/release/lighttable" ]]; then
  cd "$project_dir"
  bash scripts/cargo-local build --release --locked
fi
exec "$project_dir/target/release/lighttable" "$@"
