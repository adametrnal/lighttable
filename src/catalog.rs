use crate::{
    collections::{self, Collections, Kind},
    model::{self, Filter, Photo, Preview},
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

pub struct Catalog {
    pub path: PathBuf,
    pub key: String,
    pub version: String,
    pub photos: Vec<Photo>,
    pub collections: Collections,
    pub warnings: Vec<String>,
    pub load_ms: u128,
}

impl Catalog {
    pub fn filtered(&self, filter: &Filter) -> Vec<usize> {
        match filter.collection {
            None => model::filtered(&self.photos, filter),
            Some(id) => {
                let members = self
                    .collections
                    .get(id)
                    .filter(|c| c.can_browse())
                    .map_or(&[][..], |c| c.members.as_slice());
                model::filtered_from(&self.photos, filter, members.iter().copied())
            }
        }
    }
}

fn signature(path: &Path) -> Result<Option<(u64, SystemTime)>> {
    match fs::metadata(path) {
        Ok(m) => Ok(Some((m.len(), m.modified()?))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// Read bytes only from the supplied package. SQLite recovery happens on our copy.
pub fn snapshot(source: &Path, destination: &Path) -> Result<Connection> {
    let wal = PathBuf::from(format!("{}-wal", source.display()));
    let journal = PathBuf::from(format!("{}-journal", source.display()));
    ensure!(
        !journal.exists(),
        "A rollback journal is present. Close Lightroom and finish copying the catalog first."
    );
    let before = (signature(source)?, signature(&wal)?);
    fs::copy(source, destination).with_context(|| format!("Cannot copy {}", source.display()))?;
    if before.1.is_some_and(|(len, _)| len > 0) {
        fs::copy(&wal, format!("{}-wal", destination.display()))?;
    }
    ensure!(
        before == (signature(source)?, signature(&wal)?),
        "Catalog changed while opening. Close Lightroom and wait for the transfer to finish, then retry."
    );
    let c = Connection::open(destination)?;
    c.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")?;
    Ok(c)
}

pub fn load(path: &Path, root_override: Option<&Path>) -> Result<Catalog> {
    let start = Instant::now();
    let path = path
        .canonicalize()
        .context("The catalog path does not exist")?;
    ensure!(
        path.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("lrcat")),
        "Choose a .lrcat file"
    );
    for lock in [
        PathBuf::from(format!("{}.lock", path.display())),
        path.with_extension("lrcat.lock"),
    ] {
        ensure!(
            !lock.exists(),
            "Lightroom's catalog lock is present. Close Lightroom before opening this copy."
        );
    }
    let folder = path.parent().context("Catalog has no parent directory")?;
    let temp = tempfile::tempdir()?;
    let db = snapshot(&path, &temp.path().join("catalog.sqlite"))?;
    if root_override.is_some() {
        let roots: i64 =
            db.query_row("SELECT count(*) FROM AgLibraryRootFolder", [], |r| r.get(0))?;
        ensure!(
            roots == 1,
            "The alpha's replacement-root option supports one catalog root. This catalog has {roots}; use a portable catalog copy with catalog-relative roots instead."
        );
    }
    let version: String = db
        .query_row(
            "SELECT CAST(value AS TEXT) FROM Adobe_variablesTable WHERE name='Adobe_DBVersion'",
            [],
            |r| r.get(0),
        )
        .context("This file does not have a supported Lightroom Classic catalog schema")?;
    let key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
    let mut warnings = Vec::new();
    let mut statement = db.prepare(
        "SELECT CAST(i.id_local AS INTEGER), CAST(i.id_global AS TEXT), CAST(f.id_global AS TEXT),
        f.baseName || '.' || f.extension, d.pathFromRoot, r.absolutePath, r.relativePathFromCatalog,
        i.fileFormat, i.captureTime, CAST(i.fileWidth AS INTEGER), CAST(i.fileHeight AS INTEGER),
        cm.value, lens.value, e.aperture, e.shutterSpeed, e.isoSpeedRating, e.focalLength,
        ip.caption, i.copyName, i.masterImage IS NOT NULL, i.orientation,
        CAST(COALESCE(i.rating,0) AS INTEGER), CAST(COALESCE(i.pick,0) AS INTEGER), f.sidecarExtensions
        FROM Adobe_images i
        JOIN AgLibraryFile f ON f.id_local=i.rootFile
        JOIN AgLibraryFolder d ON d.id_local=f.folder
        JOIN AgLibraryRootFolder r ON r.id_local=d.rootFolder
        LEFT JOIN AgHarvestedExifMetadata e ON e.image=i.id_local
        LEFT JOIN AgInternedExifCameraModel cm ON cm.id_local=e.cameraModelRef
        LEFT JOIN AgInternedExifLens lens ON lens.id_local=e.lensRef
        LEFT JOIN AgLibraryIPTC ip ON ip.image=i.id_local
        ORDER BY i.captureTime, i.id_local"
    ).context("Unsupported catalog schema: required photo/metadata columns are missing")?;
    let mut photos = Vec::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let text = |i| -> rusqlite::Result<String> {
            Ok(row.get::<_, Option<String>>(i)?.unwrap_or_default())
        };
        let filename = text(3)?;
        let relative_folder = text(4)?;
        let stored_root = text(5)?;
        let relative_root = text(6)?;
        let original = resolve_path(
            folder,
            &stored_root,
            &relative_root,
            &relative_folder,
            &filename,
            root_override,
        );
        let companions = text(23)?
            .split([',', ';', ' '])
            .filter(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(|extension| original.with_extension(extension))
            .filter(|p| *p != original)
            .collect();
        let rating = row.get::<_, i8>(21)?;
        let pick = row.get::<_, i8>(22)?;
        ensure!(
            (0..=5).contains(&rating) && (-1..=1).contains(&pick),
            "Unsupported rating or flag encoding in {filename}"
        );
        let camera = text(11)?;
        let lens = text(12)?;
        let captured = text(8)?;
        let search =
            format!("{filename} {relative_folder} {camera} {lens} {captured}").to_lowercase();
        photos.push(Photo {
            id: row.get(0)?,
            uuid: text(1)?,
            file_uuid: text(2)?,
            filename,
            search,
            folder: format!("{}{}", relative_root, relative_folder),
            original,
            companions,
            format: text(7)?,
            captured_date: captured.get(..10).and_then(crate::smart::date),
            captured,
            width: row.get::<_, Option<u32>>(9)?.unwrap_or(0),
            height: row.get::<_, Option<u32>>(10)?.unwrap_or(0),
            camera,
            lens,
            aperture: row.get::<_, Option<f64>>(13)?.map(|x| 2_f64.powf(x / 2.0)),
            shutter: row.get::<_, Option<f64>>(14)?.map(|x| 2_f64.powf(-x)),
            iso: row.get(15)?,
            focal: row.get(16)?,
            caption: text(17)?,
            keywords: String::new(),
            copy_name: text(18)?,
            virtual_copy: row.get(19)?,
            orientation: text(20)?,
            source_rating: rating,
            source_pick: pick,
            rating,
            pick,
            previews: Vec::new(),
        });
    }
    let count = db.query_row("SELECT count(*) FROM Adobe_images", [], |r| {
        r.get::<_, i64>(0)
    })? as usize;
    ensure!(
        photos.len() == count,
        "Catalog relationships are incomplete: found {} of {count} photos",
        photos.len()
    );
    let mut positions = HashMap::new();
    let mut uuids = std::collections::HashSet::new();
    for (i, p) in photos.iter().enumerate() {
        ensure!(
            !p.uuid.is_empty() && uuids.insert(p.uuid.clone()),
            "Duplicate or empty photo UUID in catalog"
        );
        positions.insert(p.id, i);
    }
    if let Ok(mut s) = db.prepare("SELECT CAST(ki.image AS INTEGER), group_concat(k.name, ', ') FROM AgLibraryKeywordImage ki JOIN AgLibraryKeyword k ON k.id_local=ki.tag GROUP BY ki.image") {
        let mut rows = s.query([])?;
        while let Some(row) = rows.next()? {
            if let Some(&i) = positions.get(&row.get::<_,i64>(0)?) {
                photos[i].keywords = row.get::<_,Option<String>>(1)?.unwrap_or_default();
                let keywords = photos[i].keywords.to_lowercase();
                photos[i].search.push_str(&format!(" {keywords}"));
            }
        }
    }
    let mut collections = match collections::read(&db, &positions, &mut warnings) {
        Ok(tree) => tree,
        Err(e) => {
            warnings.push(format!("Collections unavailable: {e:#}"));
            Collections::default()
        }
    };
    collections.refresh_smart(&photos, crate::smart::today());
    let stem = path
        .file_stem()
        .context("Catalog has no name")?
        .to_string_lossy();
    let preview_root = folder.join(format!("{stem} Previews.lrdata"));
    if preview_root.join("previews.db").exists() {
        let result = (|| -> Result<()> {
            let previews = snapshot(
                &preview_root.join("previews.db"),
                &temp.path().join("previews.sqlite"),
            )?;
            let mut s = previews.prepare("SELECT CAST(c.imageId AS INTEGER), c.uuid, c.digest, CAST(l.longDimension AS INTEGER), p.colorProfile
                FROM ImageCacheEntry c JOIN PyramidLevel l ON c.uuid=l.uuid AND c.digest=l.digest
                JOIN Pyramid p ON p.uuid=c.uuid AND p.digest=c.digest ORDER BY l.longDimension")?;
            let mut rows = s.query([])?;
            while let Some(r) = rows.next()? {
                if let Some(&i) = positions.get(&r.get::<_, i64>(0)?) {
                    let uuid: String = r.get(1)?;
                    let digest: String = r.get(2)?;
                    let size: u32 = r.get(3)?;
                    if !safe_token(&uuid) || !safe_token(&digest) || uuid.len() < 4 {
                        continue;
                    }
                    let path = preview_root
                        .join(&uuid[..1])
                        .join(&uuid[..4])
                        .join(format!("{uuid}-{digest}_{size}"));
                    photos[i].previews.push(Preview {
                        path,
                        long_side: size,
                        adobe_rgb: r.get::<_, String>(4)? == "AdobeRGB",
                    });
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warnings.push(format!("Preview index unavailable: {e:#}"));
        }
    } else {
        warnings.push(
            "No standard preview database. JPEG originals/companions will be used where available."
                .into(),
        );
    }
    if photos.is_empty() {
        warnings.push("This catalog contains no photos.".into());
    }
    Ok(Catalog {
        path,
        key,
        version,
        photos,
        collections,
        warnings,
        load_ms: start.elapsed().as_millis(),
    })
}

fn safe_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

pub fn resolve_path(
    catalog_folder: &Path,
    absolute_root: &str,
    relative_root: &str,
    folder: &str,
    name: &str,
    root_override: Option<&Path>,
) -> PathBuf {
    if let Some(root) = root_override {
        return root.join(folder.replace('\\', "/")).join(name);
    }
    let relative = relative_root.replace('\\', "/");
    // A non-empty catalog-relative root is Lightroom's portable location hint.
    let base = if !relative.is_empty() && !Path::new(&relative).is_absolute() {
        catalog_folder.join(relative)
    } else {
        PathBuf::from(absolute_root.replace('\\', "/"))
    };
    base.join(folder.replace('\\', "/")).join(name)
}

pub fn inspect(path: &Path, verify: bool) -> Result<()> {
    let c = load(path, None)?;
    println!(
        "{}: {} photos, schema {}, loaded in {} ms",
        c.path.display(),
        c.photos.len(),
        c.version,
        c.load_ms
    );
    println!(
        "{} catalog originals resolve; {} photos have preview index entries",
        c.photos.iter().filter(|p| p.original.is_file()).count(),
        c.photos.iter().filter(|p| !p.previews.is_empty()).count()
    );
    for (kind, label) in [
        (Kind::Collection, "ordinary collections"),
        (Kind::Set, "collection sets"),
        (Kind::Smart, "Smart Collections"),
        (Kind::Unsupported, "unsupported collections"),
    ] {
        println!(
            "{} {label}",
            c.collections
                .nodes
                .iter()
                .filter(|n| n.kind == kind)
                .count()
        );
    }
    let smart = c.collections.nodes.iter().filter(|n| n.kind == Kind::Smart);
    println!(
        "{} Smart Collections support local date/rating evaluation (experimental)",
        smart.filter(|n| n.can_browse()).count()
    );
    for w in &c.warnings {
        println!("Warning: {w}");
    }
    if verify {
        let mut failed = 0;
        for p in &c.photos {
            match crate::images::decode_photo(p, 400) {
                Ok(decoded) => println!(
                    "OK {}: {}×{} ({})",
                    p.filename,
                    decoded.image.width(),
                    decoded.image.height(),
                    decoded.source
                ),
                Err(e) => {
                    failed += 1;
                    println!("FAIL {}: {e:#}", p.filename);
                }
            }
        }
        if failed > 0 {
            bail!("{failed} photos have no decodable preview");
        }
    }
    Ok(())
}
