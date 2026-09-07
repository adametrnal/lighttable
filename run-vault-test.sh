#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
test_dir="$project_dir/.test-data/vault-2026-09-05"
catalog="$test_dir/working/Lightroom Catalog.lrcat"
if [[ ! -r "$catalog" ]]; then
  echo "Prepared Vault test catalog not found: $catalog" >&2
  exit 1
fi
exec "$project_dir/run.sh" "$catalog" --data-dir "$test_dir/state" "$@"
