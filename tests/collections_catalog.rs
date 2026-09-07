//! Opt-in verification against a closed local catalog; no personal fixtures are checked in.
use lighttable::smart::{Comparison, Rule, Unit};
use lighttable::{catalog, collections::Kind, model::Filter};
use std::{collections::BTreeSet, path::PathBuf, time::Instant};

fn comparison(op: &Comparison) -> &'static str {
    match op {
        Comparison::Eq => "=",
        Comparison::Ne => "!=",
        Comparison::Lt => "<",
        Comparison::Le => "<=",
        Comparison::Gt => ">",
        Comparison::Ge => ">=",
    }
}

// An independent SQL execution path checks metadata adaptation and membership.
// Parser fixtures separately check how the saved Lua data becomes this rule tree.
fn smart_sql(rule: &Rule, today: &str, params: &mut Vec<String>) -> String {
    let captured = "date(substr(i.captureTime,1,10))";
    match rule {
        Rule::All(rules) | Rule::Any(rules) => {
            let join = if matches!(rule, Rule::All(_)) {
                " AND "
            } else {
                " OR "
            };
            format!(
                "({})",
                rules
                    .iter()
                    .map(|r| smart_sql(r, today, params))
                    .collect::<Vec<_>>()
                    .join(join)
            )
        }
        Rule::Rating(op, value) => {
            params.push(value.to_string());
            format!(
                "CAST(COALESCE(i.rating,0) AS INTEGER) {} CAST(? AS INTEGER)",
                comparison(op)
            )
        }
        Rule::Capture(op, value) => {
            params.push(value.to_string());
            format!("{captured} {} date(?)", comparison(op))
        }
        Rule::CaptureRange(start, end) => {
            params.extend([start.to_string(), end.to_string()]);
            format!("{captured} BETWEEN date(?) AND date(?)")
        }
        Rule::CaptureLast(n, unit) => {
            params.extend([today.into(), n.to_string(), today.into()]);
            let unit = match unit {
                Unit::Days => "days",
                Unit::Months => "months",
            };
            format!("{captured} BETWEEN date(?, '-' || ? || ' {unit}', 'floor') AND date(?)")
        }
    }
}

#[test]
#[ignore = "Set LIGHTTABLE_COLLECTION_CATALOG to a closed .lrcat copy"]
fn smart_collections_match_sql() {
    let path =
        PathBuf::from(std::env::var_os("LIGHTTABLE_COLLECTION_CATALOG").expect("catalog path"));
    let mut c = catalog::load(&path, None).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let db = catalog::snapshot(&path, &temp.path().join("verify.sqlite")).unwrap();
    // Exercise relative rules with a historical date too; today's old catalog may return zero.
    for today in [
        lighttable::smart::today(),
        lighttable::smart::date("2024-07-31").unwrap(),
    ] {
        c.collections.refresh_smart(&c.photos, today);
        let mut supported = 0;
        for node in c.collections.nodes.iter().filter(|n| n.kind == Kind::Smart) {
            let Some(Ok(rule)) = &node.smart_rule else {
                assert!(!node.can_browse());
                assert!(node.members.is_empty());
                continue;
            };
            supported += 1;
            let mut params = Vec::new();
            let predicate = smart_sql(rule, &today.to_string(), &mut params);
            let sql = format!("SELECT i.id_global FROM Adobe_images i WHERE {predicate}");
            let expected: BTreeSet<String> = db
                .prepare(&sql)
                .unwrap()
                .query_map(rusqlite::params_from_iter(params), |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let actual: BTreeSet<_> = c
                .filtered(&Filter {
                    collection: Some(node.id),
                    ..Default::default()
                })
                .iter()
                .map(|&i| c.photos[i].uuid.clone())
                .collect();
            assert_eq!(actual, expected, "Smart collection {} on {today}", node.id);
            println!(
                "Smart {}: {} matches on {today}; {}",
                node.id,
                actual.len(),
                rule.description()
            );
        }
        assert!(supported > 0);
        println!(
            "{supported} Smart Collections matched SQL on {today}, including exact UUID membership."
        );
    }
    let mut times = Vec::new();
    let today = lighttable::smart::today();
    for _ in 0..30 {
        let start = Instant::now();
        c.collections.refresh_smart(&c.photos, today);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "Recalculate all supported Smart Collections over {} photos: p50 {:.2} ms / p95 {:.2} ms (metadata only).",
        c.photos.len(),
        times[15],
        times[28]
    );
}

#[test]
#[ignore = "Set LIGHTTABLE_COLLECTION_CATALOG to a closed .lrcat copy"]
fn collections_match_catalog_sql() {
    let path =
        PathBuf::from(std::env::var_os("LIGHTTABLE_COLLECTION_CATALOG").expect("catalog path"));
    let c = catalog::load(&path, None).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let db = catalog::snapshot(&path, &temp.path().join("verify.sqlite")).unwrap();
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM AgLibraryCollection WHERE CAST(COALESCE(systemOnly,0) AS INTEGER)=0",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(c.collections.nodes.len(), count as usize);
    let mut membership = db
        .prepare(
            "SELECT DISTINCT i.id_global FROM AgLibraryCollectionImage ci
         JOIN Adobe_images i ON i.id_local=ci.image WHERE ci.collection=?1",
        )
        .unwrap();
    let mut ordinary = 0;
    let mut memberships = 0;
    let mut copies = 0;
    let mut times = Vec::new();
    for node in &c.collections.nodes {
        let parent: Option<i64> = db
            .query_row(
                "SELECT CAST(parent AS INTEGER) FROM AgLibraryCollection WHERE id_local=?1",
                [node.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(node.parent.map(|i| c.collections.nodes[i].id), parent);
        if node.kind != Kind::Collection {
            continue;
        }
        ordinary += 1;
        let filter = Filter {
            collection: Some(node.id),
            ..Default::default()
        };
        let result = c.filtered(&filter);
        let expected: BTreeSet<String> = membership
            .query_map([node.id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let actual: BTreeSet<_> = result.iter().map(|&i| c.photos[i].uuid.clone()).collect();
        assert_eq!(actual, expected, "collection {}", node.id);
        assert_eq!(node.members.len(), expected.len());
        memberships += result.len();
        copies += result.iter().filter(|&&i| c.photos[i].virtual_copy).count();
        for _ in 0..30 {
            let start = Instant::now();
            std::hint::black_box(c.filtered(&filter));
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    assert!(ordinary > 0);
    times.sort_by(f64::total_cmp);
    println!(
        "Verified {ordinary} ordinary collections, {memberships} memberships ({copies} virtual-copy memberships), and all {count} visible parent links across {} photos. Load {} ms; warm collection filter p50 {:.3} ms / p95 {:.3} ms ({} queries; metadata only).",
        c.photos.len(),
        c.load_ms,
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times.len(),
    );
}
