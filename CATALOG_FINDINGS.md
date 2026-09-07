# Test Catalog Findings

Inspected September 5, 2026. These observations informed the implementation; they are not a specification for every Lightroom Classic catalog.

Source:

`/path/to/Test Catalog.lrcat`

The project directory had no existing application files or applicable `AGENTS.md`. `git status` reported that it was not a Git repository in this environment. No repository initialization or Rust installation was performed.

## Method and transfer state

Inspection used filesystem inventories, SQLite URI connections with `mode=ro` and query-only SQL, and small image-header reads. No catalog records, photo metadata, or image content were intentionally modified. SQLite read-only connections to a WAL-mode database can create coordination files; later inventories included `.lrcat-wal` and `.lrcat-shm`, so a future zero-write inspection workflow should use a verified private snapshot or a read-only mount.

The file tree changed during inspection: `.part` files appeared as originals and previews arrived. The first inventory contained 23 complete original DNGs and 15 JPEGs, while the later original-file inventory contained 31 DNGs and 16 JPEGs. All 32 referenced originals then resolved. Treat preview payload totals as provisional until transfer completion; do not treat the early missing-original list as persistent catalog damage.

## Database observations

| Item | Observed value |
| --- | --- |
| Catalog size | 2,699,264 bytes |
| SQLite journal mode | `wal` |
| SQLite `user_version` / `application_id` | Both `0`; insufficient for adapter selection |
| `Adobe_DBVersion` | `1504001` in `Adobe_variablesTable` |
| `Adobe_CrsVersion` | `18.0.0` |
| `CullingSchemaVersion` | `12.0` |
| `PRAGMA quick_check` | `ok` for catalog and standard-preview database at inspection |
| `Adobe_images` | 32 rows; 31 DNG and 1 JPG |
| Ratings and flags | Every row has `rating=3.0`, `pick=0.0` |
| Virtual copies | No non-NULL `masterImage` values |
| File/folder/root records | 32 files, 7 folders, 1 root |
| EXIF / IPTC | 32 rows in each inspected image metadata table |
| Keywords / keyword memberships | 0 / 0 |
| Collections | 2 ordinary collections, 6 groups, 26 smart collections, inferred from `creationId` |
| Collection-image associations | 35 rows; scope needs interpretation |

The stored database/version tokens are evidence for this fixture. The Lightroom marketing version and supported-version range still need confirmation. `quick_check=ok` establishes limited SQLite structural health at a point in time, not semantic completeness or a completed asset transfer.

Useful read relationships:

```text
Adobe_images.rootFile          -> AgLibraryFile.id_local
AgLibraryFile.folder           -> AgLibraryFolder.id_local
AgLibraryFolder.rootFolder     -> AgLibraryRootFolder.id_local
AgHarvestedExifMetadata.image  -> Adobe_images.id_local
AgLibraryIPTC.image            -> Adobe_images.id_local
AgLibraryKeywordImage.image    -> Adobe_images.id_local
AgLibraryKeywordImage.tag      -> AgLibraryKeyword.id_local
```

`Adobe_images` exposes `id_global`, `rating`, `pick`, `captureTime`, `fileWidth`, `fileHeight`, `orientation`, `masterImage`, `copyName`, `pyramidIDCache`, and `developSettingsIDCache`. `AgLibraryFile` has a separate `id_global`, filename fields, and `sidecarExtensions`. Preserve image identity independently of file identity.

The EXIF table refers to interned camera and lens tables. Sample aperture values were `2.970854`, with shutter values such as `7.321928`. These appear to use APEX encoding: candidate conversions are `f_number = 2^(aperture/2)` and `seconds = 2^(-shutterSpeed)`, approximately f/2.8 and 1/160 second for that pair. Confirm against the original's EXIF and Lightroom UI before implementing general conversion. Sample capture times are ISO-like local strings with fractional seconds and no timezone suffix. Orientation values were `AB` (31 rows) and `DA` (one); do not invent their mapping from this limited sample.

Both `Adobe_images` and `AgLibraryCollectionImage` contain a `pick` field. Determine their semantics with controlled Lightroom flag changes; column names alone do not justify synchronized writes.

## Paths and related files

The root's `absolutePath` is a UNC-style network path ending in `SmallLinuxTestCatalog/SmallLinuxTestCatalog/Lightroom/`. Its `relativePathFromCatalog` is `Lightroom/`. Joining that relative root, `AgLibraryFolder.pathFromRoot`, and the file basename/extension resolves all 32 originals in the later inventory without editing source paths.

Fifteen file records have `sidecarExtensions='JPG'`; seventeen have an empty value. The later original folder has 16 JPEG files even though only one independent JPG image is cataloged. This is consistent with 15 DNG/JPEG companion pairs. Validate associations before deletion and avoid treating a filesystem JPEG inventory as the catalog photo count.

## Preview evidence

The standard cache is `SmallLinuxTestCatalog Previews.lrdata/previews.db` (155,648 bytes). It has:

| Table | Rows | Useful columns |
| --- | --- | --- |
| `ImageCacheEntry` | 32 | `imageId`, `uuid`, `digest`, `orientation` |
| `Pyramid` | 32 | `uuid`, `digest`, `colorProfile`, cropped dimensions, timestamps, `fromProxy` |
| `PyramidLevel` | 110 | `uuid`, `digest`, `level`, `longDimension`, `width`, `height`, `fileSize` |

Sample payload paths follow a pattern such as:

```text
0/0A0C/0A0CD9F0-B95C-4DBA-A04E-8F21E2E2943B-5dbecdd6d30528e0028691e1759236a8_1775
```

Header identification reports JPEG; sampled sizes were 1775×1184, 887×591, and 320×213. At the last detailed preview inventory there were 97 extensionless files and one partial payload. This establishes an extensionless JPEG layout for sampled files, not a universal `.lrprev` container layout. Verify the relationship of `imageId`, `pyramidIDCache`, UUID, digest, and level against every fixture record during the first implementation spike.

The Smart Preview folder contains 32 DNG payloads in UUID-based paths such as `3/310E/<uuid>.dng`. Two inspected headers identify TIFF/DNG with a 256×171 first image directory. A first-directory thumbnail does not establish full Smart Preview resolution or the ability to reproduce Lightroom edits; inspect the subimages and catalog proxy mapping separately.

One original DNG (`D95A7060.dng`) has a 256×171 uncompressed thumbnail, a raw image directory, and a 1024×683 reduced image with JPEG compression code `7`. Other reduced directories use compression code `52546`, whose decoder requirements were not validated. The image pipeline should prefer a supported embedded preview and explicitly detect unsupported compression. Header inspection did not benchmark full decoding.

An auxiliary `.lrcat-data` directory is also present with manifest/log/blob files. No edit-data decoding was attempted; preserve this directory with the Lightroom package.

## Evidence still needed

Before broad compatibility claims, obtain a stable fixture copy, confirm the complete preview inventory, and test all joins and payload mappings. Add controlled examples with zero and five stars, pick/reject flags, virtual copies, keywords, edited/cropped images, multiple roots, and absent previews. Verify exact UUID matching, supported metadata writes, and catalog-removal capabilities in the Lightroom SDK against an actual Lightroom session. The 80,000+ catalog has not been inspected or benchmarked.
