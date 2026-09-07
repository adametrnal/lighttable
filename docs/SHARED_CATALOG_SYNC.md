# Shared-catalog sync investigation

The workflow alternates between Linux and Windows on the same PC. The September 6 Windows test used Lightroom Classic 15.5.1, build `202608131348-cab7eed5`. Supplied and returned catalogs report schema `1504001` and Camera Raw token `18.0.0`, even though individual Develop settings were rewritten with newer version strings.

The copy-only prototype changes ratings and flags correctly in SQLite, Lighttable and the tested Lightroom views, including zero clearing and separate virtual-copy identities. **The Windows return confirms Small reopen persistence but exposes missing native edit bookkeeping.** Vault also received cloud changes and has incomplete collection/reopen coverage. Read the [Windows results](WINDOWS_SYNC_RESULTS.md) for the full-table comparisons and limits. The app continues to store selections separately and retains its companion plug-in; there is no in-place catalog-write command or button.

## Options

| Route | Fit | Tradeoff or remaining work |
| --- | --- | --- |
| Direct update of a closed catalog | Could show Linux selections on the next Windows launch without an import; retains independent virtual copies | Establish complete Lightroom write behavior, reopening, conflict handling and recovery; requires a healthy writable shared filesystem |
| Existing Lightroom companion | Calls Lightroom's own rating/pick setters, letting Lightroom handle bookkeeping | Requires an import inside Lightroom; parser/conflict tests pass, but actual SDK execution remains untested here |
| XMP | Current Lightroom supports stars and flags | Needs careful metadata merging and reading changes into Lightroom; cannot independently represent virtual-copy selections |

Adobe documents the [Lua SDK as its extension interface](https://developer.adobe.com/lightroom-classic/). No Adobe specification for directly updating these internal SQL tables was located. Adobe added [XMP flag support in Lightroom Classic 13.2](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/flag-label-rate-photos.html), so older advice that flags never leave the catalog is outdated. However, [virtual-copy changes remain catalog-specific](https://jkost.com/blog/2024/08/working-with-virtual-copies-in-lightroom-classic.html), and [reading metadata from files can overwrite catalog settings](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/advanced-metadata-actions.html). An XMP implementation would need to preserve existing Develop metadata and both star/flag values. It is not the preferred route for Vault's 1,128 virtual copies.

## Database evidence

These observations came from private snapshots; they are not a general Lightroom schema specification.

| Area | Observation | Consequence |
| --- | --- | --- |
| Selection values | `Adobe_images.rating` and `.pick`; image/file UUIDs are distinct | Update a specific image, validating both UUIDs, filename and virtual-copy status |
| Unrated encoding | Vault has 68,782 NULL ratings; other ratings use REAL values | Candidate clears stars with SQL NULL; nonzero ratings and flags use REAL values |
| Edit tracking | `touchCount`, `touchTime`, `sidecarStatus`, `propertiesCache` | Preserve until native before/after edits establish behavior |
| Metadata state | `Adobe_AdditionalMetadata` holds dirty status, XMP and synchronization digests/timestamps | Changing only two image columns may omit required bookkeeping |
| Worklists | XMP updater/save, harvested-metadata and publish queues are empty in this snapshot | Empty queues do not establish their behavior during an edit |
| Cloud tracking | Image/collection change counters and `LrMobileSyncChangeCounter` exist; Vault has cloud sync enabled in saved settings | Counter semantics and later propagation need testing |
| Collection flags | All 12,329 collection-image picks are zero; global image flags include 77 picks and 9 rejects | Do not blindly copy global flags into collection rows; check both views in Lightroom |
| Triggers | Neither supplied catalog has triggers targeting `Adobe_images` | SQLite will not automatically perform Lightroom's application-level bookkeeping |
| Binary data | Some Vault payloads are non-UTF-8 bytes stored as TEXT | Verifier retains the bytes and distinguishes TEXT from BLOB without normalization |

## Copy-only lab

`scripts/catalog_sync_lab.py` uses Python's standard library and the app's existing version-1 TSV export:

```bash
python3 scripts/catalog_sync_lab.py prepare \
  --catalog '/path/to/closed catalog.lrcat' \
  --changes '/path/to/changes.tsv' \
  --output '/path/to/new-experiment-folder'

python3 scripts/catalog_sync_lab.py compare \
  --before '/path/to/before.lrcat' \
  --after '/path/to/after.lrcat' \
  --changes '/path/to/changes.tsv' \
  --report '/path/to/new-comparison.json'
```

Preparation creates `baseline/` for comparison/recovery, an unchanged `reference/` for native Lightroom edits, and a SQL-modified `candidate/`. Each has matching `.lrcat-data`. Source catalogs are read only as file bytes. Committed WAL is recovered on a private copy, then SQLite's backup API emits a standalone catalog. The tool refuses Lightroom locks and rollback journals, checks source signatures/checksums, requires a new output directory and restricts writes to the observed schema. Failed preparation retains `INCOMPLETE.txt` and does not promote an unverified candidate to a `.lrcat` file.

For each requested field, the current value must equal its baseline or already equal the desired result. Identity mismatches or conflicts skip the whole photo; unrelated fields remain intact. Matching results need no update. The export's path-based catalog key is recorded for diagnostics, not used as a cross-machine identity check.

Changes run in one SQLite transaction. Verification checks integrity and exact desired values, then substitutes only approved cells' original values and hashes every table and schema, preserving SQLite types. Matching hashes establish that every other logical cell is unchanged. Auxiliary copies and source bytes are also checked. These checks establish the narrow changes, not their sufficiency for Lightroom.

The comparison command identifies changed tables, shows image/metadata/counter rows for the cohort, and reports changed auxiliary files. RocksDB byte changes can result from normal reopening/compaction; interpret them alongside visible Develop behavior.

## Local validation before Windows

Experiments live in `.test-data/sync-lab-2026-09-06/`, excluded from version control.

| Check | Result |
| --- | --- |
| Small catalog | Four changes; every other cell across 134 tables and all auxiliary files preserved |
| Vault catalog | Five changes, including a master/virtual-copy pair; every other cell across 138 tables and all auxiliary files preserved |
| Whole-catalog comparison | Only `Adobe_images` differs between Vault baseline and SQL candidate; auxiliary files match |
| Repeat planning | All five Vault results already match |
| Lighttable reading | Both real-catalog SQL comparison tests pass on the modified Vault candidate, including Smart rules |
| Small-catalog previews | Standard/Smart caches included; Lighttable decodes all 32 standard previews |
| Routine checks | 26 Rust tests, formatting, Clippy with warnings denied, nine Python lab tests and Lua/LuaJIT tests pass |

Lab tests cover zero clearing, virtual-copy independence, unrelated-field preservation, whole-photo conflicts, identity mismatches, malformed exports, repetition, locks, unknown schemas, non-overwriting output, committed WAL without source changes, trigger side effects, exact final values, transaction rollback, binary TEXT and bookkeeping/auxiliary comparisons. The subsequent Lightroom run and Linux return analysis are recorded in [Windows results](WINDOWS_SYNC_RESULTS.md).

## Original Windows protocol and remaining work

The returned test has been analyzed. Small completed this protocol; Vault stopped before collection and second-launch checks. Further Windows UI testing remains stopped per the handoff. The following instructions preserve the original experiment design rather than request a new run. [Windows results](WINDOWS_SYNC_RESULTS.md) record native bookkeeping gaps, verified auxiliary data and cloud contamination.

`Lighttable-Windows-Sync-Test.zip` contains both reference/candidate pairs, a small no-op control catalog, auxiliary data, expected values and a plain-text guide. Baselines stay on Linux. Small-catalog standard/Smart Previews are included; Vault retains its existing Windows `D:/Lightroom/Lightroom/…` paths. Originals are not bundled or moved. The archive includes per-file SHA-256 hashes and has a separate archive checksum.

1. Copy the archive to Windows-readable storage and extract to a local Windows disk. Pause Lightroom cloud sync and disable automatic XMP writing before editing. Do not switch cloud sync to a test catalog.
2. Open and close the small **control** without changing selections, to capture normal reopen effects. Then open the small **reference** directly and make the four listed selections, one photo at a time. The cohort includes combined, rating-only and flag-only changes. Close Lightroom.
3. Open the small **candidate**, verify its existing values and preview appearance, then close and reopen to check persistence. Leave its selections unchanged.
4. Repeat with Vault's five selections. Distinguish the master `IMG_1477.CR2` in `2009/2009-05-03` from `Copy 1`. Check zero clearing, collection/Smart views and unchanged Develop appearance. Do not use metadata Read/Save, move/delete photos or import the test catalog into the main one.
5. Record the exact Lightroom build and unexpected warnings. Close Lightroom and retain each catalog with `.lrcat-data` and any remaining WAL. Return the folders to Linux under a separate `returned/` location, preserving the original test inputs.

Compare baseline → native reference to discover Lightroom's writes, then original candidate → reopened candidate to identify reopening changes. For example:

```bash
python3 scripts/catalog_sync_lab.py compare \
  --before '.test-data/sync-lab-2026-09-06/vault/baseline/Lighttable Sync Test.lrcat' \
  --after '/path/to/returned/vault/reference/Lighttable Sync Test.lrcat' \
  --changes '.test-data/sync-lab-2026-09-06/vault/changes.tsv' \
  --report '/path/to/new-native-reference-report.json'
```

Recover an experiment by re-extracting the pristine archive or copying a retained baseline. Production direct writes still need validated Lightroom bookkeeping, backup/restore and interruption tests, reviewed conflict handling, integration with pending local selections, and exact schema/version gating.

Vault remains **read-only** on Linux (`fuseblk`, `ro`, checked this session). Its archived stale lock/WAL were retained in the earlier backup; this experiment uses the clean local working snapshot. Production writes require a healthy writable mount and a demonstrably closed catalog. Mount options and source lock files were not changed.
