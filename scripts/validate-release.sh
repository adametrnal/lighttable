#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
output="${1:-$project_dir/dist/release}"
bash scripts/check.sh
bash scripts/cargo-local build --release --locked
work="$(mktemp -d /tmp/lighttable-release-check-XXXXXX)"
python3 scripts/make-demo-catalog.py "$work/catalog"
catalog="$work/catalog/Synthetic Demo.lrcat"
sha256sum "$catalog" > "$work/catalog.sha256"
target/release/lighttable --inspect "$catalog" --verify-previews > "$work/inspect.txt"
python3 scripts/package-release.py build "$output"
(cd "$output" && sha256sum --check SHA256SUMS)
python3 - "$output" "$work" <<'PY'
import hashlib,json,sys,tarfile
from pathlib import Path
output,work=map(Path,sys.argv[1:])
for archive in output.glob('*.tar.gz'):
    with tarfile.open(archive) as t: t.extractall(work/'unpacked',filter='data')
sources=[p for p in (work/'unpacked').glob('*/SOURCE-MANIFEST.json') if (p.parent/'Cargo.toml').exists()]
assert len(sources)==1, 'Expected one complete source archive'
for name,want in json.loads(sources[0].read_text()).items():
    assert hashlib.sha256((sources[0].parent/name).read_bytes()).hexdigest()==want,name
print('Extracted source archive matches its file manifest.')
PY
bundle="$output/bundle"
"$bundle/bin/lighttable" --version
python3 "$bundle/scripts/install-desktop.py" --data-home "$work/desktop"
desktop-file-validate "$work/desktop/applications/lighttable-catalog.desktop"
mkdir -m 700 "$work/runtime"
env -u WAYLAND_DISPLAY -u DISPLAY xvfb-run -a -s '-screen 0 1600x1000x24' \
  env WINIT_UNIX_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 XDG_RUNTIME_DIR="$work/runtime" \
  "$bundle/bin/lighttable" "$catalog" --smoke-test "$work/smoke"
test -f "$work/smoke/success.txt"
grep -q 'nested collection membership' "$work/smoke/success.txt"
grep -q 'Smart membership' "$work/smoke/success.txt"
sha256sum --check "$work/catalog.sha256"
cp "$work/smoke/success.txt" "$output/desktop-smoke.txt"
cp "$work/inspect.txt" "$output/synthetic-inspect.txt"
printf 'Release checks passed. Evidence: %s\n' "$work"
