use crate::{
    cache::ImageCache,
    removal::{self, Review},
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2};
use lighttable::{
    catalog::{self, Catalog},
    model::{self, Change, Filter},
    store::{self, Store},
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const GOLD: Color32 = Color32::from_rgb(226, 183, 107);
const MUTED: Color32 = Color32::from_rgb(151, 158, 165);

enum Command {
    Load(PathBuf, Option<PathBuf>),
    Save(String, Vec<Change>),
    Export(String, PathBuf),
    Review(Vec<model::Photo>, Vec<usize>),
    Trash(String, Vec<removal::FileTicket>),
}
enum Event {
    Recent(Option<String>, usize),
    Loaded(Result<(Box<Catalog>, HashMap<String, Change>), String>),
    Saved(Result<(), String>),
    Exported(Result<(PathBuf, usize), String>),
    Review(Review),
    Trashed(Vec<String>),
    Fatal(String),
}
#[derive(Clone)]
struct History {
    before: Vec<Change>,
    after: Vec<Change>,
}
struct Pending {
    history: History,
    kind: u8,
}

pub struct App {
    catalog: Option<Catalog>,
    changes: HashMap<String, Change>,
    filter: Filter,
    visible: Vec<usize>,
    selected: HashSet<usize>,
    active: Option<usize>,
    anchor: Option<usize>,
    loupe: bool,
    zoom: bool,
    cache: ImageCache,
    logo: egui::TextureHandle,
    tx: Sender<Command>,
    rx: Receiver<Event>,
    loading: bool,
    busy: bool,
    pending: Option<Pending>,
    undo: Vec<History>,
    redo: Vec<History>,
    catalog_path: String,
    root_override: String,
    open_dialog: bool,
    export_path: String,
    export_dialog: bool,
    review: Option<Review>,
    help: bool,
    message: Option<String>,
    status: String,
    thumbnail: f32,
    scroll_to_active: bool,
    grid_columns: usize,
    data_dir: PathBuf,
    save_available: bool,
    smoke: Option<PathBuf>,
    demo: Option<crate::demo::ScrollDemo>,
    smoke_stage: u8,
    smoke_started: Instant,
    smoke_collection_started: Option<Instant>,
    smoke_smart_expected: Option<(usize, Vec<usize>)>,
    screenshot_name: String,
    fullscreen: bool,
    folders: Vec<String>,
    expanded_collections: HashSet<i64>,
    counts: [usize; 3],
    pan: Vec2,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<PathBuf>,
        root: Option<PathBuf>,
        data: PathBuf,
        smoke: Option<PathBuf>,
        demo: Option<crate::demo::ScrollDemo>,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(25, 28, 32);
        style.visuals.window_fill = Color32::from_rgb(32, 36, 41);
        style.visuals.extreme_bg_color = Color32::from_rgb(18, 21, 25);
        style.visuals.selection.bg_fill = Color32::from_rgb(90, 73, 48);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, GOLD);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(107, 83, 48);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(63, 67, 72);
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(10.0, 7.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        ctx.set_style(style);
        let (tx, commands) = unbounded();
        let (events, rx) = unbounded();
        let worker_data = data.clone();
        let worker_ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut store = match Store::open(&worker_data) {
                Ok(s) => s,
                Err(e) => {
                    let _ = events.send(Event::Fatal(format!(
                        "Cannot open selection storage: {e:#}"
                    )));
                    worker_ctx.request_repaint();
                    return;
                }
            };
            let _ = events.send(Event::Recent(
                store.setting("recent_catalog"),
                store.unfinished_trash().unwrap_or(0),
            ));
            worker_ctx.request_repaint();
            while let Ok(command) = commands.recv() {
                let event = match command {
                    Command::Load(path, root) => Event::Loaded(
                        (|| -> anyhow::Result<_> {
                            let mut c = catalog::load(&path, root.as_deref())?;
                            let mut changes = store.changes(&c.key)?;
                            let mut reconciled = Vec::new();
                            for p in &mut c.photos {
                                if let Some(change) = changes.get_mut(&p.uuid) {
                                    if change.rating == Some(p.source_rating) {
                                        change.rating = None;
                                    }
                                    if change.pick == Some(p.source_pick) {
                                        change.pick = None;
                                    }
                                    if change.rating.is_none() {
                                        change.base_rating = p.source_rating;
                                    }
                                    if change.pick.is_none() {
                                        change.base_pick = p.source_pick;
                                    }
                                    change.apply(p);
                                    reconciled.push(change.clone());
                                }
                            }
                            store.save(&c.key, &reconciled)?;
                            changes.retain(|_, c| !c.empty());
                            if !changes.is_empty() {
                                c.collections
                                    .refresh_smart(&c.photos, lighttable::smart::today());
                            }
                            store.set_setting("recent_catalog", &c.path.to_string_lossy())?;
                            Ok((Box::new(c), changes))
                        })()
                        .map_err(|e| format!("{e:#}")),
                    ),
                    Command::Save(key, changes) => {
                        Event::Saved(store.save(&key, &changes).map_err(|e| format!("{e:#}")))
                    }
                    Command::Export(key, path) => Event::Exported(
                        (|| -> anyhow::Result<_> {
                            let count = store::export(&path, &key, &store.changes(&key)?)?;
                            Ok((path, count))
                        })()
                        .map_err(|e| format!("{e:#}")),
                    ),
                    Command::Review(photos, selected) => {
                        Event::Review(removal::prepare(&photos, &selected))
                    }
                    Command::Trash(key, files) => {
                        Event::Trashed(removal::execute(&store, &key, &files))
                    }
                };
                if events.send(event).is_err() {
                    break;
                }
                worker_ctx.request_repaint();
            }
        });
        let initial = path.is_some();
        let catalog_path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut app = Self {
            catalog: None,
            changes: HashMap::new(),
            filter: Filter::default(),
            visible: Vec::new(),
            selected: HashSet::new(),
            active: None,
            anchor: None,
            loupe: false,
            zoom: false,
            cache: ImageCache::new(ctx.clone()),
            logo: crate::branding::load_mark(ctx),
            tx,
            rx,
            loading: false,
            busy: false,
            pending: None,
            undo: Vec::new(),
            redo: Vec::new(),
            catalog_path,
            root_override: root
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            open_dialog: false,
            export_path: String::new(),
            export_dialog: false,
            review: None,
            help: false,
            message: None,
            status: "Ready".into(),
            thumbnail: 210.0,
            scroll_to_active: false,
            grid_columns: 4,
            data_dir: data,
            save_available: true,
            smoke,
            demo,
            smoke_stage: 0,
            smoke_started: Instant::now(),
            smoke_collection_started: None,
            smoke_smart_expected: None,
            screenshot_name: String::new(),
            fullscreen: false,
            folders: Vec::new(),
            expanded_collections: HashSet::new(),
            counts: [0; 3],
            pan: Vec2::ZERO,
        };
        if initial {
            app.open();
        }
        app
    }
    fn open(&mut self) {
        if self.pending.is_some() || self.busy || self.loading {
            return;
        }
        let path = PathBuf::from(self.catalog_path.trim());
        if path.as_os_str().is_empty() {
            return;
        }
        let root = (!self.root_override.trim().is_empty())
            .then(|| PathBuf::from(self.root_override.trim()));
        self.loading = true;
        self.open_dialog = false;
        self.status = "Opening catalog snapshot…".into();
        if self.tx.send(Command::Load(path, root)).is_err() {
            self.loading = false;
            self.message = Some("Storage worker is unavailable. Restart the app.".into());
        }
    }
    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Recent(path, unfinished) => {
                    if self.catalog_path.is_empty()
                        && let Some(path) = path
                    {
                        self.catalog_path = path;
                        self.open();
                    }
                    if unfinished > 0 {
                        self.message = Some(format!(
                            "{unfinished} file operations were interrupted. Check the original paths and your desktop Trash before retrying. Details are in selections.sqlite / file_operations."
                        ));
                    }
                }
                Event::Loaded(result) => {
                    self.loading = false;
                    match result {
                        Ok((catalog, changes)) => {
                            self.status = format!(
                                "{} photos · opened in {} ms",
                                catalog.photos.len(),
                                catalog.load_ms
                            );
                            if !catalog.warnings.is_empty() {
                                self.message = Some(catalog.warnings.join("\n"));
                            }
                            self.catalog_path = catalog.path.to_string_lossy().into_owned();
                            self.folders = catalog
                                .photos
                                .iter()
                                .map(|p| p.folder.clone())
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect();
                            self.expanded_collections.clear();
                            self.catalog = Some(*catalog);
                            self.changes = changes;
                            self.filter = Filter::default();
                            self.selected.clear();
                            self.active = None;
                            self.undo.clear();
                            self.redo.clear();
                            self.cache.cancel(true);
                            self.refilter();
                            self.select_position(0, false, false);
                        }
                        Err(e) => self.message = Some(format!("Could not open catalog\n\n{e}")),
                    }
                }
                Event::Saved(result) => {
                    if let Some(pending) = self.pending.take() {
                        match result {
                            Ok(()) => {
                                self.status = "Selections saved locally".into();
                                match pending.kind {
                                    0 => {
                                        self.undo.push(pending.history);
                                        self.redo.clear();
                                    }
                                    1 => self.redo.push(History {
                                        before: pending.history.after,
                                        after: pending.history.before,
                                    }),
                                    _ => self.undo.push(pending.history),
                                }
                            }
                            Err(e) => {
                                self.apply_values(&pending.history.before);
                                match pending.kind {
                                    1 => self.undo.push(History {
                                        before: pending.history.after,
                                        after: pending.history.before,
                                    }),
                                    2 => self.redo.push(pending.history),
                                    _ => {}
                                }
                                self.message = Some(format!(
                                    "Save failed. The change was rolled back.\n\n{e}"
                                ));
                            }
                        }
                    }
                }
                Event::Exported(result) => {
                    self.busy = false;
                    match result {
                        Ok((path, n)) => {
                            self.status = format!("Exported {n} changed photos");
                            self.message = Some(format!(
                                "Exported {n} changed photos to:\n{}\n\nIn Lightroom Classic, use the Lighttable companion's Import selections command. The companion still needs validation in your Lightroom version.",
                                path.display()
                            ));
                        }
                        Err(e) => self.message = Some(e),
                    }
                }
                Event::Review(review) => {
                    self.busy = false;
                    self.review = Some(review);
                }
                Event::Trashed(results) => {
                    self.busy = false;
                    self.message = Some(format!(
                        "{}\n\nRestore moved files from your desktop Trash. Lightroom catalog entries remain; remove missing/rejected entries in Lightroom after reviewing them.",
                        results.join("\n")
                    ));
                    self.status = "File operation finished; review results".into();
                }
                Event::Fatal(e) => {
                    self.save_available = false;
                    self.loading = false;
                    self.message = Some(e);
                }
            }
        }
    }
    fn refilter(&mut self) {
        let previous_active = self.active;
        if let Some(c) = &self.catalog {
            self.visible = c.filtered(&self.filter);
            self.counts = [
                c.photos.len(),
                c.photos.iter().filter(|p| p.pick == 1).count(),
                c.photos.iter().filter(|p| p.pick == -1).count(),
            ];
        }
        let visible: HashSet<_> = self.visible.iter().copied().collect();
        self.selected.retain(|i| visible.contains(i));
        if self.active.is_none_or(|i| !visible.contains(&i)) {
            self.active = self.visible.first().copied();
        }
        if self.anchor.is_some_and(|i| !visible.contains(&i)) {
            self.anchor = self.active;
        }
        if self.selected.is_empty()
            && let Some(i) = self.active
        {
            self.selected.insert(i);
        }
        if self.active != previous_active {
            self.scroll_to_active = true;
            self.cache.cancel(false);
        }
    }
    fn select_position(&mut self, position: usize, control: bool, shift: bool) {
        if let Some(&index) = self.visible.get(position) {
            if shift {
                let anchor = self
                    .anchor
                    .and_then(|i| self.visible.iter().position(|&x| x == i))
                    .unwrap_or(position);
                if !control {
                    self.selected.clear();
                }
                self.selected.extend(
                    self.visible[anchor.min(position)..=anchor.max(position)]
                        .iter()
                        .copied(),
                );
            } else if control {
                if !self.selected.insert(index) {
                    self.selected.remove(&index);
                }
                self.anchor = Some(index);
            } else {
                self.selected.clear();
                self.selected.insert(index);
                self.anchor = Some(index);
            }
            if self.active != Some(index) {
                self.pan = Vec2::ZERO;
            }
            self.active = Some(index);
        }
    }
    fn navigate(&mut self, delta: isize, shift: bool) {
        if self.visible.is_empty() {
            return;
        }
        let current = self
            .active
            .and_then(|i| self.visible.iter().position(|&x| x == i))
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        self.select_position(next, false, shift);
        self.scroll_to_active = true;
        self.cache.cancel(false);
    }
    fn can_edit(&self) -> bool {
        self.catalog.is_some()
            && self.pending.is_none()
            && !self.busy
            && !self.loading
            && self.save_available
    }
    fn edit(&mut self, rating: Option<i8>, pick: Option<i8>) {
        if !self.can_edit() {
            return;
        }
        let c = self.catalog.as_ref().unwrap();
        let mut before = Vec::new();
        let mut after = Vec::new();
        for &i in &self.selected {
            let p = &c.photos[i];
            let old = self
                .changes
                .get(&p.uuid)
                .cloned()
                .unwrap_or_else(|| Change::for_photo(p));
            let mut new = old.clone();
            if let Some(v) = rating {
                new.base_rating = p.source_rating;
                new.rating = (v != new.base_rating).then_some(v);
            }
            if let Some(v) = pick {
                new.base_pick = p.source_pick;
                new.pick = (v != new.base_pick).then_some(v);
            }
            if old != new {
                before.push(old);
                after.push(new);
            }
        }
        if !after.is_empty() {
            self.save_history(History { before, after }, 0);
        }
    }
    fn apply_values(&mut self, values: &[Change]) {
        for change in values {
            if change.empty() {
                self.changes.remove(&change.uuid);
            } else {
                self.changes.insert(change.uuid.clone(), change.clone());
            }
        }
        if let Some(c) = &mut self.catalog {
            let values: HashMap<_, _> = values.iter().map(|v| (&v.uuid, v)).collect();
            for p in &mut c.photos {
                if let Some(change) = values.get(&p.uuid) {
                    change.apply(p);
                }
            }
            c.collections
                .refresh_smart(&c.photos, lighttable::smart::today());
        }
        self.refilter();
    }
    fn save_history(&mut self, history: History, kind: u8) {
        let key = self.catalog.as_ref().unwrap().key.clone();
        if self
            .tx
            .send(Command::Save(key, history.after.clone()))
            .is_err()
        {
            self.message = Some("Storage worker stopped; no changes were applied.".into());
            return;
        }
        self.apply_values(&history.after);
        self.pending = Some(Pending { history, kind });
    }
    fn undo(&mut self, redo: bool) {
        if !self.can_edit() {
            return;
        }
        if redo {
            if let Some(history) = self.redo.pop() {
                self.save_history(history, 2);
            }
        } else if let Some(history) = self.undo.pop() {
            self.save_history(
                History {
                    before: history.after,
                    after: history.before,
                },
                1,
            );
        }
    }
    fn prepare_review(&mut self) {
        if !self.can_edit() || self.selected.is_empty() {
            return;
        }
        let c = self.catalog.as_ref().unwrap();
        self.busy = true;
        let _ = self.tx.send(Command::Review(
            c.photos.clone(),
            self.selected.iter().copied().collect(),
        ));
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input()
            || self.open_dialog
            || self.export_dialog
            || self.review.is_some()
            || self.help
            || self.message.is_some()
        {
            return;
        }
        let (keys, modifiers) = ctx.input(|i| (i.keys_down.clone(), i.modifiers));
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        if modifiers.command && pressed(egui::Key::O) {
            self.open_dialog = true;
        }
        if modifiers.command && pressed(egui::Key::Z) {
            self.undo(modifiers.shift);
        }
        if modifiers.command && pressed(egui::Key::A) {
            self.selected.extend(&self.visible);
        }
        if !modifiers.command {
            for (key, rating) in [
                (egui::Key::Num0, 0),
                (egui::Key::Num1, 1),
                (egui::Key::Num2, 2),
                (egui::Key::Num3, 3),
                (egui::Key::Num4, 4),
                (egui::Key::Num5, 5),
            ] {
                if pressed(key) {
                    self.edit(Some(rating), None);
                }
            }
            for (key, flag) in [(egui::Key::P, 1), (egui::Key::X, -1), (egui::Key::U, 0)] {
                if pressed(key) {
                    self.edit(None, Some(flag));
                }
            }
            if pressed(egui::Key::G) {
                self.loupe = false;
                self.cache.cancel(false);
                self.scroll_to_active = true;
            }
            if pressed(egui::Key::E) || pressed(egui::Key::Enter) {
                self.loupe = true;
                self.cache.cancel(false);
            }
            if pressed(egui::Key::Escape) {
                self.loupe = false;
                self.zoom = false;
            }
            if pressed(egui::Key::ArrowRight) {
                self.navigate(1, modifiers.shift);
            }
            if pressed(egui::Key::ArrowLeft) {
                self.navigate(-1, modifiers.shift);
            }
            if pressed(egui::Key::ArrowDown) {
                self.navigate(
                    if self.loupe {
                        1
                    } else {
                        self.grid_columns as isize
                    },
                    modifiers.shift,
                );
            }
            if pressed(egui::Key::ArrowUp) {
                self.navigate(
                    if self.loupe {
                        -1
                    } else {
                        -(self.grid_columns as isize)
                    },
                    modifiers.shift,
                );
            }
            if pressed(egui::Key::Delete) {
                self.prepare_review();
            }
            if pressed(egui::Key::F) {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
        }
        if !keys.is_empty() {
            ctx.request_repaint();
        }
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header").frame(egui::Frame::new().fill(Color32::from_rgb(20,23,27)).inner_margin(18)).show(ctx,|ui|{
            ui.horizontal(|ui|{
                ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(32.0)));
                ui.label(RichText::new("LIGHTTABLE").size(22.0).strong());
                ui.label(RichText::new("LINUX PREVIEW").size(10.0).color(MUTED));
                ui.add_space(18.0);
                if ui.add_enabled(!self.loading && !self.busy && self.pending.is_none(),egui::Button::new("Open catalog…")).clicked(){self.open_dialog=true;}
                if self.catalog.is_some() && ui.add_enabled(self.can_edit(),egui::Button::new("Refresh")).on_hover_text("Reload a closed Lightroom catalog and reconcile imported selections").clicked(){self.open();}
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if ui.button("Shortcuts").clicked(){self.help=true;}
                    if ui.add_enabled(self.can_edit() && !self.changes.is_empty(),egui::Button::new(RichText::new("Export to Lightroom…").color(GOLD))).clicked(){
                        let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                        self.export_path=self.data_dir.join(format!("lighttable-{now}.tsv")).to_string_lossy().into_owned();self.export_dialog=true;
                    }
                    if self.pending.is_some(){ui.spinner();ui.label("Saving…");}
                    else {ui.label(RichText::new(format!("{} pending",self.changes.len())).color(MUTED));}
                });
            });
        });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .default_width(248.0)
            .width_range(208.0..=420.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(25, 28, 32))
                    .inner_margin(18),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("LIBRARY").size(11.0).color(MUTED));
                ui.add_space(7.0);
                if let Some(c) = &self.catalog {
                    let title = c
                        .path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    ui.label(RichText::new(title).strong());
                    ui.add_space(12.0);
                    let counts = self.counts;
                    for (label, pick, count) in [
                        ("All photographs", None, counts[0]),
                        ("Picked", Some(1), counts[1]),
                        ("Rejected", Some(-1), counts[2]),
                    ] {
                        let active = self.filter.pick == pick
                            && self.filter.folder.is_empty()
                            && self.filter.collection.is_none();
                        if ui
                            .add_sized(
                                [ui.available_width(), 32.0],
                                egui::Button::new(format!("{label}    {count}")).selected(active),
                            )
                            .clicked()
                        {
                            self.filter.pick = pick;
                            self.filter.folder.clear();
                            self.filter.collection = None;
                        }
                    }
                    ui.add_space(16.0);
                    ui.label(RichText::new("COLLECTIONS").size(11.0).color(MUTED));
                    let smart_count = c.collections.nodes.iter().filter(|n| n.kind == lighttable::collections::Kind::Smart).count();
                    let smart_supported = c.collections.nodes.iter().filter(|n| n.kind == lighttable::collections::Kind::Smart && n.can_browse()).count();
                    ui.label(
                        RichText::new(format!("Smart: {smart_supported} of {smart_count} supported"))
                            .size(10.0)
                            .color(MUTED),
                    ).on_hover_text("Date and rating Smart rules are evaluated in Lighttable. Hover a collection for its rule or unsupported reason.");
                    if c.collections.nodes.is_empty() {
                        ui.label(
                            RichText::new("No collections available")
                                .size(12.0)
                                .color(MUTED),
                        );
                    } else {
                        let height = ((ui.available_height() - 100.0) * 0.65).max(60.0);
                        if let Some(id) = crate::sidebar::collection_tree(
                            ui,
                            &c.collections,
                            &mut self.expanded_collections,
                            self.filter.collection,
                            height,
                            &c.key,
                        ) {
                            self.filter.collection = Some(id);
                            self.filter.folder.clear();
                        }
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new("FOLDERS").size(11.0).color(MUTED));
                    egui::ScrollArea::vertical()
                        .id_salt("folders")
                        .max_height((ui.available_height() - 65.0).max(30.0))
                        .show_rows(ui, 24.0, self.folders.len(), |ui, range| {
                            ui.spacing_mut().button_padding.y = 4.0;
                            for index in range {
                                let folder = &self.folders[index];
                                let label = folder
                                    .trim_end_matches('/')
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(folder);
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 24.0],
                                        egui::Button::selectable(
                                            self.filter.collection.is_none()
                                                && self.filter.folder == *folder,
                                            label,
                                        )
                                        .right_text("")
                                        .truncate(),
                                    )
                                    .on_hover_text(folder)
                                    .clicked()
                                {
                                    self.filter.folder = folder.clone();
                                    self.filter.collection = None;
                                }
                            }
                        });
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        ui.label(
                            RichText::new("Original catalog stays intact")
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.label(RichText::new("LOCAL SELECTIONS").size(10.0).color(GOLD));
                    });
                } else {
                    ui.label("Open a Lightroom Classic catalog to begin.");
                }
            });
    }
    fn details(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("details")
            .exact_width(267.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(25, 28, 32))
                    .inner_margin(18),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("PHOTO DETAILS").size(11.0).color(MUTED));
                ui.add_space(10.0);
                let p = self
                    .active
                    .and_then(|i| self.catalog.as_ref().map(|c| c.photos[i].clone()));
                if let Some(p) = p {
                    ui.label(RichText::new(&p.filename).size(19.0).strong());
                    if p.virtual_copy {
                        ui.label(format!("Virtual copy · {}", p.copy_name));
                    }
                    ui.label(
                        RichText::new(format!("{} · {} × {}", p.format, p.width, p.height))
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        for star in 1..=5 {
                            if ui
                                .add_enabled(
                                    self.can_edit(),
                                    egui::Button::new(
                                        RichText::new(if p.rating >= star { "★" } else { "☆" })
                                            .size(24.0)
                                            .color(GOLD),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.edit(Some(star), None);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        for (label, flag) in [("Pick", 1), ("Unflag", 0), ("Reject", -1)] {
                            if ui
                                .add_enabled(
                                    self.can_edit(),
                                    egui::Button::new(label).selected(p.pick == flag),
                                )
                                .clicked()
                            {
                                self.edit(None, Some(flag));
                            }
                        }
                    });
                    if self.selected.len() > 1 {
                        ui.label(
                            RichText::new(format!(
                                "Applies to {} selected photos",
                                self.selected.len()
                            ))
                            .color(GOLD)
                            .size(11.0),
                        );
                    }
                    ui.add_space(14.0);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("metadata")
                        .show(ui, |ui| {
                            for (label, value) in [
                                ("CAPTURED", p.captured.replace('T', "  ")),
                                ("CAMERA", p.camera.clone()),
                                ("LENS", p.lens.clone()),
                                (
                                    "EXPOSURE",
                                    format!(
                                        "{}    {}    {}",
                                        p.aperture
                                            .map(|v| format!("f/{v:.1}"))
                                            .unwrap_or("—".into()),
                                        p.shutter
                                            .map(|v| if v < 1.0 {
                                                format!("1/{:.0} s", 1.0 / v)
                                            } else {
                                                format!("{v:.1} s")
                                            })
                                            .unwrap_or("—".into()),
                                        p.iso.map(|v| format!("ISO {v:.0}")).unwrap_or("—".into())
                                    ),
                                ),
                                (
                                    "FOCAL LENGTH",
                                    p.focal.map(|v| format!("{v:.0} mm")).unwrap_or_default(),
                                ),
                                ("KEYWORDS", p.keywords.clone()),
                                ("CAPTION", p.caption.clone()),
                                ("ORIGINAL PATH", p.original.to_string_lossy().into_owned()),
                            ] {
                                if !value.is_empty() {
                                    ui.add_space(12.0);
                                    ui.label(RichText::new(label).size(10.0).color(MUTED));
                                    ui.add(egui::Label::new(value).wrap());
                                }
                            }
                            ui.add_space(20.0);
                            if ui
                                .add_enabled(
                                    self.can_edit(),
                                    egui::Button::new("Review files for Trash…"),
                                )
                                .clicked()
                            {
                                self.prepare_review();
                            }
                            ui.label(
                                RichText::new("Reject flags never delete files.")
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                } else {
                    ui.label("Select a photograph.");
                }
            });
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.loupe, "Grid").clicked() {
                self.loupe = false;
                self.scroll_to_active = true;
            }
            if ui.selectable_label(self.loupe, "Loupe").clicked() {
                self.loupe = true;
                self.cache.cancel(false);
            }
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.query)
                    .hint_text("Search photos, cameras, keywords…")
                    .desired_width(225.0),
            );
            egui::ComboBox::from_id_salt("stars")
                .selected_text(if self.filter.minimum_rating == 0 {
                    "Any rating".into()
                } else {
                    format!("{}+ stars", self.filter.minimum_rating)
                })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.minimum_rating, 0, "Any rating");
                    for n in 1..=5 {
                        ui.selectable_value(
                            &mut self.filter.minimum_rating,
                            n,
                            format!("{n}+ stars"),
                        );
                    }
                });
            egui::ComboBox::from_id_salt("flags")
                .selected_text(match self.filter.pick {
                    Some(1) => "Picked",
                    Some(-1) => "Rejected",
                    Some(_) => "Unflagged",
                    None => "Any flag",
                })
                .width(80.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.pick, None, "Any flag");
                    ui.selectable_value(&mut self.filter.pick, Some(1), "Picked");
                    ui.selectable_value(&mut self.filter.pick, Some(-1), "Rejected");
                    ui.selectable_value(&mut self.filter.pick, Some(0), "Unflagged");
                });
            egui::ComboBox::from_id_salt("sort")
                .selected_text(if self.filter.sort_name {
                    "Filename"
                } else {
                    "Capture date"
                })
                .width(95.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.sort_name, false, "Capture date");
                    ui.selectable_value(&mut self.filter.sort_name, true, "Filename");
                });
        });
        ui.horizontal(|ui|{
            ui.label(RichText::new(format!("{} photographs  ·  {} selected",self.visible.len(),self.selected.len())).size(12.0).color(MUTED));
            if self.filter!=Filter::default() && ui.small_button("Clear filters").clicked(){self.filter=Filter::default();}
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if self.loupe {if ui.checkbox(&mut self.zoom,"100% preview").on_hover_text("Inspect preview pixels; drag to pan. Preview resolution may be lower than the original.").changed(){self.pan=Vec2::ZERO;}}
                else{ui.add(egui::Slider::new(&mut self.thumbnail,140.0..=300.0).show_value(false).text("Size"));}
            });
        });
        if let Some(id) = self.filter.collection
            && let Some(node) = self.catalog.as_ref().and_then(|c| c.collections.get(id))
        {
            let smart = node.kind == lighttable::collections::Kind::Smart;
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{}: {}",
                        if smart {
                            "Smart collection"
                        } else {
                            "Collection"
                        },
                        node.name
                    ))
                    .size(12.0)
                    .color(GOLD),
                )
                .truncate(),
            )
            .on_hover_text(node.rule_description().unwrap_or_else(|| node.name.clone()));
            if smart {
                ui.label(
                    RichText::new(
                        "Calculated in Lighttable · includes local ratings · experimental",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
            }
        }
        ui.add_space(8.0);
    }
    fn prepare_demo(&mut self) {
        let Some(mut demo) = self.demo.take() else {
            return;
        };
        demo.begin_frame();
        if let Some(c) = &self.catalog {
            if !demo.collection_selected {
                if let Some(path) = &demo.collection {
                    let matches: Vec<_> = c
                        .collections
                        .nodes
                        .iter()
                        .filter(|node| {
                            let mut names = vec![node.name.as_str()];
                            let mut parent = node.parent;
                            while let Some(index) = parent {
                                let ancestor = &c.collections.nodes[index];
                                names.push(ancestor.name.as_str());
                                parent = ancestor.parent;
                            }
                            names.reverse();
                            names.join("/") == *path && node.can_browse()
                        })
                        .collect();
                    if matches.len() == 1 {
                        let node = matches[0];
                        let mut parent = node.parent;
                        while let Some(index) = parent {
                            let ancestor = &c.collections.nodes[index];
                            self.expanded_collections.insert(ancestor.id);
                            parent = ancestor.parent;
                        }
                        self.filter = Filter {
                            collection: Some(node.id),
                            ..Default::default()
                        };
                        self.refilter();
                    } else {
                        demo.error = Some(format!(
                            "Demo needs exactly one browsable collection matching {path:?}"
                        ));
                    }
                }
                demo.collection_selected = true;
            }
            if demo.prepared {
                let mut ready = true;
                for stop in &mut demo.stops {
                    if stop
                        .position
                        .is_none_or(|p| self.cache.errors.contains_key(&(self.visible[p], 1920)))
                    {
                        stop.position = stop.candidates.iter().copied().find(|&p| {
                            let index = self.visible[p];
                            !self.cache.errors.contains_key(&(index, 400))
                                && !self.cache.errors.contains_key(&(index, 1920))
                        });
                    }
                    if let Some(position) = stop.position {
                        let index = self.visible[position];
                        let photo = &self.catalog.as_ref().unwrap().photos[index];
                        stop.photo_id = Some(photo.id);
                        stop.filename = Some(photo.filename.clone());
                        // Keep all five Loupe images resident under the normal cache limit.
                        ready &= self.cache.request(index, 1920, photo).is_some();
                    } else {
                        demo.error =
                            Some("No available Loupe photo in a demo stop's viewport".into());
                        ready = false;
                    }
                }
                demo.loupe_ready = ready;
                demo.ready(ready);
            }
            if demo.is_recording() && self.loupe != demo.show_loupe() {
                self.loupe = demo.show_loupe();
                if self.loupe {
                    self.select_position(demo.stops[demo.cycle()].position.unwrap(), false, false);
                    self.zoom = false;
                }
                self.scroll_to_active = false;
                self.cache.cancel(false);
            }
        }
        self.demo = Some(demo);
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        let columns = ((ui.available_width() + 12.0) / (self.thumbnail + 12.0))
            .floor()
            .max(1.0) as usize;
        self.grid_columns = columns;
        let card_width = (ui.available_width() - 12.0 * (columns - 1) as f32) / columns as f32;
        let image_height = card_width * 0.70;
        let row_height = image_height + 65.0;
        let rows = self.visible.len().div_ceil(columns);
        if let Some(demo) = &mut self.demo {
            demo.row_pitch = row_height;
            if !demo.positioned {
                if demo.start.saturating_add(columns * 40) >= self.visible.len() {
                    demo.error = Some("Demo start needs at least 40 grid rows remaining".into());
                } else {
                    demo.start = demo.start / columns * columns;
                    demo.start_offset = (demo.start / columns) as f32 * row_height;
                    demo.end_offset = (rows as f32 * row_height
                        - ui.spacing().item_spacing.y
                        - ui.available_rect_before_wrap().height())
                    .max(0.0);
                    demo.configure(
                        columns,
                        self.visible.len(),
                        ui.available_rect_before_wrap().height(),
                    );
                    self.scroll_to_active = false;
                    let index = self.visible[demo.start];
                    self.active = Some(index);
                    self.anchor = Some(index);
                    self.selected.clear();
                    self.selected.insert(index);
                    demo.positioned = true;
                }
            }
            if demo.positioned {
                // This is the same scroll state used by scrollbar scrubbing.
                let id = ui.make_persistent_id(egui::Id::new("photo-grid"));
                let mut state = egui::scroll_area::State::load(ui.ctx(), id).unwrap_or_default();
                state.offset.y = demo.current_offset();
                state.store(ui.ctx(), id);
                self.scroll_to_active = false;
            }
        }
        let target = self
            .active
            .and_then(|i| self.visible.iter().position(|&x| x == i));
        let reveal_row = if std::mem::take(&mut self.scroll_to_active) {
            target.map(|position| position / columns)
        } else {
            None
        };
        let mut loading = 0;
        let mut unavailable = 0;
        let output = crate::grid::show_rows(ui, row_height, rows, reveal_row, |ui, range| {
            for row in range {
                ui.horizontal(|ui|{
                    ui.spacing_mut().item_spacing.x=12.0;
                    for col in 0..columns {
                        let position=row*columns+col;
                        if position>=self.visible.len(){break;}
                        let index=self.visible[position];let p=&self.catalog.as_ref().unwrap().photos[index];
                        let (rect,response)=ui.allocate_exact_size(Vec2::new(card_width,row_height-10.0),Sense::click());
                        let selected=self.selected.contains(&index);
                        ui.painter().rect_filled(rect,5.0,if selected{Color32::from_rgb(47,46,42)}else{Color32::from_rgb(31,35,40)});
                        let image_rect=egui::Rect::from_min_size(rect.min+Vec2::splat(7.0),Vec2::new(card_width-14.0,image_height-10.0));
                        if let Some(texture)=self.cache.request(index,400,p) {
                            paint_texture(ui,&texture.handle,image_rect);
                        } else {
                            ui.painter().rect_filled(image_rect,3.0,Color32::from_rgb(20,23,27));
                            let failed=self.cache.errors.contains_key(&(index,400));
                            if image_rect.intersects(ui.clip_rect()) {
                                if failed { unavailable+=1; } else { loading+=1; }
                            }
                            let text=if failed{"Preview unavailable"}else{"Loading preview…"};
                            ui.painter().text(image_rect.center(),egui::Align2::CENTER_CENTER,text,FontId::proportional(12.0),MUTED);
                        }
                        let filename=if p.filename.chars().count()>28{format!("{}…",p.filename.chars().take(27).collect::<String>())}else{p.filename.clone()};
                        ui.painter().text(rect.min+Vec2::new(10.0,image_height+5.0),egui::Align2::LEFT_TOP,filename,FontId::proportional(13.0),Color32::from_gray(225));
                        ui.painter().text(rect.min+Vec2::new(10.0,image_height+26.0),egui::Align2::LEFT_TOP,format!("{}{}", "★".repeat(p.rating as usize),"☆".repeat((5-p.rating) as usize)),FontId::proportional(16.0),GOLD);
                        let flag=match p.pick {1=>"PICK",-1=>"REJECT",_=>""};
                        ui.painter().text(rect.max-Vec2::new(9.0,13.0),egui::Align2::RIGHT_BOTTOM,flag,FontId::proportional(10.0),if p.pick<0{Color32::LIGHT_RED}else{GOLD});
                        if selected{ui.painter().rect_stroke(rect,5.0,Stroke::new(1.5_f32,GOLD),egui::StrokeKind::Inside);}
                        if response.clicked(){let m=ui.input(|i|i.modifiers);self.select_position(position,m.command,m.shift);}
                        if response.double_clicked(){self.select_position(position,false,false);self.loupe=true;self.cache.cancel(false);}
                        response.on_hover_text("Click to select · Double-click for loupe · Ctrl/Shift for multiple selection");
                    }
                });
            }
        });
        if let Some(demo) = &mut self.demo {
            demo.grid_offset = output.state.offset.y;
            demo.grid_loading = loading;
            demo.grid_unavailable = unavailable;
            if demo.positioned && !demo.is_recording() {
                demo.warmed_view(loading == 0, output.inner_rect.height());
            }
        }
    }
    fn loupe(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.active else {
            return;
        };
        let p = &self.catalog.as_ref().unwrap().photos[index];
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width(),
                (ui.available_height() - 127.0).max(100.0),
            ),
            Sense::click_and_drag(),
        );
        if self.zoom && response.dragged() {
            self.pan += response.drag_delta();
        }
        let size = if self.zoom { 4096 } else { 1920 };
        let mut source = "Loading larger preview…".to_string();
        if let Some(texture) = self.cache.request(index, size, p) {
            source = texture.source.clone();
            if self.zoom {
                let target = egui::Rect::from_center_size(
                    rect.center() + self.pan,
                    texture.handle.size_vec2() / ui.ctx().pixels_per_point(),
                );
                ui.painter().with_clip_rect(rect).image(
                    texture.handle.id(),
                    target,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                paint_texture(ui, &texture.handle, rect.shrink(12.0));
            }
        } else if let Some(texture) = self.cache.request(index, 400, p) {
            paint_texture(ui, &texture.handle, rect.shrink(12.0));
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Loading photograph…",
                FontId::proportional(18.0),
                MUTED,
            );
        }
        if let Some(error) = self.cache.errors.get(&(index, size)) {
            source = error.clone();
        }
        if response.double_clicked() {
            self.loupe = false;
            self.scroll_to_active = true;
        }
        ui.horizontal(|ui| {
            if ui.button("< Previous").clicked() {
                self.navigate(-1, false);
            }
            if ui.button("Next >").clicked() {
                self.navigate(1, false);
            }
            ui.add(egui::Label::new(RichText::new(source).size(11.0).color(MUTED)).truncate());
        });
        let active_position = self.visible.iter().position(|&i| i == index).unwrap_or(0);
        // A bounded filmstrip centered on the active image avoids laying out 80k widgets.
        let start = active_position.saturating_sub(3);
        let end = (start + 8).min(self.visible.len());
        ui.horizontal(|ui| {
            for position in start..end {
                let i = self.visible[position];
                let p = &self.catalog.as_ref().unwrap().photos[i];
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(75.0, 64.0), Sense::click());
                if let Some(texture) = self.cache.request(i, 400, p) {
                    paint_texture(ui, &texture.handle, rect.shrink(3.0));
                }
                if i == index {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.5_f32, GOLD),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    self.select_position(position, false, false);
                    self.cache.cancel(false);
                }
            }
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.open_dialog {
            egui::Window::new("Open Lightroom catalog")
                .collapsible(false)
                .resizable(true)
                .default_width(650.0)
                .show(ctx, |ui| {
                    ui.label("Choose a catalog copied after closing Lightroom Classic.");
                    ui.label("Catalog (.lrcat)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.catalog_path)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(8.0);
                    ui.label("Optional replacement photo root (for a catalog with one root)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.root_override)
                            .hint_text("Leave blank to use catalog-relative paths")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.save_available && !self.loading,
                                egui::Button::new("Open"),
                            )
                            .clicked()
                        {
                            self.open();
                        }
                        if ui.button("Cancel").clicked() {
                            self.open_dialog = false;
                        }
                    });
                });
        }
        if self.export_dialog {
            egui::Window::new("Export selections to Lightroom").collapsible(false).default_width(640.0).show(ctx,|ui|{
                ui.label(format!("Export {} changed photos. Your originals and Lightroom catalog stay intact.",self.changes.len()));
                ui.label("Save changes file:");ui.add(egui::TextEdit::singleline(&mut self.export_path).desired_width(f32::INFINITY));
                ui.label("Install lightroom/Lighttable.lrplugin in Lightroom's Plug-in Manager, then choose Library → Plug-in Extras → Import Lighttable selections.");
                ui.label(RichText::new("Companion importer: experimental; validate on your test catalog first.").color(GOLD));
                ui.horizontal(|ui|{
                    if ui.button("Export").clicked()&& let Some(c)=&self.catalog{let _=self.tx.send(Command::Export(c.key.clone(),PathBuf::from(self.export_path.trim())));self.busy=true;self.export_dialog=false;}
                    if ui.button("Cancel").clicked(){self.export_dialog=false;}
                });
            });
        }
        if self.review.is_some() {
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Review files for Trash").collapsible(false).default_width(760.0).show(ctx,|ui|{
                let review=self.review.as_mut().unwrap();
                ui.label("These files will move to your desktop Trash. Catalog entries and previews remain available.");
                let total:u64=review.files.iter().filter(|f|f.selected).map(|f|f.bytes).sum();
                ui.label(RichText::new(format!("{} files selected · {:.1} MB",review.files.iter().filter(|f|f.selected).count(),total as f64/1e6)).color(GOLD));
                egui::ScrollArea::vertical().max_height(360.0).show(ui,|ui|{
                    for file in &mut review.files{ui.checkbox(&mut file.selected,file.path.to_string_lossy());}
                    for note in &review.notes{ui.colored_label(Color32::LIGHT_RED,note);}
                });
                ui.label("If Trash is unavailable, files are left in place. There is no permanent-delete fallback.");
                ui.horizontal(|ui|{
                    confirm=ui.add_enabled(review.files.iter().any(|f|f.selected),egui::Button::new("Move selected files to Trash")).clicked();
                    cancel=ui.button("Cancel").clicked();
                });
            });
            if confirm {
                let review = self.review.take().unwrap();
                if let Some(c) = &self.catalog {
                    let _ = self.tx.send(Command::Trash(c.key.clone(), review.files));
                    self.busy = true;
                }
            }
            if cancel {
                self.review = None;
            }
        }
        if self.help {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    for (key, action) in [
                        ("G / E", "Grid / loupe"),
                        ("Arrows", "Navigate photographs"),
                        ("0–5", "Clear / set star rating"),
                        ("P / X / U", "Pick / reject / unflag"),
                        ("Ctrl+A", "Select filtered photos"),
                        ("Ctrl/Shift+click", "Toggle / range selection"),
                        ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo (this session)"),
                        ("Delete", "Review selected files for Trash"),
                        ("F", "Toggle fullscreen"),
                        ("Ctrl+O", "Open a catalog"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [180.0, 20.0],
                                egui::Label::new(RichText::new(key).color(GOLD)),
                            );
                            ui.label(action);
                        });
                    }
                });
            self.help = open;
        }
        if let Some(message) = self.message.clone() {
            let mut open = true;
            egui::Window::new("Lighttable")
                .open(&mut open)
                .default_width(620.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(440.0)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(&message).wrap());
                        });
                    if ui.button("Copy details").clicked() {
                        ctx.copy_text(message.clone());
                    }
                });
            if !open {
                self.message = None;
            }
        }
    }
    fn smoke_test(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.smoke.clone() else {
            return;
        };
        if self.smoke_started.elapsed() > Duration::from_secs(45) {
            eprintln!("Smoke test timed out: {}", self.status);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let screenshots = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        for shot in screenshots {
            let pixels: Vec<u8> = shot.pixels.iter().flat_map(|p| p.to_array()).collect();
            let path = dir.join(&self.screenshot_name);
            let _ = image::save_buffer(
                &path,
                &pixels,
                shot.size[0] as u32,
                shot.size[1] as u32,
                image::ColorType::Rgba8,
            );
            eprintln!("Screenshot saved: {}", path.display());
            if self.smoke_stage == 3 {
                self.loupe = true;
                self.cache.cancel(false);
                self.smoke_stage = 4;
            } else if self.smoke_stage == 5 {
                self.loupe = false;
                self.navigate(self.visible.len().saturating_sub(2) as isize, false);
                self.smoke_stage = 6;
            } else if self.smoke_stage == 7 {
                self.navigate(1, false);
                self.smoke_stage = 8;
            } else if self.smoke_stage == 9 {
                let c = self.catalog.as_ref().unwrap();
                let target = c.collections.nodes.iter().find_map(|node| {
                    (node.kind == lighttable::collections::Kind::Collection
                        && node.parent.is_some())
                    .then(|| {
                        node.members
                            .iter()
                            .find(|&&i| !c.photos[i].previews.is_empty())
                            .map(|&photo| (node.id, photo))
                    })
                    .flatten()
                });
                if let Some((id, photo)) = target {
                    let mut parent = c.collections.get(id).unwrap().parent;
                    while let Some(i) = parent {
                        self.expanded_collections.insert(c.collections.nodes[i].id);
                        parent = c.collections.nodes[i].parent;
                    }
                    self.filter = Filter {
                        collection: Some(id),
                        ..Default::default()
                    };
                    self.refilter();
                    let position = self.visible.iter().position(|&i| i == photo).unwrap();
                    self.select_position(position, false, false);
                    self.scroll_to_active = true;
                    self.cache.cancel(false);
                    self.edit(Some(4), Some(1));
                    self.smoke_collection_started = Some(Instant::now());
                    self.smoke_stage = 10;
                } else {
                    self.finish_smoke(ctx, "no nested collection with previews to exercise");
                }
            } else if self.smoke_stage == 11 {
                let c = self.catalog.as_ref().unwrap();
                let node = c.collections.get(self.filter.collection.unwrap()).unwrap();
                assert_eq!(self.visible, node.members);
                let photo = &c.photos[self.active.unwrap()];
                assert_eq!((photo.rating, photo.pick), (4, 1));
                assert!(self.pending.is_none());
                let target = c.collections.nodes.iter().find_map(|node| {
                    let Some(Ok(rule)) = &node.smart_rule else {
                        return None;
                    };
                    node.members
                        .iter()
                        .find(|&&i| {
                            !c.photos[i].previews.is_empty()
                                && !rule.matches(
                                    0,
                                    c.photos[i].captured_date,
                                    lighttable::smart::today(),
                                )
                        })
                        .map(|&i| (node.id, i, node.members.clone()))
                });
                if let Some((id, photo, members)) = target {
                    let mut parent = c.collections.get(id).unwrap().parent;
                    while let Some(i) = parent {
                        self.expanded_collections.insert(c.collections.nodes[i].id);
                        parent = c.collections.nodes[i].parent;
                    }
                    self.filter = Filter {
                        collection: Some(id),
                        ..Default::default()
                    };
                    self.refilter();
                    let position = self.visible.iter().position(|&i| i == photo).unwrap();
                    self.select_position(position, false, false);
                    self.smoke_smart_expected = Some((photo, members));
                    self.edit(Some(0), None);
                    self.smoke_stage = 12;
                } else {
                    self.finish_smoke(ctx, "nested collection checked; no rating-sensitive Smart member with previews to exercise");
                }
            } else if self.smoke_stage == 15 {
                assert_eq!(
                    &self.visible,
                    &self.smoke_smart_expected.as_ref().unwrap().1
                );
                self.finish_smoke(ctx, "nested collection membership and rating/pick save checked; Smart membership, zero-rating removal and undo checked");
            }
        }
        match self.smoke_stage {
            0 if self.catalog.is_some() && self.cache.textures.len() >= 8 && self.can_edit() => {
                self.edit(Some(5), None);
                self.smoke_stage = 1;
            }
            1 if self.pending.is_none() => {
                self.edit(None, Some(1));
                self.smoke_stage = 2;
            }
            2 if self.pending.is_none() => {
                self.screenshot_name = "grid.png".into();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.smoke_stage = 3;
            }
            4 if self
                .active
                .is_some_and(|i| self.cache.textures.contains_key(&(i, 1920))) =>
            {
                self.screenshot_name = "loupe.png".into();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.smoke_stage = 5;
            }
            6 | 8
                if !self.scroll_to_active
                    && self
                        .active
                        .is_some_and(|i| self.cache.textures.contains_key(&(i, 400))) =>
            {
                self.screenshot_name = if self.smoke_stage == 6 {
                    "grid-bottom.png"
                } else {
                    "grid-bottom-next.png"
                }
                .into();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.smoke_stage += 1;
            }
            10 if self
                .smoke_collection_started
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(750))
                && self.pending.is_none()
                && !self.scroll_to_active
                && self
                    .active
                    .is_some_and(|i| self.cache.textures.contains_key(&(i, 400))) =>
            {
                self.screenshot_name = "collection.png".into();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.smoke_stage = 11;
            }
            12 if self.pending.is_none() => {
                let photo = self.smoke_smart_expected.as_ref().unwrap().0;
                assert!(!self.visible.contains(&photo));
                assert_eq!(self.catalog.as_ref().unwrap().photos[photo].rating, 0);
                self.undo(false);
                self.smoke_stage = 13;
            }
            13 if self.pending.is_none() => {
                let (photo, members) = self.smoke_smart_expected.as_ref().unwrap();
                assert_eq!(&self.visible, members);
                let position = self.visible.iter().position(|i| i == photo).unwrap();
                self.select_position(position, false, false);
                self.scroll_to_active = true;
                self.smoke_collection_started = Some(Instant::now());
                self.smoke_stage = 14;
            }
            14 if self
                .smoke_collection_started
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(750))
                && !self.scroll_to_active
                && self
                    .active
                    .is_some_and(|i| self.cache.textures.contains_key(&(i, 400))) =>
            {
                self.screenshot_name = "smart-collection.png".into();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.smoke_stage = 15;
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn finish_smoke(&mut self, ctx: &egui::Context, collections: &str) {
        std::fs::write(self.smoke.as_ref().unwrap().join("success.txt"), format!(
            "Loaded {} photos; decoded {}; rating and pick saved; grid, loupe and bottom-row navigation rendered; {collections}.\n",
            self.catalog.as_ref().unwrap().photos.len(), self.cache.decoded,
        )).expect("write smoke test result");
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        self.smoke_stage = 16;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        let today = lighttable::smart::today();
        if let Some(c) = &mut self.catalog
            && c.collections.evaluated_on != Some(today)
        {
            c.collections.refresh_smart(&c.photos, today);
            self.refilter();
            self.scroll_to_active = true;
            self.cache.cancel(false);
        }
        // Relative date collections also advance when the app is left idle overnight.
        if self.catalog.is_some() {
            ctx.request_repaint_after(Duration::from_secs(60));
        }
        self.cache.poll(ctx);
        self.prepare_demo();
        if let Some(demo) = &self.demo {
            self.status = demo.status();
        }
        self.shortcuts(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && (self.pending.is_some() || self.busy) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status =
                "Waiting for pending operations before closing. Close again when saved.".into();
        }
        let before = self.filter.clone();
        self.header(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 23, 27))
                    .inner_margin(9),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.loading || self.busy {
                        ui.spinner();
                    }
                    ui.label(RichText::new(&self.status).size(11.0).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "Textures {:.0} MB · {} cached",
                                self.cache.bytes as f64 / 1e6,
                                self.cache.textures.len()
                            ))
                            .size(10.0)
                            .color(MUTED),
                        );
                        if ui
                            .add_enabled(
                                self.can_edit() && !self.redo.is_empty(),
                                egui::Button::new("Redo").small(),
                            )
                            .clicked()
                        {
                            self.undo(true);
                        }
                        if ui
                            .add_enabled(
                                self.can_edit() && !self.undo.is_empty(),
                                egui::Button::new("Undo").small(),
                            )
                            .clicked()
                        {
                            self.undo(false);
                        }
                    });
                });
            });
        self.sidebar(ctx);
        self.details(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(18,21,25)).inner_margin(18)).show(ctx,|ui|{
            if self.catalog.is_some(){
                self.toolbar(ui);
                if self.filter!=before{self.refilter();self.cache.cancel(false);self.scroll_to_active=true;}
                if self.visible.is_empty(){ui.centered_and_justified(|ui|{ui.label("No photographs match these filters.");});}
                else if self.loupe{self.loupe(ui);}else{self.grid(ui);}
            }else{
                ui.vertical_centered(|ui|{
                    ui.add_space(120.0);ui.label(RichText::new("Your photographs. A lighter workspace.").size(30.0));ui.add_space(16.0);
                    ui.label("Browse Lightroom catalogs, choose keepers, and rate your photographs on Linux.");ui.add_space(25.0);
                    if self.loading{ui.spinner();ui.label("Reading a private catalog snapshot…");}
                    else if ui.button(RichText::new("Open a Lightroom catalog…").color(GOLD).size(18.0)).clicked(){self.open_dialog=true;}
                    ui.add_space(22.0);ui.label(RichText::new("Keep the catalog's Previews.lrdata folder beside the .lrcat file.").color(MUTED));
                });
            }
        });
        if ctx.input(|i| i.raw_scroll_delta.length() > 0.0) && !self.loupe {
            self.cache.cancel(false);
        }
        self.dialogs(ctx);
        self.smoke_test(ctx);
        if let Some(demo) = &mut self.demo {
            demo.update(
                ctx,
                self.catalog.as_ref().map_or(0, |c| c.photos.len()),
                self.visible.len(),
                self.cache.textures.len(),
                self.cache.errors.len(),
            );
        }
        if self.loading || self.busy || self.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

fn paint_texture(ui: &egui::Ui, texture: &egui::TextureHandle, rect: egui::Rect) {
    let dimensions = texture.size_vec2();
    let scale = (rect.width() / dimensions.x).min(rect.height() / dimensions.y);
    let target = egui::Rect::from_center_size(rect.center(), dimensions * scale);
    ui.painter().image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}
