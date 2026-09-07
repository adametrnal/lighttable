use lighttable::{
    catalog,
    collections::Kind,
    images,
    model::{Change, Filter},
    store::{self, Store},
};
use rusqlite::Connection;
use std::{collections::HashMap, fs, path::Path};

fn fixture(path: &Path) {
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE Adobe_variablesTable(name,value);
        INSERT INTO Adobe_variablesTable VALUES('Adobe_DBVersion','1504001');
        CREATE TABLE Adobe_images(id_local INTEGER,id_global,rootFile,fileFormat,captureTime,fileWidth,fileHeight,copyName,masterImage,orientation,rating,pick);
        CREATE TABLE AgLibraryFile(id_local INTEGER,id_global,baseName,extension,folder,sidecarExtensions);
        CREATE TABLE AgLibraryFolder(id_local INTEGER,pathFromRoot,rootFolder);
        CREATE TABLE AgLibraryRootFolder(id_local INTEGER,absolutePath,relativePathFromCatalog);
        CREATE TABLE AgHarvestedExifMetadata(image,aperture,shutterSpeed,isoSpeedRating,focalLength,cameraModelRef,lensRef);
        CREATE TABLE AgInternedExifCameraModel(id_local,value);
        CREATE TABLE AgInternedExifLens(id_local,value);
        CREATE TABLE AgLibraryIPTC(image,caption);
        INSERT INTO AgLibraryRootFolder VALUES(1,'//server/share/photos/','photos/');
        INSERT INTO AgLibraryFolder VALUES(1,'2026/',1);
        INSERT INTO AgLibraryFile VALUES(1,'12345678-1234-1234-1234-123456789ABC','first','dng',1,'JPG');
        INSERT INTO Adobe_images VALUES(1,'AAAAAAAA-1234-1234-1234-123456789ABC',1,'DNG','2026-01-01T12:34:56.123',6000,4000,NULL,NULL,'AB',NULL,0);
        INSERT INTO Adobe_images VALUES(2,'BBBBBBBB-1234-1234-1234-123456789ABC',1,'DNG','2026-01-01T12:34:56.123',6000,4000,'Copy 1',1,'DA',5,-1);
        INSERT INTO AgHarvestedExifMetadata VALUES(1,3.0,7.0,400.0,50.0,1,1);
        INSERT INTO AgInternedExifCameraModel VALUES(1,'Test Camera');
        INSERT INTO AgInternedExifLens VALUES(1,'50mm');
        INSERT INTO AgLibraryIPTC VALUES(1,'Test caption');").unwrap();
}

fn collection_fixture(path: &Path) {
    fixture(path);
    Connection::open(path).unwrap().execute_batch("
        CREATE TABLE AgLibraryCollection(id_local INTEGER, name, creationId, parent, systemOnly, imageCount);
        CREATE TABLE AgLibraryCollectionImage(collection, image, pick);
        INSERT INTO AgLibraryCollection VALUES
          (10,'Trips','com.adobe.ag.library.group',NULL,0,-1),
          (11,'2026','com.adobe.ag.library.group',10,0,NULL),
          (12,'Keepers','com.adobe.ag.library.collection',11,0,999),
          (13,'Keepers','com.adobe.ag.library.collection',NULL,0,99),
          (14,'Empty','com.adobe.ag.library.collection',11,0,3),
          (15,'Five stars','com.adobe.ag.library.smart_collection',10,0,999),
          (16,'Saved print','com.adobe.ag.print',12,0,999),
          (17,'quick collection','com.adobe.ag.library.collection',NULL,1.0,0),
          (18,'Another empty','com.adobe.ag.library.collection',NULL,'',0);
        INSERT INTO AgLibraryCollectionImage VALUES
          (12,2,1), (12,2,1), (13,1,-1), (13,2,1), (15,1,0), (16,1,0), (17,1,0);
    ").unwrap();
}

fn smart_fixture(path: &Path) {
    collection_fixture(path);
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE AgLibraryCollectionContent(collection,content,owningModule);
        UPDATE Adobe_images SET captureTime='2026-01-02T23:59:59.999' WHERE id_local=2;
        INSERT INTO Adobe_images VALUES(3,'CCCCCCCC-1234-1234-1234-123456789ABC',1,'DNG','2026-01-03T00:00:00.000',6000,4000,'Copy 2',1,'AB',3,1);
        INSERT INTO AgLibraryCollection VALUES
          (19,'Dates and stars','com.adobe.ag.library.smart_collection',11,0,999),
          (20,'Unsupported OR','com.adobe.ag.library.smart_collection',11,0,999),
          (21,'Ambiguous','com.adobe.ag.library.smart_collection',11,0,999),
          (22,'Last day','com.adobe.ag.library.smart_collection',11,0,999);").unwrap();
    for (id, rule) in [
        (
            15,
            "s={{criteria='rating',operation='>=',value=5},combine='intersect'}",
        ),
        (
            19,
            "s={{criteria='rating',operation='>=',value=3},{criteria='captureTime',operation='in',value='2026-01-01',value2='2026-01-02'},combine='intersect'}",
        ),
        (
            20,
            "s={{criteria='rating',operation='>=',value=0},{criteria='labelColor',operation='==',value=3},combine='union'}",
        ),
        (
            21,
            "s={{criteria='rating',operation='>=',value=0},combine='intersect'}",
        ),
        (
            21,
            "s={{criteria='rating',operation='>=',value=5},combine='intersect'}",
        ),
        (
            22,
            "s={{criteria='captureTime',operation='inLast',value=1,value_units='days'},combine='intersect'}",
        ),
    ] {
        db.execute(
            "INSERT INTO AgLibraryCollectionContent VALUES(?1,?2,'ag.library.smart_collection')",
            rusqlite::params![id, rule],
        )
        .unwrap();
    }
}

#[test]
fn smart_membership_reacts_to_persisted_ratings_undo_and_calendar_rollover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("smart.lrcat");
    smart_fixture(&path);
    let original = fs::read(&path).unwrap();
    let mut c = catalog::load(&path, None).unwrap();
    let today = lighttable::smart::date("2026-01-03").unwrap();
    c.collections.refresh_smart(&c.photos, today);
    let mut filter = Filter {
        collection: Some(15),
        ..Default::default()
    };
    assert_eq!(c.filtered(&filter), [1]);
    assert_eq!(c.collections.get(15).unwrap().members, [1]);
    assert_eq!(c.collections.get(19).unwrap().members, [1]); // final millisecond of range end
    assert_eq!(c.collections.get(22).unwrap().members, [1, 2]);
    for id in [20, 21] {
        assert!(!c.collections.get(id).unwrap().can_browse());
        assert!(c.collections.get(id).unwrap().members.is_empty());
        filter.collection = Some(id);
        assert!(c.filtered(&filter).is_empty());
    }
    filter.collection = Some(15);
    filter.pick = Some(1);
    assert!(c.filtered(&filter).is_empty());
    filter.pick = Some(-1);
    assert_eq!(c.filtered(&filter), [1]);
    filter.query = "absent".into();
    assert!(c.filtered(&filter).is_empty());
    filter.query.clear();
    let before = Change::for_photo(&c.photos[1]);
    let mut change = before.clone();
    change.rating = Some(0);
    change.apply(&mut c.photos[1]);
    c.collections.refresh_smart(&c.photos, today);
    assert!(c.filtered(&filter).is_empty());
    assert!(c.collections.get(19).unwrap().members.is_empty());
    assert_eq!(c.collections.get(22).unwrap().members, [1, 2]); // date-only rule unaffected
    assert_eq!(c.photos[2].rating, 3); // another virtual copy of the same original
    let state = dir.path().join("state");
    Store::open(&state)
        .unwrap()
        .save(&c.key, &[change])
        .unwrap();
    let mut reloaded = catalog::load(&path, None).unwrap();
    let changes = Store::open(&state).unwrap().changes(&c.key).unwrap();
    changes[&reloaded.photos[1].uuid].apply(&mut reloaded.photos[1]);
    reloaded.collections.refresh_smart(&reloaded.photos, today);
    assert!(reloaded.filtered(&filter).is_empty());
    before.apply(&mut reloaded.photos[1]);
    reloaded.collections.refresh_smart(&reloaded.photos, today);
    assert_eq!(reloaded.filtered(&filter), [1]);
    reloaded
        .collections
        .refresh_smart(&reloaded.photos, today.succ_opt().unwrap());
    assert_eq!(reloaded.collections.get(22).unwrap().members, [2]);
    assert_eq!(original, fs::read(&path).unwrap());
}

#[test]
fn collections_preserve_hierarchy_membership_identity_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collections.lrcat");
    collection_fixture(&path);
    let before = fs::read(&path).unwrap();
    let c = catalog::load(&path, None).unwrap();
    assert_eq!(before, fs::read(&path).unwrap());
    let tree = &c.collections;
    assert_eq!(tree.nodes.len(), 8); // system-only Quick Collection is hidden
    assert!(tree.get(17).is_none());
    let collection = tree.get(12).unwrap();
    let parent = &tree.nodes[collection.parent.unwrap()];
    assert_eq!(parent.id, 11);
    assert_eq!(tree.nodes[parent.parent.unwrap()].id, 10);
    assert_eq!(collection.members, [1]); // duplicate rows collapse, master does not leak in
    assert_eq!(tree.get(13).unwrap().members, [0, 1]);
    assert!(tree.get(14).unwrap().members.is_empty()); // cached imageCount is ignored
    assert_eq!(tree.get(15).unwrap().kind, Kind::Smart);
    assert!(tree.get(15).unwrap().members.is_empty()); // stale Smart results are not shown
    assert_eq!(tree.get(16).unwrap().kind, Kind::Unsupported);
    let collapsed = tree.visible_rows(&Default::default());
    assert_eq!(collapsed.len(), 3);
    assert_eq!(tree.nodes[tree.get(16).unwrap().parent.unwrap()].id, 12);
    let expanded = tree.visible_rows(&[10, 11, 12].into_iter().collect());
    assert_eq!(expanded.len(), 8);
    assert!(
        expanded
            .iter()
            .any(|&(i, depth)| tree.nodes[i].id == 12 && depth == 2)
    );
    assert_eq!((c.photos[1].rating, c.photos[1].pick), (5, -1)); // membership pick is unrelated
}

#[test]
fn collection_filters_intersect_metadata_and_keep_virtual_copy_edits_separate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collections.lrcat");
    collection_fixture(&path);
    let mut c = catalog::load(&path, None).unwrap();
    let mut filter = Filter {
        collection: Some(12),
        ..Default::default()
    };
    assert_eq!(c.filtered(&filter), [1]);
    filter.minimum_rating = 5;
    filter.pick = Some(-1);
    filter.query = "FIRST".into();
    filter.sort_name = true;
    assert_eq!(c.filtered(&filter), [1]);
    filter.query = "missing".into();
    assert!(c.filtered(&filter).is_empty());
    filter.query.clear();
    filter.folder = "elsewhere".into();
    assert!(c.filtered(&filter).is_empty());
    filter.folder.clear();
    filter.format = "JPEG".into();
    assert!(c.filtered(&filter).is_empty());
    filter.format.clear();
    let mut change = Change::for_photo(&c.photos[1]);
    change.rating = Some(0);
    change.pick = Some(0);
    let mut store = Store::open(&dir.path().join("state")).unwrap();
    store.save(&c.key, &[change.clone()]).unwrap();
    change.apply(&mut c.photos[1]);
    assert!(c.filtered(&filter).is_empty());
    let saved = store.changes(&c.key).unwrap();
    assert_eq!(saved.len(), 1);
    assert!(saved[&c.photos[1].uuid].virtual_copy);
    assert_eq!((c.photos[0].rating, c.photos[0].pick), (0, 0));
    filter.minimum_rating = 0;
    filter.pick = Some(0);
    assert_eq!(c.filtered(&filter), [1]);
    for id in [10, 14, 15, 16, 17, 999] {
        filter.collection = Some(id);
        assert!(c.filtered(&filter).is_empty());
    }
    assert_eq!(c.filtered(&Filter::default()), [0, 1]);
    // Reloaded membership is independent of overlays; the stored zero values still apply.
    let mut reloaded = catalog::load(&path, None).unwrap();
    saved[&reloaded.photos[1].uuid].apply(&mut reloaded.photos[1]);
    filter.collection = Some(12);
    assert_eq!(reloaded.filtered(&filter), [1]);
}

#[test]
fn malformed_or_missing_collection_data_keeps_photo_browsing_available() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collections.lrcat");
    collection_fixture(&path);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "
        UPDATE AgLibraryCollection SET parent=11 WHERE id_local=10;
        UPDATE AgLibraryCollection SET parent=999 WHERE id_local=13;
        INSERT INTO AgLibraryCollectionImage VALUES(12,999,0);
    ",
        )
        .unwrap();
    let c = catalog::load(&path, None).unwrap();
    assert_eq!(c.collections.get(12).unwrap().kind, Kind::Unsupported);
    assert!(c.collections.get(13).unwrap().parent.is_none());
    assert!(c.warnings.iter().any(|w| w.contains("cycle")));
    assert!(c.warnings.iter().any(|w| w.contains("missing photos")));
    let expanded = c.collections.nodes.iter().map(|n| n.id).collect();
    assert_eq!(
        c.collections.visible_rows(&expanded).len(),
        c.collections.nodes.len()
    );
    Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE AgLibraryCollectionImage")
        .unwrap();
    let c = catalog::load(&path, None).unwrap();
    assert!(c.collections.nodes.is_empty());
    assert!(
        c.warnings
            .iter()
            .any(|w| w.contains("Collections unavailable"))
    );
    assert_eq!(c.filtered(&Filter::default()).len(), 2);
}

#[test]
fn snapshot_preserves_source_bytes_and_maps_metadata_and_virtual_copies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.lrcat");
    fixture(&path);
    let before = fs::read(&path).unwrap();
    let catalog = catalog::load(&path, None).unwrap();
    assert_eq!(before, fs::read(&path).unwrap());
    assert!(!dir.path().join("sample.lrcat-wal").exists());
    assert_eq!(catalog.photos.len(), 2);
    let p = &catalog.photos[0];
    assert_eq!(p.rating, 0);
    assert_eq!(p.camera, "Test Camera");
    assert_eq!(p.original, dir.path().join("photos/2026/first.dng"));
    assert!((p.aperture.unwrap() - 2.828427).abs() < 0.00001);
    assert_eq!(p.shutter, Some(1.0 / 128.0));
    assert_eq!(p.companions, vec![dir.path().join("photos/2026/first.JPG")]);
    assert!(catalog.photos[1].virtual_copy);
    assert_ne!(p.uuid, catalog.photos[1].uuid);
    assert_eq!(p.file_uuid, catalog.photos[1].file_uuid);
}

#[test]
fn wal_snapshot_includes_committed_rows_without_touching_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.lrcat");
    fixture(&path);
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; UPDATE Adobe_images SET rating=4 WHERE id_local=1;")
        .unwrap();
    let before = fs::read(&path).unwrap();
    let wal = fs::read(dir.path().join("sample.lrcat-wal")).unwrap();
    let catalog = catalog::load(&path, None).unwrap();
    assert_eq!(catalog.photos[0].rating, 4);
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(wal, fs::read(dir.path().join("sample.lrcat-wal")).unwrap());
}

#[test]
fn lock_and_unsupported_schema_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.lrcat");
    fixture(&path);
    fs::write(dir.path().join("sample.lrcat.lock"), b"busy").unwrap();
    assert!(
        catalog::load(&path, None)
            .err()
            .unwrap()
            .to_string()
            .contains("lock")
    );
    let other = dir.path().join("other.lrcat");
    fs::write(&other, b"not SQLite").unwrap();
    assert!(catalog::load(&other, None).is_err());
}

fn change() -> Change {
    Change {
        uuid: "AAAAAAAA-1234-1234-1234-123456789ABC".into(),
        file_uuid: "12345678-1234-1234-1234-123456789ABC".into(),
        filename: "a.dng".into(),
        virtual_copy: false,
        base_rating: 3,
        base_pick: 1,
        rating: Some(0),
        pick: Some(0),
    }
}

#[test]
fn clearing_values_survives_restart_and_catalog_namespaces_are_separate() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut store = Store::open(dir.path()).unwrap();
        store.save("catalog-a", &[change()]).unwrap();
    }
    let mut store = Store::open(dir.path()).unwrap();
    let changes = store.changes("catalog-a").unwrap();
    assert_eq!(changes[&change().uuid].rating, Some(0));
    assert_eq!(changes[&change().uuid].pick, Some(0));
    assert!(store.changes("catalog-b").unwrap().is_empty());
    let mut undone = change();
    undone.rating = None;
    undone.pick = None;
    store.save("catalog-a", &[undone]).unwrap();
    assert!(store.changes("catalog-a").unwrap().is_empty());
}

#[test]
fn batch_save_rolls_back_if_any_change_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let mut invalid = change();
    invalid.uuid = "other".into();
    invalid.rating = Some(9);
    assert!(store.save("catalog", &[change(), invalid]).is_err());
    assert!(store.changes("catalog").unwrap().is_empty());
}

#[test]
fn export_preserves_zero_and_never_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("changes.tsv");
    let map = HashMap::from([(change().uuid.clone(), change())]);
    assert_eq!(store::export(&path, "abcdef", &map).unwrap(), 1);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\t3\t0\t1\t0\n"));
    assert!(store::export(&path, "abcdef", &map).is_err());
    assert_eq!(text, fs::read_to_string(path).unwrap());
}

#[test]
fn malformed_dng_is_bounded_and_unknown_orientation_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.dng");
    fs::write(&path, b"II*\0\xff\xff\xff\xff").unwrap();
    assert!(images::embedded_jpeg(&path).is_err());
    assert!(images::lr_orientation("??").is_err());
    assert_eq!(
        images::lr_orientation("DA").unwrap(),
        image::metadata::Orientation::Rotate270
    );
}
