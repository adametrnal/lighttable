use eframe::egui::{self, RichText};
use lighttable::collections::{Collections, Kind};
use std::collections::HashSet;

/// Returns a chosen ordinary collection. Sets expand without changing photo scope.
pub fn collection_tree(
    ui: &mut egui::Ui,
    tree: &Collections,
    expanded: &mut HashSet<i64>,
    selected: Option<i64>,
    height: f32,
    catalog_key: &str,
) -> Option<i64> {
    let rows = tree.visible_rows(expanded);
    let mut chosen = None;
    let scroll_id = ui.make_persistent_id(egui::Id::new(("collections", catalog_key)));
    let selection_id = scroll_id.with("last-selection");
    let last = ui
        .data(|d| d.get_temp::<Option<i64>>(selection_id))
        .flatten();
    ui.data_mut(|d| d.insert_temp(selection_id, selected));
    let pitch = 26.0 + ui.spacing().item_spacing.y;
    let height = height.min(ui.available_height()).max(0.0);
    let max_offset = (rows.len() as f32 * pitch - ui.spacing().item_spacing.y - height).max(0.0);
    let mut offset = egui::scroll_area::State::load(ui.ctx(), scroll_id)
        .unwrap_or_default()
        .offset
        .y
        .clamp(0.0, max_offset);
    if selected != last
        && let Some(row) = rows
            .iter()
            .position(|&(i, _)| Some(tree.nodes[i].id) == selected)
    {
        let top = row as f32 * pitch;
        if top < offset {
            offset = top;
        } else if top + 26.0 > offset + height {
            offset = top + 26.0 - height;
        }
    }
    egui::ScrollArea::vertical()
        .id_salt(("collections", catalog_key))
        .max_height(height)
        .min_scrolled_height(0.0)
        .vertical_scroll_offset(offset.clamp(0.0, max_offset))
        .auto_shrink([false, false])
        .show_rows(ui, 26.0, rows.len(), |ui, range| {
            for row in range {
                let (index, depth) = rows[row];
                let node = &tree.nodes[index];
                ui.push_id(node.id, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding.y = 4.0;
                        let is_set = node.kind == Kind::Set;
                        let open = expanded.contains(&node.id);
                        let can_expand = is_set || !node.children.is_empty();
                        let (badge, hint) = match node.kind {
                            Kind::Collection => (
                                node.members.len().to_string(),
                                "Catalog membership count before search, rating and flag filters.".to_owned(),
                            ),
                            Kind::Set => (String::new(), "Expand or collapse this collection set.".to_owned()),
                            Kind::Smart if node.can_browse() => (format!("Smart {}", node.members.len()),
                                format!("{}\nCalculated in Lighttable using local ratings and capture dates. Counts are before toolbar filters. Experimental; compare with Lightroom.", node.rule_description().unwrap_or_default())),
                            Kind::Smart => ("Unavailable".into(), node.rule_description().unwrap_or_else(|| "Smart rule unavailable".into())),
                            Kind::Unsupported => ("Unsupported".into(), "This collection type or membership is unsupported in Lighttable.".to_owned()),
                        };
                        let badge_width = if badge.is_empty() { 0.0 } else {
                            ui.painter().layout_no_wrap(badge.clone(), egui::FontId::proportional(11.0), ui.visuals().weak_text_color()).size().x + 6.0
                        };
                        let indent = (depth as f32 * 12.0)
                            .min((ui.available_width() - badge_width - 80.0).max(0.0));
                        ui.add_space(indent);
                        let mut toggled = false;
                        if can_expand {
                            let toggle = ui.add_sized([18.0, 26.0], egui::Button::new("").frame(false))
                                .on_hover_text(if open { "Collapse" } else { "Expand" });
                            toggle.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true,
                                format!("{} {}", if open { "Collapse" } else { "Expand" }, node.name)));
                            let center = toggle.rect.center();
                            let points = if open {
                                [egui::vec2(-4.0, -2.0), egui::vec2(4.0, -2.0), egui::vec2(0.0, 3.0)]
                            } else {
                                [egui::vec2(-2.0, -4.0), egui::vec2(-2.0, 4.0), egui::vec2(3.0, 0.0)]
                            };
                            ui.painter().add(egui::Shape::convex_polygon(
                                points.into_iter().map(|p| center + p).collect(),
                                ui.visuals().text_color(), egui::Stroke::NONE));
                            toggled = toggle.clicked();
                        } else {
                            ui.allocate_exact_size(egui::vec2(18.0, 26.0), egui::Sense::hover());
                        }
                        let width = (ui.available_width() - badge_width).max(20.0);
                        let response = ui.add_enabled_ui(
                            node.can_browse() || is_set,
                            |ui| ui.add_sized([width, 26.0], egui::Button::selectable(selected == Some(node.id), &node.name)
                                .right_text("").truncate()),
                        ).inner.on_hover_text(format!("{}\n{hint}", node.name))
                         .on_disabled_hover_text(format!("{}\n{hint}", node.name));
                        if response.clicked() {
                            if is_set { toggled = true; }
                            else { chosen = Some(node.id); }
                        }
                        if toggled {
                            if open { expanded.remove(&node.id); }
                            else { expanded.insert(node.id); }
                        }
                        if !badge.is_empty() {
                            ui.label(RichText::new(badge).size(11.0).weak()).on_hover_text(hint);
                        }
                    });
                });
            }
        });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Collections {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE AgLibraryCollection(id_local,name,creationId,parent,systemOnly);
            CREATE TABLE AgLibraryCollectionImage(collection,image);
            CREATE TABLE AgLibraryCollectionContent(collection,content,owningModule);
            INSERT INTO AgLibraryCollection VALUES
              (1,'Set','com.adobe.ag.library.group',NULL,0),
              (2,'A very long ordinary collection name that must truncate','com.adobe.ag.library.collection',1,0),
              (3,'B smart collection','com.adobe.ag.library.smart_collection',1,0),
              (4,'Print','com.adobe.ag.print',2,0),
              (5,'B2 supported smart','com.adobe.ag.library.smart_collection',1,0);").unwrap();
        db.execute(
            "INSERT INTO AgLibraryCollectionContent VALUES(5,?1,'ag.library.smart_collection')",
            ["s={{criteria='rating',operation='>=',value=3},combine='intersect'}"],
        )
        .unwrap();
        for i in 100..500 {
            db.execute("INSERT INTO AgLibraryCollection VALUES(?1,?2,'com.adobe.ag.library.collection',1,0)",
                rusqlite::params![i, format!("C {i}")]).unwrap();
        }
        lighttable::collections::read(&db, &Default::default(), &mut Vec::new()).unwrap()
    }

    #[test]
    fn tree_clicks_expand_select_and_disable_unsupported_rows_without_overflow() {
        let tree = fixture();
        let ctx = egui::Context::default();
        let mut expanded = HashSet::new();
        let mut selected = None;
        let mut frame = |events: Vec<egui::Event>| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(208.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        ui.spacing_mut().button_padding = egui::vec2(10.0, 7.0);
                        if let Some(id) =
                            collection_tree(ui, &tree, &mut expanded, selected, 200.0, "test")
                        {
                            selected = Some(id);
                        }
                        assert!(ui.min_rect().width() <= 193.0, "sidebar width overflow");
                    });
                },
            );
            assert!(
                output.shapes.len() < 150,
                "offscreen rows should not be painted"
            );
            (expanded.clone(), selected)
        };
        frame(Vec::new());
        let mut click = |pos: egui::Pos2| {
            let event = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(vec![egui::Event::PointerMoved(pos)]);
            frame(vec![event(true)]);
            frame(vec![event(false)]);
            frame(Vec::new()) // register newly expanded rows before the next click
        };
        let (expanded, selected) = click(egui::pos2(70.0, 21.0));
        assert!(expanded.contains(&1));
        assert_eq!(selected, None);
        let (expanded, selected) = click(egui::pos2(90.0, 57.0));
        assert_eq!(selected, Some(2));
        assert!(!expanded.contains(&2)); // name selects an ordinary collection
        let (expanded, selected) = click(egui::pos2(29.0, 57.0));
        assert!(expanded.contains(&2)); // arrow independently opens its saved print child
        assert_eq!(selected, Some(2));
        assert_eq!(click(egui::pos2(90.0, 93.0)).1, Some(2)); // unsupported print
        assert_eq!(click(egui::pos2(90.0, 129.0)).1, Some(2)); // unsupported Smart rules
        assert_eq!(click(egui::pos2(90.0, 165.0)).1, Some(5)); // supported Smart rules are selectable
        let (expanded, selected) = click(egui::pos2(70.0, 21.0));
        assert!(!expanded.contains(&1));
        assert_eq!(selected, Some(5));
    }

    #[test]
    fn offscreen_collection_stays_revealed_and_collapsing_clamps_scroll() {
        let tree = fixture();
        let ctx = egui::Context::default();
        let mut expanded = HashSet::from([1]);
        let frame = |expanded: &mut HashSet<i64>| {
            let mut offset = 0.0;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(248.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        collection_tree(ui, &tree, expanded, Some(499), 200.0, "test");
                        let id = ui.make_persistent_id(egui::Id::new(("collections", "test")));
                        offset = egui::scroll_area::State::load(ctx, id).unwrap().offset.y;
                    });
                },
            );
            offset
        };
        let revealed = frame(&mut expanded);
        assert!(revealed > 10_000.0);
        for _ in 0..3 {
            assert!((frame(&mut expanded) - revealed).abs() < 1.0);
        }
        expanded.clear();
        assert_eq!(frame(&mut expanded), 0.0);
        assert_eq!(frame(&mut expanded), 0.0);
    }
}
