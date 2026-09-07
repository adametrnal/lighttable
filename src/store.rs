use crate::model::Change;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LIGHTTABLE_DATA_DIR") {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(p).join("lighttable");
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
        .join(".local/share/lighttable")
}

pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let db = Connection::open(dir.join("selections.sqlite"))?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS changes(catalog TEXT NOT NULL, uuid TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(catalog,uuid));
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS file_operations(id INTEGER PRIMARY KEY, catalog TEXT NOT NULL, path TEXT NOT NULL, status TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '', created TEXT DEFAULT CURRENT_TIMESTAMP);")?;
        Ok(Self { db })
    }
    pub fn changes(&self, key: &str) -> Result<HashMap<String, Change>> {
        let mut s = self
            .db
            .prepare("SELECT data FROM changes WHERE catalog=?")?;
        let mut result = HashMap::new();
        for data in s.query_map([key], |r| r.get::<_, String>(0))? {
            let c: Change = serde_json::from_str(&data?)?;
            ensure!(
                c.rating.is_none_or(|v| (0..=5).contains(&v))
                    && c.pick.is_none_or(|v| (-1..=1).contains(&v)),
                "Invalid saved selection values"
            );
            result.insert(c.uuid.clone(), c);
        }
        Ok(result)
    }
    pub fn save(&mut self, key: &str, changes: &[Change]) -> Result<()> {
        let tx = self.db.transaction()?;
        for c in changes {
            ensure!(
                (0..=5).contains(&c.base_rating)
                    && (-1..=1).contains(&c.base_pick)
                    && c.rating.is_none_or(|v| (0..=5).contains(&v))
                    && c.pick.is_none_or(|v| (-1..=1).contains(&v)),
                "Invalid rating or flag"
            );
            if c.empty() {
                tx.execute(
                    "DELETE FROM changes WHERE catalog=? AND uuid=?",
                    params![key, c.uuid],
                )?;
            } else {
                tx.execute("INSERT INTO changes(catalog,uuid,data) VALUES(?,?,?) ON CONFLICT(catalog,uuid) DO UPDATE SET data=excluded.data", params![key,c.uuid,serde_json::to_string(c)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn setting(&self, key: &str) -> Option<String> {
        self.db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .ok()
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        Ok(())
    }
    pub fn start_trash(&self, key: &str, path: &Path) -> Result<i64> {
        self.db.execute(
            "INSERT INTO file_operations(catalog,path,status) VALUES(?,?,'pending')",
            params![key, path.to_string_lossy()],
        )?;
        Ok(self.db.last_insert_rowid())
    }
    pub fn finish_trash(&self, id: i64, status: &str, detail: &str) -> Result<()> {
        self.db.execute(
            "UPDATE file_operations SET status=?,detail=? WHERE id=?",
            params![status, detail, id],
        )?;
        Ok(())
    }
    pub fn unfinished_trash(&self) -> Result<usize> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM file_operations WHERE status='pending'",
            [],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}

pub fn export(path: &Path, catalog_key: &str, changes: &HashMap<String, Change>) -> Result<usize> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file =
        tempfile::NamedTempFile::new_in(parent).context("Cannot create export in this folder")?;
    writeln!(file, "LIGHTTABLE\t1\t{catalog_key}")?;
    writeln!(
        file,
        "uuid\tfile_uuid\tfilename_hex\tvirtual_copy\tbase_rating\trating\tbase_pick\tpick"
    )?;
    let mut sorted: Vec<_> = changes.values().filter(|c| !c.empty()).collect();
    sorted.sort_by_key(|c| &c.uuid);
    for c in &sorted {
        ensure!(
            [&c.uuid, &c.file_uuid]
                .iter()
                .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')),
            "Invalid photo identity"
        );
        let filename: String = c
            .filename
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            c.uuid,
            c.file_uuid,
            filename,
            u8::from(c.virtual_copy),
            c.base_rating,
            c.rating.map_or("-".into(), |v| v.to_string()),
            c.base_pick,
            c.pick.map_or("-".into(), |v| v.to_string())
        )?;
    }
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .context("Could not save export (choose a new filename if it already exists)")?;
    Ok(sorted.len())
}
