use anyhow::{Result, ensure};
use lighttable::{model::Photo, store::Store};
use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::SystemTime,
};

#[derive(Clone)]
pub struct FileTicket {
    pub path: PathBuf,
    pub bytes: u64,
    pub selected: bool,
    device: u64,
    inode: u64,
    modified: SystemTime,
}
impl FileTicket {
    pub fn capture(path: PathBuf) -> Result<Self> {
        let m = fs::symlink_metadata(&path)?;
        ensure!(
            m.is_file() && !m.file_type().is_symlink(),
            "Only regular files can be moved to Trash"
        );
        Ok(Self {
            path,
            bytes: m.len(),
            selected: true,
            device: m.dev(),
            inode: m.ino(),
            modified: m.modified()?,
        })
    }
    pub fn validate(&self) -> Result<()> {
        let m = fs::symlink_metadata(&self.path)?;
        ensure!(
            m.is_file()
                && !m.file_type().is_symlink()
                && m.dev() == self.device
                && m.ino() == self.inode
                && m.len() == self.bytes
                && m.modified()? == self.modified,
            "File changed after review; review it again before deleting"
        );
        Ok(())
    }
}
pub struct Review {
    pub files: Vec<FileTicket>,
    pub notes: Vec<String>,
}

pub fn prepare(photos: &[Photo], selected: &[usize]) -> Review {
    let mut files = Vec::new();
    let mut notes = Vec::new();
    let mut seen = HashSet::new();
    let mut references = HashMap::new();
    for p in photos {
        *references.entry(&p.file_uuid).or_insert(0usize) += 1;
    }
    let selected_set: HashSet<_> = selected.iter().copied().collect();
    let protected_paths: HashSet<_> = photos
        .iter()
        .enumerate()
        .filter(|(i, _)| !selected_set.contains(i))
        .map(|(_, p)| &p.original)
        .collect();
    for &i in selected {
        let p = &photos[i];
        if p.virtual_copy || references.get(&p.file_uuid).copied().unwrap_or(0) > 1 {
            notes.push(format!("{} shares an original with a virtual copy. Remove it in Lightroom; this alpha will not trash a shared original.",p.filename));
            continue;
        }
        let mut paths = vec![p.original.clone()];
        paths.extend(p.companions.iter().filter(|p| p.is_file()).cloned());
        for ext in ["xmp", "XMP", "acr", "ACR"] {
            let sidecar = p.original.with_extension(ext);
            if sidecar.is_file() {
                paths.push(sidecar);
            }
        }
        for path in paths {
            if protected_paths.contains(&path) {
                notes.push(format!(
                    "{} is also an unselected catalog original; it will be left in place.",
                    path.display()
                ));
                continue;
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            match FileTicket::capture(path.clone()) {
                Ok(ticket) => files.push(ticket),
                Err(e) => notes.push(format!("{}: {e}", path.display())),
            }
        }
    }
    Review { files, notes }
}

pub fn execute(store: &Store, catalog_key: &str, files: &[FileTicket]) -> Vec<String> {
    files
        .iter()
        .filter(|f| f.selected)
        .map(|file| {
            let result = (|| -> Result<()> {
                file.validate()?;
                let id = store.start_trash(catalog_key, &file.path)?;
                match trash::delete(&file.path) {
                    Ok(()) => store.finish_trash(
                        id,
                        "trashed",
                        "Restore with the desktop file manager's Trash",
                    )?,
                    Err(e) => {
                        store.finish_trash(id, "failed", &e.to_string())?;
                        return Err(e.into());
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => format!("Moved to Trash: {}", file.path.display()),
                Err(e) => format!("FAILED: {} — {e:#}", file.path.display()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_file_cannot_be_deleted_with_old_review() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jpg");
        fs::write(&path, b"one").unwrap();
        let ticket = FileTicket::capture(path.clone()).unwrap();
        fs::write(&path, b"replacement").unwrap();
        assert!(ticket.validate().is_err());
        assert!(path.exists());
    }
    #[test]
    fn symlinks_are_not_reviewed_as_originals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target");
        fs::write(&path, b"data").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(path, &link).unwrap();
        assert!(FileTicket::capture(link).is_err());
    }

    #[test]
    #[ignore = "Uses desktop Trash; only a generated temporary file is moved and immediately restored"]
    fn desktop_trash_round_trip() {
        let dir = tempfile::Builder::new()
            .prefix("lighttable-trash-test-")
            .tempdir_in(std::env::current_dir().unwrap())
            .unwrap();
        let path = dir.path().join("disposable-test.jpg");
        fs::write(&path, b"Lighttable disposable Trash test").unwrap();
        let ticket = FileTicket::capture(path.clone()).unwrap();
        let store = Store::open(&dir.path().join("state")).unwrap();
        let results = execute(&store, "test-catalog", &[ticket]);
        assert!(results[0].starts_with("Moved to Trash:"), "{}", results[0]);
        assert!(!path.exists());
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|item| item.original_path() == path)
            .collect();
        assert_eq!(items.len(), 1);
        trash::os_limited::restore_all(items).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"Lighttable disposable Trash test"
        );
        assert_eq!(store.unfinished_trash().unwrap(), 0);
    }
}
