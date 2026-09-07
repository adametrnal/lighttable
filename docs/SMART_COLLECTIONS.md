# Smart Collections: date and rating support

Lighttable can now evaluate 15 of the 26 Smart Collections in each supplied catalog. This is experimental support for the saved rules, with a count and selectable photo results. Hover a collection to see its actual rule; the collection name does not determine its behavior.

## Supported rules and behavior

- Star ratings: equal, not equal, less/greater than, and inclusive comparisons, including zero stars.
- Capture date: exact dates, comparisons, inclusive date ranges, and “in the last” whole days or calendar months.
- Match all (`intersect`) and match any (`union`), including nested groups of supported rules.

Capture-date comparisons use the stored calendar date, without assigning a timezone to camera timestamps. An exact date covers that whole day; a range includes both endpoints. An inactive `value2` left over from an earlier range is ignored when the saved operation is equality. Missing or invalid capture dates do not match date predicates.

For relative dates, Lighttable uses the Linux system's local calendar date. “Last N days” includes dates from today minus N days through today; “last N months” subtracts calendar months and clamps to the last valid day when necessary. Future dates are excluded. Month-end, leap-day and exact cutoff behavior still need comparison against the user's Lightroom version; these definitions are Lighttable's current interpretation.

Counts are calculated before the toolbar's search/rating/flag filters. Results include locally changed ratings, so a photo can leave or enter a Smart Collection immediately when rated. Undo, redo, failed-save rollback and reopened selection overlays recalculate membership. Virtual copies remain distinct photo records. Relative collections also recalculate when the local date changes, with an idle wakeup at least once a minute.

## What remains unavailable

The remaining 11 Smart Collections in the supplied catalogs include two edit-date rules, four collections using color labels, and one each involving collection names, file type, filename, keywords and aperture. Saved Lightroom edit dates (`touchTime`) are distinct from capture dates and are not interpreted yet. Local ratings do not rewrite Lightroom's edit timestamp.

Unsupported collections stay disabled with an **Unavailable** badge. Hovering explains the unsupported criterion, invalid definition, or missing data. If any branch is unsupported, the entire Smart Collection is unavailable, including mixed OR groups; supported branches are never used as a partial approximation. Other combination modes, units, operators and rule fields remain unavailable unless explicitly implemented. No Smart Collection rule editing is provided.

Adobe describes Smart Collections as rule-based selections; its [collection guide](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/photo-collections.html#work-with-smart-collections) and [criteria reference](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/smart-collections-criteria-in-lightroom-classic.html) describe the Lightroom feature. The on-disk rule encoding here is based on the supplied schema `1504001`, rather than an Adobe catalog-format contract.

## Implementation and validation

`AgLibraryCollectionContent` supplies the saved `s = { ... }` data. The parser accepts a bounded subset of table/string/number syntax and never executes Lua. Unknown fields, duplicate keys/definitions, executable expressions, malformed values and oversized/deeply nested input are rejected. Limits are 64 KiB, 16 nesting levels and 512 parsed values.

Capture dates are parsed once during loading. Smart memberships are cached as photo positions and recomputed after relevant changes; rendering and toolbar filtering use those positions without reevaluating every rule on every frame. The source catalog and photo files remain unchanged.

Verification includes:

- Parser/evaluator tests for observed date/rating rules, all comparisons, zero stars, date endpoints, inactive range fields, nested all/any groups, leap years, month ends, missing dates and unsupported or executable input.
- Catalog/store tests for virtual-copy isolation, persisted zero ratings, restoring prior values, date rollover, unsupported OR branches, duplicate definitions and source-byte preservation.
- Egui tests for clicking a supported Smart Collection, disabled unsupported rows, tree expansion, clipping and retained scrolling.
- Exact photo UUID comparisons between all 15 supported collections and a separate SQL evaluation in both catalogs, using local date `2026-09-05` and a frozen historical date `2024-07-31`. The historical date also exercises nonempty relative-date results in Vault. SQL verifies metadata adaptation and the implemented semantics; it does not establish Lightroom equivalence.
- Native smoke tests in isolated selection stores: browse a Smart Collection, clear a qualifying photo to zero stars, verify it leaves, undo, verify the complete membership is restored, and capture `smart-collection.png`.

Recalculating all 15 Smart Collections over Vault's 79,393 photo records measured p50 **5.97 ms** and p95 **6.00 ms** in 30 release-build runs. This is metadata computation only, excluding decoding, UI work and storage. One full catalog load with this feature took 317 ms with warm storage.

Run the checks with a closed disposable catalog copy:

```bash
bash scripts/check.sh
LIGHTTABLE_COLLECTION_CATALOG='/path/to/catalog.lrcat' \
  bash scripts/cargo-local test --release --test collections_catalog --locked -- --ignored --nocapture
bash scripts/cargo-local build --release --locked
target/release/lighttable '/path/to/catalog.lrcat' --smoke-test /tmp/lighttable-smart-smoke
```

**Next validation:** compare representative exact-date, range and relative-date results inside Lightroom with matching ratings and local dates. The runtime labels Smart results experimental until that check is complete.
