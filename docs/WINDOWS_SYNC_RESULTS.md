# Windows sync experiment results

Windows run: September 6, 2026. Linux comparison: September 7 UTC (September 6 local time at receipt).

**The SQL prototype's rating/flag values work in the tested Lightroom views, but it does not reproduce native edit bookkeeping.** Small passed its selection, collection/filter and reopen checks. Vault retained all five intended selections, including independent master/virtual-copy values; cloud activity and unfinished UI checks prevent a clean full Vault pass. Production direct writing remains unimplemented.

## Evidence and verification

The returned environment was Lightroom Classic **15.5.1**, build **202608131348-cab7eed5**, on Windows 11 x64, reported OS version **11.0.26200**. The supplied and returned catalogs still report `Adobe_DBVersion=1504001` and `Adobe_CrsVersion=18.0.0`; those variables do not identify the running Lightroom build or the version strings within individual Develop settings.

The full `Lighttable-Linux-Handoff-2026-09-06.zip` was copied from the supplied NAS folder into `.test-data/sync-lab-2026-09-06/returned/`. Its 1,561,088,716 bytes match SHA-256 `fa479fe2c61580be6f0a3b9934ace84863fc4d2ed9b0cae3d011fdead2a00c21`. All 1,576 payload files were extracted into a new `windows-2026-09-06/` directory and checked against `RETURNED-MANIFEST.json`, including a reread of the extracted bytes. The evidence-only archive and its 58 payload files also passed their hashes.

The original Windows input manifest exactly matches the retained Linux package manifest. Its initial Windows verification records 511 matching files. Retained baseline and candidate catalog bytes still match the preparation receipts; the original Small control was identical to the Small baseline. Both reference inputs were verified identical to their baselines during preparation.

Analysis used nine private SQLite snapshots: both baselines, both original SQL candidates and five returned catalogs. Every snapshot passed `PRAGMA integrity_check`. Eleven comparisons hashed every table and the schema, preserving SQLite types, binary TEXT and row multiplicity. Changed tables were inspected by primary key or exact row multisets. A separate UUID-based check validated all image/file identities, filenames, master/virtual-copy distinctions and exact expected rating/flag values across the full catalogs. Source catalog/WAL and auxiliary bytes remained unchanged during analysis. The preserved returned WAL files are zero bytes; catalog locks are absent.

## Selection and UI results

| Check | Result |
| --- | --- |
| Small control | 32 images; zero selection changes after ordinary open/close |
| Small native reference | Exactly the four intended photos changed |
| Small SQL candidate | All four intended results; zero further selection changes across all 32 images |
| Small UI | Windows notes/screenshots record ordinary collection, rating Smart Collection, flagged/unrated filters, Develop appearance and close/reopen persistence passing |
| Vault native reference | Exactly the five intended photos changed among 79,393 images |
| Vault SQL candidate | All five intended results; zero further selection changes across all 79,393 images |
| Vault UI | All five values and Develop appearances checked; collection/Smart Collection tests and a second launch remain incomplete |
| Linux reading after return | Lighttable opens both candidates; all 32 Small standard previews decode |

Zero-star values are SQL NULL, flags retain their numeric values, and the `IMG_1477.CR2` master and `Copy 1` remain independent. Returned reference and candidate selections match across **every image**, not just the selected test group. No image was added or removed.

The supplied Small screenshots support the documented rating Smart Collection result: its rule is `rating >= 4`, despite its name `5star`. This does not validate all Smart rules, relative-date cutoffs or Vault collection behavior. Linux inspection used the returned package without mapping original photo roots; originals therefore resolve to zero paths there, while available previews remain readable.

## Native bookkeeping is not reproduced by reopening

Small supplies the useful no-edit control. All three returned Small catalogs changed `sidecarStatus` from 1 to 2 and lengthened `aiUpdateRequiredStatus` from 14 to 15 characters on all 32 images. They also gained 32 XMP-updater entries and the same auxiliary schema additions: `AgLibraryBackups`, `sqlite_stat1` and `sqlite_stat4`. These shared changes cannot be attributed to the SQL selections. All 32 Develop text values remain exactly equal to input.

Against this control, the native Small edits additionally:

- Updated `touchCount` and `touchTime` on the four edited images. Touch-count increments were 1, 1, 2 and 1; this run does not establish a fixed per-photo increment rule.
- Added four `AgLibraryImageSyncedAssetData` rows with payload key `/changedOnDevice`, one per edited image.
- Added `LrMobileSyncChangeCounter` row `id=1, changeCounter=6`.
- Left `AgMetadata_autoWriteXmpBlocked=1`, while control and candidate have 0. Automatic XMP writing itself is off in all three.

The SQL candidate still lacks those native tracking updates after reopening. Its image table differs from the returned native reference only in the four photos' touch counters/times; their visible selections match. Native tracking rows exist even in this Small test without active cloud sync. Their presence does not by itself establish upload behavior, and simply copying these observed constants would not implement Lightroom's rules.

`Adobe_AdditionalMetadata`, collection membership and collection-image flags are unchanged throughout Small. View-state, proxy timestamps, relative root hints and SQLite statistics also changed during Windows use and are recorded separately in the private reports.

| Input → returned comparison | Tables differing | Schema |
| --- | --- | --- |
| Small baseline → control | 11 | 134 → 137 tables |
| Small baseline → native reference | 13 | 134 → 137 tables |
| Small original SQL candidate → returned candidate | 11 | 134 → 137 tables |
| Vault baseline → native reference | 17 | Same 138 tables and schema |
| Vault original SQL candidate → returned candidate | 18 | Same 138 tables and schema |

These counts include statistics and newly created tables; a separate schema fingerprint is not counted as a table. Private reports match newly allocated primary keys literally. For tables whose IDs were allocated independently, those row-change counts are not necessarily counts of semantic changes; interpret variables by name and asset records by their photo/file identity.

## Vault Develop changes and cloud activity

All five target **complete Develop rows** are identical between the returned native reference and SQL candidate, including their text and digest. Both differ from their supplied input in the same way. The original SQL candidates, before Windows, changed only `Adobe_images` rating/pick cells and preserved every Develop cell.

The target differences include `Version` changing to `18.5.1`, `AILook` additions, explicit grayscale mixer values replacing `AutoGrayscaleMix`, a camera-profile digest change on the JPEG, and added white-balance fields on the recent DNG. `IMG_2063.dng` has a larger rewrite from version `7.0`, including removed legacy tone-curve fields and added newer defaults. Identical reference/candidate results are consistent with Lightroom updating stored settings during use. They do **not** establish that every rewrite is visually or semantically neutral relative to the original input. Windows reports matching reference/candidate appearance; Linux inspection of the saved `IMG_2063` screenshot pair agrees. The Small cached-Library versus Develop brightness difference remains unexplained.

Outside the targets, the Vault copies diverge materially:

| Observation versus the matching supplied input | Native reference | SQL candidate after Windows |
| --- | --- | --- |
| Develop text records changed | 123 | 102 |
| Image change-counter rows changed | 2,382 | 2,343 |
| Additional-metadata XMP records changed | 25 | 25 |
| New Develop history steps | 0 | 10 |
| Smart Preview proxy records removed | 1,610 | 1,610 |

All ten new candidate history steps are named **`From Lr mobile`** and affect photos outside the test group. Two candidate Develop records also change `processVersion`; 28 non-target image rows change `touchTime`. There are 165 differing Develop text records between the returned reference and candidate, while all five target texts match. The same 25 XMP changes appear in both copies. Asset metadata, cloud revision records, updater queues and folder/view state also changed.

The history labels provide direct evidence of incoming mobile Develop history, strengthening the Windows notes' cloud-sync caveat. They do not explain every difference. Different browsing exposure and version conversion can also affect records, and there is no Vault no-edit control. Do not use these global differences as a recipe for the rating/flag writer or claim whole-catalog Develop preservation after Windows.

## Auxiliary data is present

The handoff note incorrectly says no `.lrcat-data` was found. **All five returned catalogs include those directories**, containing 12/12/14 files for Small control/reference/candidate and 17 files each for Vault reference/candidate. Their bytes match the returned manifest. The original `.sst` and `.blob` files remain byte-identical in every returned folder. Logs, manifests, options and other files changed; Small candidate also gained a new `.sst` file.

This resolves the inventory concern, not every possible auxiliary semantic question. RocksDB logical contents were not decoded, and equality of retained payload files alone does not prove unchanged auxiliary state. The original input folders and complete return remain intact.

## Implementation consequence and next work

Keep direct sync as a copy-only experiment. The evidence supports local selection visibility, zero clearing, virtual-copy independence and Small reopen persistence. It establishes that Lightroom does not automatically recreate all native tracking merely by opening the SQL candidate. The effect on later metadata export, publishing or cloud propagation is still untested.

Further direct-writer work needs a controlled, offline native-edit baseline that isolates bookkeeping after any version conversion, followed by recovery/interruption checks and integration with pending local selections. Record the actual Lightroom build as well as the catalog schema. A schema-version string alone did not distinguish the observed conversion behavior. The companion's Lightroom setters remain the alternative, but their actual SDK execution still needs validation. Further Windows UI work was stopped at the user's request in the handoff; it has not been restarted here.

Private evidence and reproducible analysis live under `.test-data/sync-lab-2026-09-06/returned/`:

- `extraction-verification.json`, `windows-2026-09-06/RETURNED-MANIFEST.json`, notes, audits and screenshots.
- `analyze_return.py`: private snapshots, typed fingerprints, eleven table comparisons and target Develop diffs.
- `verify_semantics.py`: independent whole-catalog identity/selection checks and targeted interpretation.
- `analysis/summary.json`, `semantic-verification.json`, `source-preservation.json`, `fingerprints.json` and per-comparison JSON files.

Both scripts intentionally require fresh report outputs. Retain the original run and choose a new output location before rerunning. Nine existing Python lab regression tests pass. No production writer, source catalog or photograph was changed by this Linux analysis.
