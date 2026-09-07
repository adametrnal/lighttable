# Contributing to Lighttable

Build instructions and supported features are in [README.md](README.md). Run
`bash scripts/check.sh` before submitting a change. Tests use generated fixtures;
they do not require a personal Lightroom catalog. Use the pinned Rust toolchain.

Include the concrete problem, resulting behavior and relevant checks in a pull
request. Keep source catalogs read-only, local selections separate and sync
explicitly experimental. Do not introduce deletion outside the existing reviewed
Trash flow.

For a bug report, include Lighttable's version, Linux distribution, desktop
session (Wayland/X11), catalog schema from `--inspect`, and reproduction steps.
Use a synthetic fixture when possible. Do not attach personal catalogs, previews,
original photographs, selection databases, exported selections or credentials.
Review screenshots and logs for private paths and metadata before posting.

Changes to preview decoding, catalog adaptation or sync need fixtures that
exercise both success and failure. See [release preparation](docs/RELEASING.md)
for packaging, dependency notices and desktop verification.

Contributions are provided under the project's [MIT license](LICENSE).
