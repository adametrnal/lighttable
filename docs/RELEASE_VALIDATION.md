# Release validation — v0.1.0

Recorded September 7, 2026. The Linux x86_64 alpha artifacts are prepared for
review for [adametrnal/lighttable](https://github.com/adametrnal/lighttable).
The checks below ran locally; the GitHub workflows have not run, and no release
has been published.

## Build and package results

The explicit public source export contains 86 files. It was built in Ubuntu
24.04.4 LTS with Rust/Cargo 1.98.1 and glibc 2.39, using the pinned toolchain and
locked Cargo dependencies. The first container build started with empty
container caches; subsequent recipe fixes and documentation updates reused
BuildKit caches. No host toolchain, personal catalog or local build directory
entered the build context.

| Check | Result |
| --- | --- |
| Formatting and Clippy | Passed; warnings denied |
| Rust tests | 27 passed; three opt-in tests excluded |
| Python tests | 12 passed, including source-export exclusion and rejection cases |
| Lightroom exchange tests | Passed under Lua 5.4 and LuaJIT |
| Optimized Linux build | Passed in Ubuntu 24.04; packaged `--version` reports 0.1.0 |
| Workflow syntax | Both workflows passed actionlint 1.7.12 |
| Synthetic catalog | All 96 originals resolve and all 96 indexed previews decode |
| Packaged desktop workflow | Passed in both the builder and a separate Ubuntu image containing runtime libraries but no compiler/development packages |
| Desktop launcher | Staged installer and `desktop-file-validate` passed |
| Source preservation | Synthetic catalog SHA-256 unchanged after the packaged desktop test |
| Source archive | Every file matches `SOURCE-MANIFEST.json`; exactly the 86 allowed files plus the manifest |
| Release assets | Linux tarball, source tarball and companion ZIP all pass `SHA256SUMS` verification |
| Dependency notices | Collected for 217 crates in the Linux dependency graph, embedded fonts, bundled SQLite and Rust libraries |
| Visual review | Public synthetic screenshot and runtime-only grid/Loupe screenshots inspected; images and UI render correctly |

Both desktop runs used Xvfb with X11 and software Mesa. They exercised grid,
Loupe, bottom-row navigation, saved ratings/picks, nested collection membership,
Smart Collection membership, zero-rating removal and undo. Generated imagery
contains no personal photographs. The public source and packages exclude real
catalogs, preview caches, selection databases, backups, toolchains and private
demo footage.

The prepared output is under `dist/release-0.1.0/`. Alongside the three archives
and checksums, it contains `desktop-smoke.txt`, `runtime-smoke.txt`,
`synthetic-inspect.txt`, `runtime-grid.png` and `runtime-loupe.png`. The Linux
bundle includes `BUILD-INFO.json`, source hashes and dependency notices.
See [the release procedure](RELEASING.md) to reproduce the checks.

## Scope and remaining work

This establishes an Ubuntu 24.04 x86_64 alpha baseline. It does not establish
compatibility with every distribution, GPU, preview format or Lightroom schema.
Wayland was exercised separately on the development desktop; the container
release checks use X11. The opt-in generated-file Trash and two external-catalog
tests were not rerun for release packaging; earlier results are recorded in
[validation history](VALIDATION.md).

Sustained browsing of the real library and Lightroom-side companion validation
remain outstanding. Direct catalog writing remains copy-only research. The
synthetic catalog is a Lighttable test fixture, not a complete Lightroom catalog.

Publication still requires pushing the reviewed source and version tag, waiting
for GitHub CI and the draft-release workflow, and reviewing the resulting draft.
