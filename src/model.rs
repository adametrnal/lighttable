use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Preview {
    pub path: PathBuf,
    pub long_side: u32,
    pub adobe_rgb: bool,
}

#[derive(Clone, Debug)]
pub struct Photo {
    pub id: i64,
    pub uuid: String,
    pub file_uuid: String,
    pub filename: String,
    pub search: String,
    pub folder: String,
    pub original: PathBuf,
    pub companions: Vec<PathBuf>,
    pub format: String,
    pub captured: String,
    pub captured_date: Option<chrono::NaiveDate>,
    pub width: u32,
    pub height: u32,
    pub camera: String,
    pub lens: String,
    pub aperture: Option<f64>,
    pub shutter: Option<f64>,
    pub iso: Option<f64>,
    pub focal: Option<f64>,
    pub caption: String,
    pub keywords: String,
    pub copy_name: String,
    pub virtual_copy: bool,
    pub orientation: String,
    pub source_rating: i8,
    pub source_pick: i8,
    pub rating: i8,
    pub pick: i8,
    pub previews: Vec<Preview>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub uuid: String,
    pub file_uuid: String,
    pub filename: String,
    pub virtual_copy: bool,
    pub base_rating: i8,
    pub base_pick: i8,
    pub rating: Option<i8>,
    pub pick: Option<i8>,
}

impl Change {
    pub fn for_photo(p: &Photo) -> Self {
        Self {
            uuid: p.uuid.clone(),
            file_uuid: p.file_uuid.clone(),
            filename: p.filename.clone(),
            virtual_copy: p.virtual_copy,
            base_rating: p.source_rating,
            base_pick: p.source_pick,
            rating: None,
            pick: None,
        }
    }
    pub fn empty(&self) -> bool {
        self.rating.is_none() && self.pick.is_none()
    }
    pub fn apply(&self, p: &mut Photo) {
        p.rating = self.rating.unwrap_or(p.source_rating);
        p.pick = self.pick.unwrap_or(p.source_pick);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub query: String,
    pub folder: String,
    pub collection: Option<i64>,
    pub minimum_rating: i8,
    pub pick: Option<i8>,
    pub format: String,
    pub sort_name: bool,
}

pub fn filtered(photos: &[Photo], filter: &Filter) -> Vec<usize> {
    filtered_from(photos, filter, 0..photos.len())
}

/// Apply metadata predicates to an already resolved catalog scope.
pub fn filtered_from(
    photos: &[Photo],
    filter: &Filter,
    candidates: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let query = filter.query.to_lowercase();
    let mut ids: Vec<usize> = candidates
        .filter(|&i| {
            let p = &photos[i];
            p.rating >= filter.minimum_rating
                && filter.pick.is_none_or(|flag| p.pick == flag)
                && (filter.folder.is_empty() || p.folder == filter.folder)
                && (filter.format.is_empty() || p.format == filter.format)
                && (query.is_empty() || p.search.contains(&query))
        })
        .collect();
    if filter.sort_name {
        ids.sort_unstable_by(|&a, &b| {
            photos[a]
                .filename
                .cmp(&photos[b].filename)
                .then(photos[a].id.cmp(&photos[b].id))
        });
    }
    ids
}
