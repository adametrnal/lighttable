#!/usr/bin/env python3
"""Experimental Lightroom sync on NEW catalog copies only; never updates a source.

prepare creates baseline/reference/candidate catalogs and an audit report.
compare examines closed before/after catalogs, including Lightroom bookkeeping.
This is a column-only experiment, not a production catalog writer.
"""

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import struct
import tempfile

VERSION = "1504001"
CATALOG_NAME = "Lighttable Sync Test.lrcat"
COLUMNS = "uuid\tfile_uuid\tfilename_hex\tvirtual_copy\tbase_rating\trating\tbase_pick\tpick"
UUID = re.compile(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\Z")


class BinaryText(bytes):
    """Lightroom stores some non-UTF-8 payloads with SQLite's TEXT storage type."""


def decode_text(raw):
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return BinaryText(raw)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def qi(name):
    return '"' + name.replace('"', '""') + '"'


def digest_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def signature(path):
    try:
        stat = path.stat()
        require(path.is_file(), f"Expected a regular file: {path}")
        return [stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns]
    except FileNotFoundError:
        return None


def catalog_inputs(source):
    require(source.suffix.lower() == ".lrcat", "Choose a .lrcat catalog")
    require(source.is_file(), f"Catalog does not exist: {source}")
    for suffix in (".lock", "-journal"):
        require(not Path(str(source) + suffix).exists(),
                f"Close Lightroom and finish recovery/copying first; present: {source}{suffix}")
    return {str(path): signature(path) for path in (source, Path(str(source) + "-wal"))}


def snapshot(source, destination):
    """Recover committed WAL only on a private byte copy, then emit a standalone DB."""
    source = source.resolve(strict=True)
    before = catalog_inputs(source)
    with tempfile.TemporaryDirectory(prefix="lighttable-snapshot-", dir=destination.parent) as tmp:
        local = Path(tmp) / "source.sqlite"
        hashes = {}
        for path, sig in before.items():
            if sig is not None:
                target = local if path == str(source) else Path(str(local) + "-wal")
                shutil.copyfile(path, target)
                hashes[path] = digest_file(target)
                require(hashes[path] == digest_file(Path(path)), "Source changed during snapshot")
        require(before == catalog_inputs(source), "Source changed during snapshot")
        with contextlib.closing(sqlite3.connect(local)) as db:
            db.execute("PRAGMA trusted_schema=OFF")
            db.execute("PRAGMA query_only=ON")
            with contextlib.closing(sqlite3.connect(destination)) as standalone:
                db.backup(standalone)
                standalone.execute("PRAGMA journal_mode=DELETE")
                require(standalone.execute("PRAGMA integrity_check").fetchall() == [("ok",)],
                        "Catalog failed SQLite integrity_check")
        require(before == catalog_inputs(source), "Source changed during snapshot")
    return {"signatures": before, "sha256": hashes}


def verify_source(source, receipt):
    require(receipt["signatures"] == catalog_inputs(source), "Source changed during experiment")
    for name, expected in receipt["sha256"].items():
        require(digest_file(Path(name)) == expected, "Source bytes changed during experiment")


def parse_changes(path):
    with path.open("rb") as file:
        raw = file.read(32 * 1024 * 1024 + 1)
    require(len(raw) <= 32 * 1024 * 1024, "Changes file exceeds 32 MiB")
    lines = [line for line in raw.decode("utf-8").splitlines() if line]
    require(len(lines) >= 2 and re.fullmatch(r"LIGHTTABLE\t1\t[0-9a-fA-F]+", lines[0]),
            "Expected a Lighttable version 1 TSV export")
    require(lines[1] == COLUMNS, "Unexpected changes-file columns")
    result, seen = [], set()
    for line in lines[2:]:
        fields = line.split("\t")
        require(len(fields) == 8, "Expected eight fields per change")
        uuid, file_uuid, encoded, virtual, *numbers = fields
        require(UUID.fullmatch(uuid) and UUID.fullmatch(file_uuid), "Invalid photo/file UUID")
        require(uuid.lower() not in seen, "Duplicate photo UUID")
        seen.add(uuid.lower())
        require(0 < len(encoded) <= 4096 and len(encoded) % 2 == 0
                and re.fullmatch(r"[0-9a-fA-F]+", encoded), "Invalid filename encoding")
        filename = bytes.fromhex(encoded).decode("utf-8")
        require(not any(char in filename for char in "\x00/\\"), "Filename must be a basename")
        require(virtual in ("0", "1"), "Invalid virtual-copy marker")
        entry = dict(uuid=uuid, file_uuid=file_uuid, filename=filename, virtual_copy=virtual == "1")
        for key, text, low, high, optional in zip(
                ("base_rating", "rating", "base_pick", "pick"), numbers,
                (0, 0, -1, -1), (5, 5, 1, 1), (False, True, False, True)):
            if optional and text == "-":
                value = None
            else:
                require(re.fullmatch(r"-?\d+", text, flags=re.ASCII), "Invalid numeric field")
                value = int(text)
                require(low <= value <= high, "Rating or flag outside allowed range")
            entry[key] = value
        require(entry["rating"] is not None or entry["pick"] is not None, "Empty change")
        result.append(entry)
    return lines[0].split("\t")[2], result


def table_names(db):
    return [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]


def check_schema(db):
    values = db.execute("SELECT CAST(value AS TEXT) FROM Adobe_variablesTable WHERE name='Adobe_DBVersion'").fetchall()
    require(values == [(VERSION,)], f"Experiment is restricted to observed schema {VERSION}")
    for table, expected in {
        "Adobe_images": {"id_local", "id_global", "rootFile", "masterImage", "copyName", "rating", "pick"},
        "AgLibraryFile": {"id_local", "id_global", "baseName", "extension"},
    }.items():
        actual = {row[1] for row in db.execute(f"PRAGMA table_info({qi(table)})")}
        require(expected <= actual, f"Missing required columns in {table}")


def current_photo(db, uuid):
    rows = db.execute("""SELECT i.id_local, i.id_global, f.id_global, f.baseName || '.' || f.extension,
        i.masterImage IS NOT NULL, i.rating, i.pick, i.copyName
        FROM Adobe_images i JOIN AgLibraryFile f ON f.id_local=i.rootFile WHERE i.id_global=?""", (uuid,)).fetchall()
    if len(rows) != 1:
        return None
    result = dict(zip(("id", "uuid", "file_uuid", "filename", "virtual_copy", "rating", "pick", "copy_name"), rows[0]))
    if "AgLibraryRootFolder" in table_names(db):
        location = db.execute("""SELECT r.absolutePath, d.pathFromRoot
            FROM AgLibraryFile f JOIN AgLibraryFolder d ON d.id_local=f.folder
            JOIN AgLibraryRootFolder r ON r.id_local=d.rootFolder
            JOIN Adobe_images i ON i.rootFile=f.id_local WHERE i.id_local=?""", (result["id"],)).fetchone()
        if location:
            result["original_path"] = "".join(part or "" for part in location) + result["filename"]
    return result


def plan_changes(db, entries):
    plans = []
    for entry in entries:
        current = current_photo(db, entry["uuid"])
        plan = {"entry": entry, "current": current, "status": "already", "fields": {}}
        if current is None:
            plan.update(status="skipped", reason="Photo identity is missing or ambiguous")
        elif any(current[key] != entry[key] for key in ("uuid", "file_uuid", "filename", "virtual_copy")):
            plan.update(status="skipped", reason="Photo/file identity, filename or virtual-copy mismatch")
        else:
            for field, low, high in (("rating", 0, 5), ("pick", -1, 1)):
                raw = current[field]
                value = 0 if raw is None else raw
                if not isinstance(value, (int, float)) or not low <= value <= high or int(value) != value:
                    plan.update(status="skipped", reason=f"Unsupported current {field} encoding")
                    break
                desired = entry[field]
                if desired is not None and desired != value:
                    if value != entry[f"base_{field}"]:
                        plan.update(status="skipped", reason=f"Conflict in {field}")
                        break
                    # Observed catalogs use SQL NULL for unrated and REAL values otherwise.
                    plan["fields"][field] = None if field == "rating" and desired == 0 else float(desired)
            if plan["status"] != "skipped" and plan["fields"]:
                plan["status"] = "apply"
        if plan["status"] == "skipped":
            plan["fields"] = {}
        plans.append(plan)
    return plans


def typed_bytes(value):
    if value is None:
        return b"N"
    if isinstance(value, BinaryText):
        tag, payload = b"S", value
    elif isinstance(value, bytes):
        tag, payload = b"B", value
    elif isinstance(value, str):
        tag, payload = b"S", value.encode("utf-8")
    elif isinstance(value, float):
        tag, payload = b"F", struct.pack(">d", value)
    else:
        tag, payload = b"I", str(value).encode("ascii")
    return tag + struct.pack(">Q", len(payload)) + payload


def fingerprints(db, restore=None):
    """Hash every logical cell, preserving SQLite value types and row multiplicity.

    restore substitutes only approved changed cells with their baseline values;
    matching hashes then prove that all other cells remain unchanged.
    """
    result = {}
    schema = db.execute("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name").fetchall()
    result["schema"] = hashlib.sha256(b"".join(typed_bytes(v) for row in schema for v in row)).hexdigest()
    for table in table_names(db):
        info = db.execute(f"PRAGMA table_info({qi(table)})").fetchall()
        names = [column[1] for column in info]
        keys = [column[1] for column in sorted(info, key=lambda column: column[5]) if column[5]]
        # Without a declared primary key, order by all columns. The catalog's
        # edited image table has id_local as its primary key.
        order = keys or names
        hasher, count = hashlib.sha256(), 0
        for raw in db.execute(f"SELECT {','.join(map(qi, names))} FROM {qi(table)} ORDER BY {','.join(map(qi, order))}"):
            row = list(raw)
            if table == "Adobe_images" and restore:
                for field, original in restore.get(row[names.index("id_local")], {}).items():
                    row[names.index(field)] = original
            hasher.update(b"R")
            for value in row:
                hasher.update(typed_bytes(value))
            count += 1
        result[table] = {"rows": count, "sha256": hasher.hexdigest()}
    return result


def apply_candidate(db, plans):
    # A failed SQL statement rolls back this whole disposable candidate.
    with db:
        db.execute("BEGIN IMMEDIATE")
        for plan in plans:
            if plan["status"] != "apply":
                continue
            fields = plan["fields"]
            assignments = ",".join(f"{qi(key)}=?" for key in fields)
            cursor = db.execute(f"UPDATE Adobe_images SET {assignments} WHERE id_local=? AND id_global=?",
                                [*fields.values(), plan["current"]["id"], plan["entry"]["uuid"]])
            require(cursor.rowcount == 1, "Photo identity changed during candidate update")


def copy_auxiliary(source, output):
    aux = source.with_suffix(".lrcat-data")
    if not aux.exists():
        return {}
    require(aux.is_dir() and not aux.is_symlink(), "Expected a real .lrcat-data directory")
    def inventory():
        files = {}
        for path in sorted(aux.rglob("*")):
            require(not path.is_symlink(), f"Symlinks are not copied from auxiliary data: {path}")
            if not path.is_dir():
                files[str(path.relative_to(aux))] = signature(path)
        return files
    before, hashes = inventory(), {}
    for relative in before:
        path = aux / relative
        hashes[relative] = digest_file(path)
        for variant in ("baseline", "reference", "candidate"):
            dest = output / variant / Path(CATALOG_NAME).with_suffix(".lrcat-data") / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, dest)
            require(digest_file(dest) == hashes[relative], "Auxiliary data changed while copying")
    require(before == inventory(), "Auxiliary data changed while copying")
    return hashes


def write_json(path, value):
    with path.open("x", encoding="utf-8") as file:
        json.dump(value, file, indent=2, ensure_ascii=False)
        file.write("\n")
        file.flush()
        os.fsync(file.fileno())


def prepare(source, changes, output):
    source = source.resolve(strict=True)
    changes_hash = digest_file(changes)
    origin_key, entries = parse_changes(changes)
    require(entries, "Export contains no changes")
    catalog_inputs(source)
    output.mkdir(parents=True, exist_ok=False)
    incomplete = output / "INCOMPLETE.txt"
    incomplete.write_text("Preparation has not completed. Do not use these catalogs.\n", encoding="utf-8")
    for variant in ("baseline", "reference", "candidate"):
        (output / variant).mkdir()
    baseline = output / "baseline" / CATALOG_NAME
    receipt = snapshot(source, baseline)
    candidate = output / "candidate" / "building.sqlite"
    shutil.copyfile(baseline, output / "reference" / CATALOG_NAME)
    shutil.copyfile(baseline, candidate)
    with contextlib.closing(sqlite3.connect(candidate)) as db:
        db.text_factory = decode_text
        db.execute("PRAGMA trusted_schema=OFF")
        db.execute("PRAGMA synchronous=FULL")
        check_schema(db)
        plans = plan_changes(db, entries)
        print("Hashing all baseline tables…", flush=True)
        before = fingerprints(db)
        apply_candidate(db, plans)
        require(db.execute("PRAGMA integrity_check").fetchall() == [("ok",)], "Candidate integrity check failed")
        restore = {plan["current"]["id"]: {key: plan["current"][key] for key in plan["fields"]}
                   for plan in plans if plan["status"] == "apply"}
        print("Verifying every unchanged cell, including Develop and paths…", flush=True)
        require(fingerprints(db, restore) == before, "Unapproved catalog changes detected; candidate is not ready")
        for plan in plans:
            if plan["status"] == "apply":
                actual = current_photo(db, plan["entry"]["uuid"])
                require(actual is not None and all(typed_bytes(actual[key]) == typed_bytes(value)
                                                  for key, value in plan["fields"].items()),
                        "Candidate did not retain expected values")
    auxiliary = copy_auxiliary(source, output)
    verify_source(source, receipt)
    candidate.rename(output / "candidate" / CATALOG_NAME)
    shutil.copyfile(changes, output / "changes.tsv")
    require(digest_file(output / "changes.tsv") == changes_hash, "Changes file changed during experiment")
    require(parse_changes(output / "changes.tsv") == (origin_key, entries), "Export contents changed during experiment")
    report = dict(experiment="column-only; Lightroom bookkeeping not implemented or validated",
                  source=str(source), schema_version=VERSION, export_catalog_key=origin_key,
                  source_unchanged=receipt, plans=plans,
                  counts={status: sum(plan["status"] == status for plan in plans) for status in ("apply", "already", "skipped")},
                  baseline_fingerprints=before, auxiliary_sha256=auxiliary,
                  catalog_sha256={variant: digest_file(output / variant / CATALOG_NAME)
                                  for variant in ("baseline", "reference", "candidate")})
    write_json(output / "report.json", report)
    lines = ["# Lighttable direct-sync experiment", "", "This is a disposable test, not a production sync.", "",
             "- baseline/: keep closed and unchanged; use it to restart a test.",
             "- reference/: apply the listed changes inside Lightroom, then close Lightroom.",
             "- candidate/: changes were applied by SQL; open and inspect, then close Lightroom.",
             "- report.json: identities, skipped conflicts, checksums and preservation evidence.", "",
             "Copy this whole folder to Windows-readable storage before rebooting. Keep .lrcat-data beside each catalog.",
             "Before editing: pause cloud sync and turn off Automatically write changes into XMP. Do not switch cloud sync to a test catalog.",
             "Do not use Read/Save Metadata to File or move/delete originals during this test. The catalog's original file paths are preserved.",
             "Open the .lrcat in reference/ or candidate/ directly; do not import it into your main catalog.",
             "Record Help > System Info's exact Lightroom version. In candidate/, check the desired ratings/flags and compare Develop appearance with reference/.",
             "Filter by filename, then use Copy Name to distinguish virtual copies. Make the reference changes one photo at a time.", "",
             "| Filename | Copy name | Virtual copy | Original rating / flag | Desired rating / flag | Status |",
             "| --- | --- | --- | --- | --- | --- |"]
    for plan in plans:
        entry, current = plan["entry"], plan["current"]
        clean_name = entry["filename"].replace("|", "\\|").replace("\n", " ").replace("\r", " ")
        old = f"{current['rating'] or 0} / {current['pick'] or 0}" if current else "missing"
        desired = " / ".join("unchanged" if entry[field] is None else str(entry[field]) for field in ("rating", "pick"))
        copy_name = str(current["copy_name"] or "") if current else ""
        copy_name = copy_name.replace("|", "\\|").replace("\n", " ").replace("\r", " ")
        lines.append(f"| {clean_name} | {copy_name} | {entry['virtual_copy']} | {old} | {desired} | {plan['status']} |")
    lines += ["", "Exact original paths (use these if filenames repeat):", ""]
    for plan in plans:
        if plan["current"] and plan["current"].get("original_path"):
            lines.append(f"- {plan['entry']['uuid']}: {plan['current']['original_path']}")
    lines += ["", "Flags: -1 = reject, 0 = unflagged, 1 = pick. Rating 0 = no stars.",
              "Do not apply skipped rows in the reference catalog. UUIDs in report.json disambiguate duplicate filenames.", "",
              "After closing both catalogs, retain any remaining -wal files with them and return the folder to Linux.",
              "Compare baseline → reference to identify Lightroom's bookkeeping, then baseline → candidate to see reopen effects.",
              "Keep the main shared catalog unchanged until these results have been reviewed.", ""]
    (output / "START-HERE.md").write_text("\n".join(lines), encoding="utf-8")
    baseline.chmod(0o444)
    incomplete.unlink()
    print(json.dumps({"bundle": str(output.resolve()), **report["counts"]}), flush=True)
    return report


def short_value(value):
    if isinstance(value, bytes) or isinstance(value, str) and len(value) > 160:
        return {"type": type(value).__name__, "length": len(value), "sha256": hashlib.sha256(typed_bytes(value)).hexdigest()}
    return value


def auxiliary_hashes(source):
    aux = source.with_suffix(".lrcat-data")
    if not aux.exists():
        return {}
    require(aux.is_dir() and not aux.is_symlink(), "Expected a real .lrcat-data directory")
    paths = sorted(aux.rglob("*"))
    require(all(not path.is_symlink() for path in paths), "Auxiliary data contains a symlink")
    before = {str(path): signature(path) for path in paths if not path.is_dir()}
    hashes = {str(Path(path).relative_to(aux)): digest_file(Path(path)) for path in before}
    after = {str(path): signature(path) for path in sorted(aux.rglob("*")) if not path.is_dir()}
    require(before == after, "Auxiliary data changed during inspection")
    return hashes


def photo_details(db, uuid):
    photo = current_photo(db, uuid)
    if photo is None:
        return None
    result = {}
    for table, field in (("Adobe_images", "id_local"), ("Adobe_AdditionalMetadata", "image"),
                         ("AgLibraryImageChangeCounter", "image")):
        if table not in table_names(db):
            continue
        cursor = db.execute(f"SELECT * FROM {qi(table)} WHERE {qi(field)}=?", (photo["id"],))
        result[table] = [dict(zip((column[0] for column in cursor.description), map(short_value, row))) for row in cursor]
    return result


def compare(before_path, after_path, changes, report_path):
    before_path, after_path = before_path.resolve(strict=True), after_path.resolve(strict=True)
    _, entries = parse_changes(changes)
    require(not report_path.exists(), "Choose a new comparison report filename")
    before_aux, after_aux = auxiliary_hashes(before_path), auxiliary_hashes(after_path)
    with tempfile.TemporaryDirectory(prefix="lighttable-compare-") as tmp:
        local = Path(tmp)
        before_receipt = snapshot(before_path, local / "before.sqlite")
        after_receipt = snapshot(after_path, local / "after.sqlite")
        with contextlib.closing(sqlite3.connect(local / "before.sqlite")) as before, \
                contextlib.closing(sqlite3.connect(local / "after.sqlite")) as after:
            for db in (before, after):
                db.text_factory = decode_text
                db.execute("PRAGMA trusted_schema=OFF")
                db.execute("PRAGMA query_only=ON")
            print("Comparing logical hashes across every table…", flush=True)
            left, right = fingerprints(before), fingerprints(after)
            changed = {key: {"before": left.get(key), "after": right.get(key)}
                       for key in sorted(left.keys() | right.keys()) if left.get(key) != right.get(key)}
            report = {"changed_tables": changed, "source_unchanged": [before_receipt, after_receipt],
                      "auxiliary_changes": {key: {"before": before_aux.get(key), "after": after_aux.get(key)}
                                            for key in sorted(before_aux.keys() | after_aux.keys())
                                            if before_aux.get(key) != after_aux.get(key)},
                      "photos": [{"uuid": entry["uuid"], "filename": entry["filename"],
                                  "before": photo_details(before, entry["uuid"]),
                                  "after": photo_details(after, entry["uuid"])} for entry in entries],
                      "after_plan": plan_changes(after, entries)}
        verify_source(before_path.resolve(), before_receipt)
        verify_source(after_path.resolve(), after_receipt)
        require(before_aux == auxiliary_hashes(before_path) and after_aux == auxiliary_hashes(after_path),
                "Auxiliary data changed during comparison")
    write_json(report_path, report)
    print(f"Changed tables: {', '.join(changed) or 'none'}; report: {report_path}")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="Create new baseline/reference/candidate catalogs")
    prep.add_argument("--catalog", type=Path, required=True)
    prep.add_argument("--changes", type=Path, required=True)
    prep.add_argument("--output", type=Path, required=True)
    diff = commands.add_parser("compare", help="Compare closed catalogs without opening either source in SQLite")
    diff.add_argument("--before", type=Path, required=True)
    diff.add_argument("--after", type=Path, required=True)
    diff.add_argument("--changes", type=Path, required=True)
    diff.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(args.catalog, args.changes, args.output)
        else:
            compare(args.before, args.after, args.changes, args.report)
    except (ValueError, OSError, sqlite3.Error) as error:
        parser.exit(1, f"Sync experiment stopped: {error}\n")


if __name__ == "__main__":
    main()
