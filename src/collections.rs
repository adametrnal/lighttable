//! Read-only collection membership from the observed Lightroom Classic schema.
use crate::{
    model::Photo,
    smart::{self, Rule},
};
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Collection,
    Set,
    Smart,
    Unsupported,
}

#[derive(Debug)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Photo positions in capture order, never file IDs (virtual copies are distinct).
    pub members: Vec<usize>,
    pub smart_rule: Option<Result<Rule, String>>,
}

impl Collection {
    pub fn can_browse(&self) -> bool {
        self.kind == Kind::Collection || self.smart_rule.as_ref().is_some_and(Result::is_ok)
    }

    pub fn rule_description(&self) -> Option<String> {
        self.smart_rule.as_ref().map(|r| match r {
            Ok(rule) => rule.description(),
            Err(reason) => reason.clone(),
        })
    }
}

#[derive(Debug, Default)]
pub struct Collections {
    pub nodes: Vec<Collection>,
    pub roots: Vec<usize>,
    by_id: HashMap<i64, usize>,
    pub evaluated_on: Option<NaiveDate>,
}

impl Collections {
    /// Refresh after loading overlays, rating edits/undo, or a change of local date.
    pub fn refresh_smart(&mut self, photos: &[Photo], today: NaiveDate) {
        for node in &mut self.nodes {
            if let Some(Ok(rule)) = &node.smart_rule {
                node.members.clear();
                node.members.extend(
                    photos
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| rule.matches(p.rating, p.captured_date, today))
                        .map(|(i, _)| i),
                );
            }
        }
        self.evaluated_on = Some(today);
    }
    pub fn get(&self, id: i64) -> Option<&Collection> {
        self.by_id.get(&id).map(|&i| &self.nodes[i])
    }

    /// Flatten only expanded branches for the sidebar's virtualized rows.
    pub fn visible_rows(&self, expanded: &HashSet<i64>) -> Vec<(usize, usize)> {
        let mut pending: Vec<_> = self.roots.iter().rev().map(|&i| (i, 0)).collect();
        let mut rows = Vec::new();
        while let Some((i, depth)) = pending.pop() {
            rows.push((i, depth));
            if expanded.contains(&self.nodes[i].id) {
                pending.extend(self.nodes[i].children.iter().rev().map(|&j| (j, depth + 1)));
            }
        }
        rows
    }
}

pub fn read(
    db: &Connection,
    photo_positions: &HashMap<i64, usize>,
    warnings: &mut Vec<String>,
) -> Result<Collections> {
    let mut tree = Collections::default();
    let mut parents = Vec::new();
    let mut statement = db.prepare(
        "SELECT CAST(id_local AS INTEGER), name, creationId, CAST(parent AS INTEGER)
         FROM AgLibraryCollection WHERE CAST(COALESCE(systemOnly, 0) AS INTEGER)=0
         ORDER BY name COLLATE NOCASE, id_local",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = row.get(0)?;
        ensure!(
            tree.by_id.insert(id, tree.nodes.len()).is_none(),
            "Duplicate collection ID"
        );
        let creation: String = row.get(2)?;
        tree.nodes.push(Collection {
            id,
            name: row.get(1)?,
            kind: match creation.as_str() {
                "com.adobe.ag.library.collection" => Kind::Collection,
                "com.adobe.ag.library.group" => Kind::Set,
                "com.adobe.ag.library.smart_collection" => Kind::Smart,
                _ => Kind::Unsupported,
            },
            parent: None,
            children: Vec::new(),
            members: Vec::new(),
            smart_rule: (creation == "com.adobe.ag.library.smart_collection")
                .then(|| Err("Smart rule definition is missing".into())),
        });
        parents.push(row.get::<_, Option<i64>>(3)?);
    }
    for (i, parent_id) in parents.into_iter().enumerate() {
        if let Some(id) = parent_id {
            // Saved print collections can be children of an ordinary collection.
            match tree.by_id.get(&id).copied() {
                Some(p) => tree.nodes[i].parent = Some(p),
                _ => warnings.push(format!(
                    "Collection '{}' has an unavailable parent; shown at the top level.",
                    tree.nodes[i].name
                )),
            }
        }
    }
    // Break malformed parent cycles before traversing; avoid recursive stack growth.
    let mut state = vec![0u8; tree.nodes.len()];
    for i in 0..tree.nodes.len() {
        let mut chain = Vec::new();
        let mut next = Some(i);
        while let Some(j) = next {
            if state[j] == 2 {
                break;
            }
            if state[j] == 1 {
                tree.nodes[j].parent = None;
                warnings.push(format!(
                    "Collection hierarchy cycle at '{}'; shown at the top level.",
                    tree.nodes[j].name
                ));
                break;
            }
            state[j] = 1;
            chain.push(j);
            next = tree.nodes[j].parent;
        }
        for j in chain {
            state[j] = 2;
        }
    }
    for i in 0..tree.nodes.len() {
        if let Some(p) = tree.nodes[i].parent {
            tree.nodes[p].children.push(i);
        } else {
            tree.roots.push(i);
        }
    }
    let mut statement = db.prepare(
        "SELECT CAST(collection AS INTEGER), CAST(image AS INTEGER) FROM AgLibraryCollectionImage",
    )?;
    let mut rows = statement.query([])?;
    let mut missing = HashSet::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let image: i64 = row.get(1)?;
        if let Some(&i) = tree.by_id.get(&id)
            && tree.nodes[i].kind == Kind::Collection
        {
            if let Some(&position) = photo_positions.get(&image) {
                tree.nodes[i].members.push(position);
            } else {
                missing.insert(id);
            }
        }
    }
    for node in &mut tree.nodes {
        node.members.sort_unstable();
        node.members.dedup();
        // Do not expose an incomplete membership as a trustworthy editable selection.
        if missing.contains(&node.id) {
            node.kind = Kind::Unsupported;
            node.members.clear();
            warnings.push(format!(
                "Collection '{}' references missing photos; browsing it is unavailable.",
                node.name
            ));
        }
    }
    if tree.nodes.iter().any(|n| n.kind == Kind::Smart) {
        // A missing optional rule table must not disable ordinary collections.
        let result = (|| -> Result<()> {
            let mut statement = db.prepare(
                "SELECT CAST(collection AS INTEGER), content FROM AgLibraryCollectionContent
                 WHERE owningModule='ag.library.smart_collection'",
            )?;
            let mut rows = statement.query([])?;
            let mut seen = HashSet::new();
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                if let Some(&i) = tree.by_id.get(&id)
                    && tree.nodes[i].kind == Kind::Smart
                {
                    tree.nodes[i].smart_rule = Some(if !seen.insert(id) {
                        Err("Multiple Smart rule definitions found".into())
                    } else {
                        let source: Option<String> = row.get(1)?;
                        smart::parse(source.as_deref().unwrap_or_default())
                            .map_err(|e| e.to_string())
                    });
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            for node in &mut tree.nodes {
                if node.kind == Kind::Smart {
                    node.smart_rule = Some(Err(format!("Smart rules unavailable: {e}")));
                }
            }
        }
    }
    Ok(tree)
}
