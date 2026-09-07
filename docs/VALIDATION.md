# Linux alpha validation

Recorded September 5, 2026. Release: Lighttable 0.1.0.

## Release preparation — September 7, 2026

The v0.1.0 public source export and Linux bundle pass the clean Ubuntu 24.04 release recipe: formatting, Clippy, 27 Rust tests, 12 Python tests, Lua/LuaJIT checks, all 96 synthetic preview decodes, source/archive checksums, staged launcher validation and packaged desktop tests in both build and runtime-only images. Runtime grid and Loupe screenshots were inspected. Both GitHub workflows pass actionlint; they have not run on GitHub yet. See [release validation](RELEASE_VALIDATION.md) for evidence and limits. Publication remains pending.

## Scrolling demo — September 7 UTC

The latest native recording shows ten real-time seconds in **Smart Collections > 4 or 5 stars**, containing 1,392 records from the 79,393-record Vault catalog. After warming the native grid from top to bottom and back, it records five cycles of one-second rapid downward scrolling and one-second Loupe viewing. All 300 capture slots were present; the H.264 MP4 is 2160 × 1410 at 30 fps, with a verified 10.000-second duration and successful full decode. Recorded offsets confirm the warm-up, downward-only recording and five stops. Rendered toolbar pixels confirm five exact 30-frame Loupe intervals; all five main photos are decoded, and the clip ends on the last photo. The starting grid, five Loupe photos and final frame were visually inspected. The catalog hash is preserved, and isolated recording storage contains zero selection changes or file operations. The bounded cache and preview gaps remain visible during fast scrolling. This does not validate cold decoding, all preview formats or sustained performance.

Formatting, Clippy with warnings denied, all 27 standard Rust tests, nine Python lab tests, Lua/LuaJIT exchange tests and the release build pass after this revision. The preceding ordinary Vault desktop smoke test passed; screenshots and `success.txt` are under `/tmp/lighttable-after-showcase-20260907/`. Recording CLI checks reject missing dependencies between options and prevent output overwrites.

## Shared-catalog sync experiment

The copy-only lab passes nine Python tests alongside the previously recorded Rust/Lua checks. Before Windows, it applied four changes to Small and five to Vault, preserving every unrelated cell across 134/138 tables and all auxiliary bytes. Both existing catalog SQL comparison tests passed against the modified Vault candidate.

The September 6 Windows return from Lightroom Classic 15.5.1 has now been verified and analyzed: the full ZIP and all 1,576 payload files match their hashes. Eleven full-table comparisons and an independent UUID-based check confirm every expected selection across 32/79,393 photos. Small's collection/filter and reopen checks passed. Both candidates reopen in the Linux reader, and all 32 returned Small standard previews decode. Nine Python lab tests were rerun successfully; Rust/Lua implementation was unchanged.

Native Lightroom edits also changed touch/sync tracking that the SQL candidate lacks. Vault has ten new non-target `From Lr mobile` Develop history steps and broader cloud-related differences; collection and second-launch checks remain incomplete. Target Develop rows match the returned native reference, but whole-catalog Develop preservation after Windows is not established. All five `.lrcat-data` directories are present, correcting the handoff inventory note. Sources remain unchanged by analysis. See [the full results and limits](WINDOWS_SYNC_RESULTS.md).

## Logo update

The original slide-mount SVG now supplies the header mark, native window icon and desktop icons. The preview sheet was visually inspected on light/dark backgrounds and at 16–128 pixels; every exported PNG was decoded and its dimensions/transparency checked. The header was also visually inspected in the actual Vault catalog window. The 32-pixel header uses a 128-pixel texture for high-DPI rendering.

The release build, formatting, Clippy with warnings denied, all 26 standard Rust tests, and Lua/LuaJIT exchange tests pass. The Vault desktop smoke test passes with isolated state; screenshots and `success.txt` are in `/tmp/lighttable-logo-vault-20260906`. The three opt-in tests were not rerun for the artwork change.

The desktop launcher and all ten icon assets were staged under `/tmp/lighttable-desktop-check`; `desktop-file-validate` passes. They were then installed to the user's applications/icon directories. GIO resolves the installed launcher and icon name, and all ten installed icon files match the project assets byte-for-byte. GIO also successfully launched a disposable fixture through the generated desktop entry with spaces, quotes, dollar signs, backticks and backslashes in its executable path. The installer rejects percent signs after the same check exposed GIO's inability to resolve that path; it also rejects equals signs and control characters. Dock/menu presentation varies with the desktop's icon cache and has not been visually verified. See [branding notes](BRANDING.md) for regeneration and installation.

## Smart Collection update

The current build passes 26 Rust tests, formatting, Clippy with warnings denied, and the companion tests under Lua and LuaJIT. Three tests are opt-in: generated-file Trash and two external-catalog checks. Both catalog checks were explicitly run against both supplied catalogs. All 15 supported Smart Collections matched exact UUID results from a separate SQL evaluation at the current local date (2026-09-05) and a frozen historical date. Recalculating the 15 collections across 79,393 records measured p95 6.00 ms in the release build; this excludes UI/decoding. [Smart Collection notes](SMART_COLLECTIONS.md) define the supported rules and remaining Lightroom comparison.

Desktop smoke tests passed on both catalogs, exercising a Smart Collection, zero-rating removal and undo in isolated stores. Screenshots and success reports are in `/tmp/lighttable-smart-vault-20260906` and `/tmp/lighttable-smart-small-20260906`; both Smart Collection screenshots were visually inspected.

## Initial collection update — September 6, 2026

The updated build passes formatting, Clippy with warnings denied, 19 Rust tests and the companion tests under Lua and LuaJIT. The generated-file Trash test and the external-catalog comparison remain opt-in; the latter was explicitly run successfully against both supplied catalogs. It verified exact UUID membership/counts for all 202 ordinary Vault collections and both small-catalog collections, plus every visible parent link. [Collection notes](COLLECTIONS.md) record the counts, schema observations, timing scope and remaining Lightroom-side visual comparison.

Desktop smoke tests passed on both catalogs with isolated selection stores, including collection membership and rating/pick saving. Final grid, loupe, bottom-row and collection screenshots were captured under `/tmp/lighttable-collections-vault-verified-20260906` and `/tmp/lighttable-collections-small-verified-20260906`; the collection screenshots were visually inspected. The nested sets, selected collection, membership count, metadata filters and photos render correctly. At that stage Smart and saved print types were unsupported; the later Smart update above enables the date/rating subset.

## Environment

The initial alpha was built on the supplied x86_64 Linux workstation (CachyOS kernel 7.2.2) with project-local Rust 1.98.1. The app uses egui/eframe 0.33.3 and its OpenGL renderer; exact dependency versions are in Cargo.lock. Native desktop launches used the current Wayland session. The later release checks above add Ubuntu 24.04 and X11 software rendering coverage.

## Completed checks

| Check | Result |
| --- | --- |
| Release build | Passed |
| rustfmt check | Passed |
| Clippy, all targets, warnings denied | Passed |
| Standard Rust tests | 14 passed; desktop Trash test excluded from routine runs |
| Grid navigation regression tests | Five tests use real egui layout across frames: visible lower rows stay still, offscreen rows scroll minimally, bottom rows never paint an overscrolled frame, filtering clamps before drawing, and tall cards/window resizing stay stable |
| Desktop Trash integration test | Passed separately: one generated file moved to Trash, found by exact original path, restored, and checked byte-for-byte |
| Lua exchange tests | Passed on system Lua and LuaJIT, including clear-to-zero, conflicts, repeated import, identity mismatch, duplicates and malformed input |
| Supplied catalog reading | 32/32 photo records; 32/32 original paths resolve |
| Supplied rendered previews | 32/32 photos decoded successfully through the app image pipeline |
| Portrait handling | The sample's `DA` orientation agrees with original DNG EXIF orientation 8; rendered output is portrait |
| Actual Linux window | Launched and rendered grid/loupe; screenshots inspected, including stable grid positions before/after adjacent bottom-row navigation |
| Local selection save | Native smoke test saved a five-star rating and pick in isolated state; SQLite assertions/tests cover restart persistence and explicit zero values |
| Source preservation | Synthetic tests compare main database bytes and committed WAL bytes before/after snapshot loading; source is never opened by the app's SQLite connection |
| 100k metadata filter | 30 iterations reported p50/p95 around 0.08 ms for a rating/flag filter on this machine |

The 100k dataset repeats metadata derived from the small sample with synthetic ratings/flags. That number measures the filter function, not cold loading, unique image decoding, UI frame times, or the real 80,000+ collection. The small catalog's observed read times were approximately 1–2 ms. These are local observations, not supported-hardware performance promises.

A subsequent test loaded the real 79,393-record Vault catalog from a verified local working copy and rendered grid/loupe and bottom-row navigation. See [Vault test notes](VAULT_TEST.md) for backup details, path resolution, preview coverage, and the initial warm measurements. Sustained browsing and full preview compatibility remain unmeasured.

## Automated regression coverage

Tests exercise SQLite snapshot handling with committed WAL contents, source-byte preservation, required-schema/lock failures, NULL ratings, APEX conversion, portable path resolution, virtual-copy identity, saved-selection catalog namespaces, restart persistence, transactional batch rollback, non-overwriting export, malformed DNG bounds, unsupported orientation reporting, changed-file review invalidation, and symlink exclusion.

The original grid unconditionally aligned the selected row to the top on each arrow press. Four navigation regressions were reproduced against that behavior before implementing minimal reveal and clamping offsets before layout. Tests compare the actual card rectangles on the navigation frame and subsequent frames, since checking only egui's saved scroll offset misses its correction after painting. The native smoke test also captures `grid-bottom.png` and `grid-bottom-next.png` around navigation to an adjacent photo in the last row.

The optional Trash integration test creates its own temporary file within the project filesystem and restores it immediately. No supplied photograph is deleted or moved by the automated test suite or desktop smoke test.

## Reproduce

```bash
cd lighttable
bash scripts/check.sh
bash scripts/cargo-local build --release --locked
target/release/lighttable --inspect "/path/to/Test Catalog.lrcat" --verify-previews
target/release/lighttable --benchmark "/path/to/Test Catalog.lrcat" 100000
target/release/lighttable "/path/to/Test Catalog.lrcat" --smoke-test /tmp/lighttable-smoke
```

To explicitly run the generated-file Trash integration test:

```bash
bash scripts/cargo-local test desktop_trash_round_trip -- --ignored
```

## Manual tests for the photographer

1. Launch `run.sh`; browse the grid, double-click a photo, navigate with arrows, and try preview zoom/panning.
2. Set different ratings with `0–5`, use `P/X/U`, filter to picks, and confirm that undo/redo returns the expected photos.
3. Select a range with Shift-click; verify that the selected count matches the intended batch before applying ratings.
4. Close and reopen the app; confirm selections remain.
5. Try Trash on expendable copies and restore through the desktop file manager. Catalog entries should remain visible from previews.
6. Install the companion into Lightroom on the other computer. Import changes into a disposable matching catalog; verify ratings/flags, repeated import, a deliberately conflicting Lightroom change, and unchanged Develop settings.
7. Close Lightroom, copy the changed catalog back to the original Linux path, and refresh. Imported matching changes should no longer be pending.

Real Lightroom SDK execution, exported-catalog UUID correspondence, mirrored images, old preview formats, disconnected/network Trash, partial-failure/crash fault injection, custom monitor profiles, and full-catalog memory/frame-time measurements remain to be validated. Synthetic compatibility tests do not establish those behaviors.
