Lighttable 0.1.0 is the first Linux alpha for browsing and culling Lightroom Classic catalogs.

It includes a virtualized grid and Loupe, ordinary collections and sets, an experimental date/rating Smart Collection subset, local stars and flags, undo/redo, and reviewed moves to desktop Trash. Local selections are stored separately; the app reads catalog snapshots without applying SQL changes to the source catalog.

The Linux x86_64 tarball targets Ubuntu 24.04 (glibc 2.39+), with Little CMS 2 and OpenGL provided by the distribution. Extract it and run `bin/lighttable`. Python 3 is needed only for the optional desktop launcher installer. See the included README for runtime dependencies and storage behavior.

Assets include a Linux binary bundle, an explicit source archive, the experimental Lightroom companion ZIP and SHA-256 checksums. The binary bundle includes dependency/font notices and build provenance. Personal catalogs, photos, preview caches, demo footage and selection databases are excluded.

**Alpha limitations:** broad Lightroom/schema compatibility and sustained performance are unverified. Some preview formats and raw files are unsupported. The companion's parser and conflict tests pass, but Lightroom-side validation is incomplete. Direct catalog writing remains separate copy-only research and is not a production app feature. Use disposable catalog and file copies for initial testing.

Release checks cover formatting, Clippy, Rust/Python/Lua tests, a clean Ubuntu build, source/archive integrity, a synthetic 96-record catalog and a packaged desktop smoke test under X11 software rendering. See `docs/RELEASE_VALIDATION.md` for the actual results and any remaining limitations. Wayland has also been exercised on the development desktop; the container checks do not certify every GPU or distribution.
