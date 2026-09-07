# Vault catalog test

Prepared September 5, 2026, after confirmation that the Windows machine was completely shut down. The Vault volume is NTFS and mounted read-only on Linux.

## Run the prepared copy

```bash
./run-vault-test.sh
```

The launcher opens `.test-data/vault-2026-09-05/working/Lightroom Catalog.lrcat` and keeps this test's selections in `.test-data/vault-2026-09-05/state`. These local files are excluded from version control. Automated smoke-test edits live separately under `/tmp/lighttable-vault-smoke-20260905/state`.

## Backup and paths

The original remains at `/mnt/photo-library/Lightroom/Lightroom Catalog.lrcat`.

`.test-data/vault-2026-09-05/backup` contains verified byte copies of the catalog, its remaining WAL, SHM and lock files, the auxiliary `.lrcat-data` directory, and the preview index database. `manifest.json` records the source location, sizes, modification times and SHA-256 checksums. Backup payload files are read-only. This is a catalog-data backup; it does not duplicate original photos, preview image payloads, Smart Previews, Helper or Sync caches, and it is stored on this same workstation.

SQLite recovered committed WAL contents only in a disposable local copy. Its backup API produced the standalone working database; `PRAGMA quick_check` returned `ok`. The stale lock was retained in the archival backup and was never removed from Vault. The working catalog has no lock file.

No stored root paths or photo identifiers were rewritten. The working folder's `Lightroom` and `Lightroom Catalog Previews.lrdata` links point to the corresponding folders on Vault. These preserve the catalog-relative original paths and allow previews to be read in place. Keep Vault mounted at its current location while browsing this copy. Auxiliary catalog data was also copied into the working folder.

The main root references `D:/Lightroom/Lightroom/`, with the relative hint `Lightroom/`. All 79,003 catalog records using this root resolve to files through the local link. The other 390 records refer to Windows E: or C: locations that are not mapped here. Moving the catalog did not change those stored references.

## Observations

| Check | Result |
| --- | --- |
| Catalog items loaded | 79,393, including 1,128 virtual copies and 1,399 video records |
| Source database version | 1504001 |
| Catalog-data backup size | 932,683,042 bytes, excluding the manifest |
| Resolvable original paths | 79,003 records; counts include virtual copies sharing originals |
| Supported preview index entries | 5,713 records; other records depend on supported original/companion fallback |
| Metadata load reported by release build | 306 ms in one run |
| Full CLI inspection | About 0.93 seconds, including original-path checks; peak RSS about 118 MiB |
| Native smoke test | Passed: grid, loupe, first-to-last navigation, local rating and pick persistence; 38 decoded images |
| Native screenshots | Inspected grid, loupe, and bottom-row navigation |

Backup and hashing had already read the databases before timing, so these are warm observations. They do not measure a cold launch, sustained scrolling, GPU memory, decoding the entire catalog, or long-session stability. Video previews are still images; the app does not play videos. The catalog also contains 20 records with mirrored orientation codes not yet supported by this alpha.

Next, browse different years and formats, test filters and rapid navigation, and record unavailable previews or pauses. Full collection preview compatibility and sustained performance remain open validation tasks.
