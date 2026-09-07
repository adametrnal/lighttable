#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
bash scripts/cargo-local fmt --check
bash scripts/cargo-local clippy --all-targets --locked -- -D warnings
bash scripts/cargo-local test --locked
python3 -m unittest discover -s tests -p 'test_*.py'
lua tests/exchange_test.lua
if command -v luajit >/dev/null; then luajit tests/exchange_test.lua; fi
