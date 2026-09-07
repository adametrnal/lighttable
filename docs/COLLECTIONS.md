# Collection browsing

Implemented September 6, 2026 against the two supplied Classic catalogs (schema `1504001`).

## Using the sidebar

Expand collection sets with the triangle or set name. Click an ordinary collection's name to browse its photos. Ordinary collections with saved print children have a separate expansion triangle. Drag the sidebar edge to give long names more room; hovering shows the full name.

The count beside each ordinary collection is its catalog membership before metadata filters. Search, rating, flag and sorting controls apply within the chosen collection; the toolbar shows the filtered count and collection name. **Any flag** includes picks, rejects and unflagged photos. **Clear filters** restores all photographs. Selecting a folder or a Library shortcut leaves the collection scope.

Collections retain separate virtual-copy records and use the same local rating/flag overlays, selection, loupe and undo behavior as the rest of the app. Refresh reloads membership from a new private snapshot. Collection names, membership and ordering cannot be edited in Lighttable; photo order uses capture time or filename rather than Lightroom's custom collection order.

Supported date/rating Smart Collections appear with a **Smart** badge and calculated count. Other Smart rules stay disabled with an **Unavailable** badge and a reason on hover. See [Smart Collection support](SMART_COLLECTIONS.md) for the experimental semantics and validation. Saved print and unknown collection types appear as **Unsupported**, retaining their place in the hierarchy. Sets expand for navigation; selecting a set does not aggregate its descendants. System-only collections, including Quick Collection, are excluded. Published collections are outside this initial adapter.

## Catalog evidence and implementation

`AgLibraryCollection.creationId` distinguishes ordinary collections (`com.adobe.ag.library.collection`), sets (`com.adobe.ag.library.group`), Smart Collections (`com.adobe.ag.library.smart_collection`) and saved print collections (`com.adobe.ag.print`). `parent` provides the hierarchy, including print children of ordinary collections. These are observations of the supplied databases, not a published Adobe schema contract.

`AgLibraryCollectionImage.collection` links each membership to `Adobe_images.id_local` through its `image` column. Membership is resolved to photo positions once during background loading, sorted in capture order and deduplicated. File IDs and basenames are not used for matching. The membership table's `pick` value is not the photo's pick flag. Stored `imageCount` values are absent or stale in these catalogs, so displayed counts come from the resolved membership.

Smart rule text in `AgLibraryCollectionContent`, under `owningModule='ag.library.smart_collection'`, includes rating, color, keywords, file-format, capture-time and modification-time predicates, plus time units and combination modes. It is parsed as bounded data and is never executed as Lua. The supported date/rating subset is evaluated locally; a collection containing any unsupported branch stays unavailable.

The adapter makes no catalog writes. Optional collection-schema errors leave photo browsing available and display a warning. Invalid parent links are rooted with a warning; cycles are broken before iterative traversal. Collections referencing missing photos are disabled instead of exposing incomplete membership for batch editing.

The sidebar renders only visible collection/folder rows. Filtering an ordinary collection visits only its resolved members; it does not scan all photos, rebuild folder names or query SQLite per frame.

## Verification

The Rust fixtures cover nested and duplicate-named collections, empty membership, stale counts, duplicate membership rows, unsupported Smart/print collections, system-only collections, virtual-copy identity, photo flags versus membership flags, filter intersections, durable zero-value edits, source-byte preservation, missing tables, missing photos, and malformed parent cycles. Egui tests check expansion, independent selection, disabled entries, name truncation, bounded rendering with 405 collection nodes, retained scrolling after revealing a distant selection, and clamping after collapsing a branch.

The opt-in SQL comparison checks every visible parent link and the exact UUID membership/count of every ordinary collection against a separate read-only snapshot:

| Catalog | Photos | Ordinary collections | Sets | Smart | Other | Ordinary memberships | Virtual-copy memberships |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SmallLinuxTestCatalog | 32 | 2 | 6 | 26 | 0 | 35 | 0 |
| Vault working copy | 79,393 | 202 | 37 | 26 | 7 | 12,036 | 988 |

All 34 visible parent links in the small catalog and all 272 in Vault matched. Membership totals count the same photo again when it belongs to multiple collections.

The optimized Vault run loaded in 310 ms. Across 6,060 warm collection-filter queries, p95 was approximately 0.001 ms. This measures the membership/metadata filter only, excluding UI work, thumbnail decoding, cold storage and sustained scrolling.

Desktop smoke tests also passed on both catalogs, using isolated selection stores. Each checked a nested ordinary collection's membership and rating/pick save, then captured `collection.png`. The final screenshots in `/tmp/lighttable-collections-vault-verified-20260906` and `/tmp/lighttable-collections-small-verified-20260906` were visually inspected.

Reproduce with a closed local catalog copy:

```bash
bash scripts/check.sh
LIGHTTABLE_COLLECTION_CATALOG='/path/to/catalog.lrcat' \
  bash scripts/cargo-local test --release --test collections_catalog --locked -- --ignored --nocapture
bash scripts/cargo-local build --release --locked
target/release/lighttable '/path/to/catalog.lrcat' --smoke-test /tmp/lighttable-collections-smoke
```

**Still required:** compare the displayed hierarchy and a few representative membership/count examples inside Lightroom itself, including a virtual copy. SQL agreement establishes fidelity to the saved catalog; Lightroom-side visual validation and broader Smart rule support remain separate work.
