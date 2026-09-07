import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import sqlite3
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("sync_lab", Path(__file__).resolve().parents[1] / "scripts/catalog_sync_lab.py")
lab = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lab)

UUIDS = [f"{number:08x}-1111-2222-3333-444444444444" for number in range(1, 4)]
FILE_UUID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"


class SyncLabTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="lighttable-sync-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root / "source.lrcat"
        with contextlib.closing(sqlite3.connect(self.source)) as db:
            db.executescript("""
                CREATE TABLE Adobe_variablesTable(name, value);
                INSERT INTO Adobe_variablesTable VALUES('Adobe_DBVersion','1504001');
                CREATE TABLE AgLibraryFile(id_local INTEGER PRIMARY KEY,id_global,baseName,extension);
                CREATE TABLE Adobe_images(id_local INTEGER PRIMARY KEY,id_global UNIQUE,rootFile,masterImage,copyName,rating,pick,touchCount,touchTime);
                CREATE TABLE Adobe_imageDevelopSettings(id_local INTEGER PRIMARY KEY,image,text);
                CREATE TABLE AgLibraryFolder(id_local INTEGER PRIMARY KEY,absolutePath);
                CREATE TABLE Adobe_AdditionalMetadata(id_local INTEGER PRIMARY KEY,image,externalXmpIsDirty,xmp);
                CREATE TABLE AgLibraryCollectionImage(id_local INTEGER PRIMARY KEY,image,pick);
                CREATE TABLE BinaryPayload(id INTEGER PRIMARY KEY,data);
                INSERT INTO BinaryPayload VALUES(1,CAST(X'FF001122' AS TEXT));
                INSERT INTO BinaryPayload VALUES(2,X'FF001122');
                INSERT INTO AgLibraryFolder VALUES(1,'D:/Photos/');
                INSERT INTO Adobe_imageDevelopSettings VALUES(1,1,'exposure=-0.35;crop=original');
                INSERT INTO Adobe_AdditionalMetadata VALUES(1,1,0,X'001122FF');
                INSERT INTO AgLibraryCollectionImage VALUES(1,1,0.0);
            """)
            db.execute("INSERT INTO AgLibraryFile VALUES(1,?,'photo','DNG')", (FILE_UUID,))
            for number, uuid in enumerate(UUIDS, 1):
                db.execute("INSERT INTO Adobe_images VALUES(?,?,1,?,?,?,?,7.0,12345.5)",
                           (number, uuid, 1 if number == 2 else None, "Copy 1" if number == 2 else "",
                            {1: 3.0, 2: 4.0, 3: None}[number], {1: 1.0, 2: -1.0, 3: 0.0}[number]))
            db.commit()
        aux = self.source.with_suffix(".lrcat-data")
        aux.mkdir()
        (aux / "edit-blob").write_bytes(b"Original AI/develop auxiliary data\x00")
        self.changes = self.root / "changes.tsv"

    def export(self, rows=None):
        rows = rows or [(0, 3, 0, 1, 0), (1, 4, 5, -1, 1), (2, 0, 3, 0, None)]
        lines = ["LIGHTTABLE\t1\t" + "a" * 64, lab.COLUMNS]
        for number, base_rating, rating, base_pick, pick in rows:
            fields = [UUIDS[number], FILE_UUID, "photo.DNG".encode().hex(), str(int(number == 1)),
                      str(base_rating), "-" if rating is None else str(rating),
                      str(base_pick), "-" if pick is None else str(pick)]
            lines.append("\t".join(fields))
        self.changes.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def prepare(self, name="output"):
        with contextlib.redirect_stdout(io.StringIO()):
            return lab.prepare(self.source, self.changes, self.root / name)

    def execute(self, sql):
        with contextlib.closing(sqlite3.connect(self.source)) as db:
            db.executescript(sql)

    def test_copy_only_preservation_zero_values_virtual_copy_and_repeat(self):
        self.export()
        before = self.source.read_bytes()
        report = self.prepare()
        self.assertEqual(self.source.read_bytes(), before)
        self.assertEqual(report["counts"], dict(apply=3, already=0, skipped=0))
        self.assertEqual(report["catalog_sha256"]["baseline"], report["catalog_sha256"]["reference"])
        candidate = self.root / "output/candidate" / lab.CATALOG_NAME
        with contextlib.closing(sqlite3.connect(candidate)) as db:
            self.assertEqual(db.execute("SELECT rating,pick,touchCount,touchTime FROM Adobe_images ORDER BY id_local").fetchall(),
                             [(None, 0.0, 7.0, 12345.5), (5.0, 1.0, 7.0, 12345.5), (3.0, 0.0, 7.0, 12345.5)])
            self.assertTrue(all(p["status"] == "already" for p in lab.plan_changes(db, lab.parse_changes(self.changes)[1])))
        for variant in ("baseline", "reference", "candidate"):
            self.assertEqual((self.root / "output" / variant / "Lighttable Sync Test.lrcat-data/edit-blob").read_bytes(),
                             b"Original AI/develop auxiliary data\x00")
        self.assertFalse((self.root / "output/INCOMPLETE.txt").exists())

    def test_conflict_skips_whole_photo_and_unrelated_field_is_preserved(self):
        self.export([(0, 3, 5, 1, 0), (1, 4, 5, -1, None)])
        self.execute("UPDATE Adobe_images SET rating=2.0 WHERE id_local=1; UPDATE Adobe_images SET pick=0.0 WHERE id_local=2;")
        report = self.prepare()
        self.assertEqual(report["counts"], dict(apply=1, skipped=1, already=0))
        self.assertEqual(report["plans"][0]["reason"], "Conflict in rating")
        with contextlib.closing(sqlite3.connect(self.root / "output/candidate" / lab.CATALOG_NAME)) as db:
            self.assertEqual(db.execute("SELECT rating,pick FROM Adobe_images WHERE id_local=1").fetchone(), (2.0, 1.0))
            self.assertEqual(db.execute("SELECT rating,pick FROM Adobe_images WHERE id_local=2").fetchone(), (5.0, 0.0))

    def test_identity_mismatch_and_missing_photo(self):
        self.export()
        self.execute("UPDATE AgLibraryFile SET id_global='ffffffff-bbbb-cccc-dddd-eeeeeeeeeeee'; DELETE FROM Adobe_images WHERE id_local=3;")
        report = self.prepare()
        self.assertEqual(report["counts"], dict(apply=0, skipped=3, already=0))

    def test_malformed_exports_and_duplicate_identity_are_rejected(self):
        self.export()
        original = self.changes.read_text()
        for value in (original + original.splitlines()[2] + "\n",
                      original.replace("\t3\t0\t1\t0", "\t3\t6\t1\t0"),
                      original.replace("70686f746f2e444e47", "2e2e2f70686f746f"),
                      original.replace("\t0\t3\t0\t1\t0", "\t2\t3\t0\t1\t0")):
            self.changes.write_text(value)
            with self.assertRaises(ValueError):
                lab.parse_changes(self.changes)

    def test_locks_journals_unknown_schema_and_existing_outputs_are_refused(self):
        self.export()
        for suffix in (".lock", "-journal"):
            lock = Path(str(self.source) + suffix)
            lock.touch()
            with self.assertRaises(ValueError):
                self.prepare()
            lock.unlink()
        output = self.root / "existing"
        output.mkdir()
        (output / "keep").write_text("keep me")
        with self.assertRaises(FileExistsError):
            self.prepare("existing")
        self.assertEqual((output / "keep").read_text(), "keep me")
        self.execute("UPDATE Adobe_variablesTable SET value='9999999';")
        with self.assertRaisesRegex(ValueError, "restricted"):
            self.prepare()
        self.assertFalse((self.root / "output/candidate" / lab.CATALOG_NAME).exists())

    def test_wal_is_recovered_only_on_copy(self):
        with contextlib.closing(sqlite3.connect(self.source)) as db:
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("UPDATE Adobe_images SET rating=5.0 WHERE id_local=1")
            db.commit()
            main = self.source.read_bytes()
            wal_path = Path(str(self.source) + "-wal")
            wal = wal_path.read_bytes()
            snapshot = self.root / "snapshot.sqlite"
            receipt = lab.snapshot(self.source, snapshot)
            self.assertEqual(self.source.read_bytes(), main)
            self.assertEqual(wal_path.read_bytes(), wal)
            lab.verify_source(self.source.resolve(), receipt)
            with contextlib.closing(sqlite3.connect(snapshot)) as copy:
                self.assertEqual(copy.execute("SELECT rating FROM Adobe_images WHERE id_local=1").fetchone()[0], 5.0)
                self.assertEqual(copy.execute("PRAGMA journal_mode").fetchone()[0], "delete")

    def test_trigger_side_effects_fail_preservation_and_wrong_values_fail_verification(self):
        self.export()
        self.execute("CREATE TRIGGER side_effect AFTER UPDATE OF rating ON Adobe_images BEGIN UPDATE Adobe_AdditionalMetadata SET externalXmpIsDirty=1; END;")
        with self.assertRaisesRegex(ValueError, "Unapproved"):
            self.prepare("side-effect")
        self.assertFalse((self.root / "side-effect/candidate" / lab.CATALOG_NAME).exists())
        self.execute("DROP TRIGGER side_effect; CREATE TRIGGER wrong_value AFTER UPDATE OF rating ON Adobe_images BEGIN UPDATE Adobe_images SET rating=1.0 WHERE id_local=NEW.id_local; END;")
        with self.assertRaisesRegex(ValueError, "expected values"):
            self.prepare("wrong-value")

    def test_failed_transaction_rolls_back_candidate_and_keeps_source(self):
        self.export()
        self.execute("CREATE TRIGGER fail_second BEFORE UPDATE ON Adobe_images WHEN OLD.id_local=2 BEGIN SELECT RAISE(ABORT,'injected failure'); END;")
        before = self.source.read_bytes()
        with self.assertRaises(sqlite3.IntegrityError):
            self.prepare()
        self.assertEqual(self.source.read_bytes(), before)
        with contextlib.closing(sqlite3.connect(self.root / "output/candidate/building.sqlite")) as db:
            self.assertEqual(db.execute("SELECT rating FROM Adobe_images WHERE id_local=1").fetchone()[0], 3.0)

    def test_compare_exposes_bookkeeping_and_does_not_modify_sources(self):
        self.export()
        after = self.root / "after.lrcat"
        shutil.copyfile(self.source, after)
        shutil.copytree(self.source.with_suffix(".lrcat-data"), after.with_suffix(".lrcat-data"))
        (after.with_suffix(".lrcat-data") / "edit-blob").write_bytes(b"changed test payload")
        with contextlib.closing(sqlite3.connect(after)) as db:
            db.execute("UPDATE Adobe_images SET touchCount=8.0,touchTime=23456.5,rating=5.0 WHERE id_local=1")
            db.execute("UPDATE Adobe_AdditionalMetadata SET externalXmpIsDirty=1")
            db.commit()
        hashes = [lab.digest_file(p) for p in (self.source, after)]
        with contextlib.redirect_stdout(io.StringIO()):
            report = lab.compare(self.source, after, self.changes, self.root / "comparison.json")
        self.assertEqual(set(report["changed_tables"]), {"Adobe_images", "Adobe_AdditionalMetadata"})
        self.assertEqual(set(report["auxiliary_changes"]), {"edit-blob"})
        self.assertEqual(report["photos"][0]["after"]["Adobe_images"][0]["touchCount"], 8.0)
        self.assertEqual([lab.digest_file(p) for p in (self.source, after)], hashes)


if __name__ == "__main__":
    unittest.main()
