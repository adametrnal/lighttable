use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui::{self, TextureHandle};
use lighttable::{images, model::Photo};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

type Key = (usize, u32);
struct Job {
    key: Key,
    photo: Photo,
    generation: u64,
}
struct Done {
    key: Key,
    result: Result<images::Decoded, String>,
    generation: u64,
}
pub struct Texture {
    pub handle: TextureHandle,
    pub source: String,
    used: u64,
    bytes: usize,
}
pub struct ImageCache {
    tx: Sender<Job>,
    rx: Receiver<Done>,
    generation: Arc<AtomicU64>,
    pub textures: HashMap<Key, Texture>,
    pub errors: HashMap<Key, String>,
    pending: HashSet<Key>,
    pub bytes: usize,
    tick: u64,
    pub decoded: usize,
}
impl ImageCache {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, jobs) = bounded::<Job>(24);
        let (done, rx) = bounded::<Done>(8);
        let generation = Arc::new(AtomicU64::new(0));
        for _ in 0..3 {
            let jobs = jobs.clone();
            let done = done.clone();
            let ctx = ctx.clone();
            let generation = generation.clone();
            std::thread::spawn(move || {
                while let Ok(job) = jobs.recv() {
                    if job.generation != generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result =
                        images::decode_photo(&job.photo, job.key.1).map_err(|e| format!("{e:#}"));
                    if job.generation == generation.load(Ordering::Relaxed) {
                        if done
                            .send(Done {
                                key: job.key,
                                result,
                                generation: job.generation,
                            })
                            .is_err()
                        {
                            break;
                        }
                        ctx.request_repaint();
                    }
                }
            });
        }
        Self {
            tx,
            rx,
            generation,
            textures: HashMap::new(),
            errors: HashMap::new(),
            pending: HashSet::new(),
            bytes: 0,
            tick: 0,
            decoded: 0,
        }
    }
    pub fn cancel(&mut self, clear: bool) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.pending.clear();
        if clear {
            self.textures.clear();
            self.errors.clear();
            self.bytes = 0;
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.tick += 1;
        let start = Instant::now();
        for _ in 0..4 {
            let Ok(done) = self.rx.try_recv() else { break };
            if done.generation != self.generation.load(Ordering::Relaxed) {
                continue;
            }
            self.pending.remove(&done.key);
            match done.result {
                Ok(decoded) => {
                    let bytes = decoded.image.as_raw().len();
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [
                            decoded.image.width() as usize,
                            decoded.image.height() as usize,
                        ],
                        decoded.image.as_raw(),
                    );
                    let handle = ctx.load_texture(
                        format!("photo-{}-{}", done.key.0, done.key.1),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.bytes += bytes;
                    self.decoded += 1;
                    self.textures.insert(
                        done.key,
                        Texture {
                            handle,
                            source: decoded.source,
                            used: self.tick,
                            bytes,
                        },
                    );
                }
                Err(error) => {
                    if self.errors.len() > 512 {
                        self.errors.clear();
                    }
                    self.errors.insert(done.key, error);
                }
            }
            if start.elapsed() > Duration::from_millis(6) {
                break;
            }
        }
        while self.bytes > 192 * 1024 * 1024 {
            let Some(key) = self
                .textures
                .iter()
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| *k)
            else {
                break;
            };
            if let Some(value) = self.textures.remove(&key) {
                self.bytes -= value.bytes;
            }
        }
        if !self.pending.is_empty() || !self.rx.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    pub fn request(&mut self, index: usize, size: u32, photo: &Photo) -> Option<&Texture> {
        let key = (index, size);
        if let Some(texture) = self.textures.get_mut(&key) {
            texture.used = self.tick;
        } else if !self.pending.contains(&key)
            && !self.errors.contains_key(&key)
            && self
                .tx
                .try_send(Job {
                    key,
                    photo: photo.clone(),
                    generation: self.generation.load(Ordering::Relaxed),
                })
                .is_ok()
        {
            self.pending.insert(key);
        }
        self.textures.get(&key)
    }
}
