//! Optional recording harness. Drives native scroll state and captures rendered frames.
use anyhow::{Context, Result, ensure};
use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui;
use serde::Serialize;
use std::{
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub const SECONDS: f64 = 10.0;
pub const FPS: usize = 30;
pub const CYCLES: usize = 5;

#[derive(Serialize)]
pub struct DemoStop {
    pub offset: f32,
    pub candidates: Vec<usize>,
    pub position: Option<usize>,
    pub photo_id: Option<i64>,
    pub filename: Option<String>,
}

#[derive(Serialize)]
struct WarmView {
    offset: f32,
    returning: bool,
}

#[derive(Clone, Serialize)]
struct FrameRecord {
    slot: usize,
    elapsed_seconds: f64,
    loupe: bool,
    grid_offset: f32,
    grid_loading: usize,
    grid_unavailable: usize,
    loupe_ready: bool,
    photo_id: Option<i64>,
}

type Shot = (usize, Arc<egui::ColorImage>);

pub struct ScrollDemo {
    pub start: usize,
    pub collection: Option<String>,
    pub collection_selected: bool,
    pub positioned: bool,
    pub row_pitch: f32,
    pub start_offset: f32,
    pub end_offset: f32,
    pub grid_offset: f32,
    pub stops: Vec<DemoStop>,
    pub grid_loading: usize,
    pub grid_unavailable: usize,
    pub loupe_ready: bool,
    warm_offset: f32,
    returning: bool,
    warm_views: Vec<WarmView>,
    pub prepared: bool,
    output: PathBuf,
    created: Instant,
    ready_since: Option<Instant>,
    started: Option<Instant>,
    last_slot: Option<usize>,
    elapsed: f64,
    pending: usize,
    frames: Vec<FrameRecord>,
    writer: Option<Sender<Shot>>,
    done: Receiver<Result<usize, String>>,
    pub error: Option<String>,
    finished: bool,
}

impl ScrollDemo {
    pub fn new(output: PathBuf, start: usize, collection: Option<String>) -> Result<Self> {
        ensure!(!output.exists(), "Choose a new demo output directory");
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir(&output)?;
        std::fs::create_dir(output.join("captures"))?;
        let (tx, shots) = bounded::<Shot>(16);
        let (finished, done) = bounded(1);
        let folder = output.join("captures");
        std::thread::spawn(move || {
            let result = (|| -> Result<usize> {
                let mut count = 0;
                while let Ok((slot, shot)) = shots.recv() {
                    let rgb: Vec<u8> = shot
                        .pixels
                        .iter()
                        .flat_map(|p| [p.r(), p.g(), p.b()])
                        .collect();
                    let mut file = std::io::BufWriter::new(std::fs::File::create_new(
                        folder.join(format!("{slot:04}.ppm")),
                    )?);
                    write!(file, "P6\n{} {}\n255\n", shot.size[0], shot.size[1])?;
                    file.write_all(&rgb)?;
                    file.flush()?;
                    count += 1;
                }
                Ok(count)
            })();
            let _ = finished.send(result.map_err(|e| format!("{e:#}")));
        });
        Ok(Self {
            start,
            collection,
            collection_selected: false,
            positioned: false,
            row_pitch: 0.0,
            start_offset: 0.0,
            end_offset: 0.0,
            grid_offset: 0.0,
            stops: Vec::new(),
            grid_loading: 0,
            grid_unavailable: 0,
            loupe_ready: false,
            warm_offset: 0.0,
            returning: false,
            warm_views: Vec::new(),
            prepared: false,
            output,
            created: Instant::now(),
            ready_since: None,
            started: None,
            last_slot: None,
            elapsed: 0.0,
            pending: 0,
            frames: Vec::new(),
            writer: Some(tx),
            done,
            error: None,
            finished: false,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.started.is_some()
    }

    pub fn ready(&mut self, ready: bool) {
        if ready && self.started.is_none() {
            let since = self.ready_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_secs(1) {
                self.started = Some(Instant::now());
            }
        } else if !ready {
            self.ready_since = None;
        }
    }

    pub fn begin_frame(&mut self) {
        self.elapsed = self.started.map_or(0.0, |s| s.elapsed().as_secs_f64());
    }

    pub fn show_loupe(&self) -> bool {
        self.elapsed < SECONDS && self.elapsed % 2.0 >= 1.0
    }

    pub fn cycle(&self) -> usize {
        ((self.elapsed / 2.0) as usize).min(CYCLES - 1)
    }

    pub fn configure(&mut self, columns: usize, photos: usize, viewport_height: f32) {
        self.warm_offset = self.start_offset;
        for cycle in 0..CYCLES {
            let offset = self.offset((cycle * 2 + 1) as f64);
            let first = (offset / self.row_pitch).ceil() as usize * columns;
            let last = (((offset + viewport_height) / self.row_pitch).floor() as usize * columns)
                .min(photos);
            let preferred = first + columns / 2;
            let mut candidates: Vec<_> = (first..last).collect();
            candidates.sort_by_key(|&p| p.abs_diff(preferred));
            self.stops.push(DemoStop {
                offset,
                candidates,
                position: None,
                photo_id: None,
                filename: None,
            });
        }
    }

    pub fn warmed_view(&mut self, ready: bool, viewport_height: f32) {
        if !ready || self.prepared {
            return;
        }
        self.warm_views.push(WarmView {
            offset: self.grid_offset,
            returning: self.returning,
        });
        let step = (viewport_height / self.row_pitch).floor().max(1.0) * self.row_pitch;
        if self.returning {
            if self.warm_offset <= self.start_offset {
                self.prepared = true;
            } else {
                self.warm_offset = (self.warm_offset - step).max(self.start_offset);
            }
        } else if self.warm_offset >= self.end_offset {
            self.returning = true;
        } else {
            self.warm_offset = (self.warm_offset + step).min(self.end_offset);
        }
    }

    pub fn offset(&self, elapsed: f64) -> f32 {
        self.start_offset + scroll_fraction(elapsed) * (self.end_offset - self.start_offset)
    }

    pub fn current_offset(&self) -> f32 {
        if self.is_recording() {
            self.offset(self.elapsed)
        } else {
            self.warm_offset
        }
    }

    pub fn status(&self) -> String {
        match self.started {
            Some(start) => format!(
                "Scrolling demo · prepared previews · {:.1} / 10.0 s",
                start.elapsed().as_secs_f64().min(SECONDS)
            ),
            None => format!(
                "Preparing demo · {}…",
                if self.prepared {
                    "loading five Loupe photos"
                } else if self.returning {
                    "warming grid back to the top"
                } else {
                    "warming grid down to the bottom"
                }
            ),
        }
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        photos: usize,
        scope: usize,
        cached: usize,
        errors: usize,
    ) {
        if self.finished {
            return;
        }
        if let Err(error) = self.capture(ctx, photos, scope, cached, errors) {
            self.error = Some(format!("Demo stopped: {error:#}"));
        }
        if let Some(error) = &self.error {
            eprintln!("{error}");
            let _ = std::fs::write(self.output.join("ERROR.txt"), error);
            self.finished = true;
            self.writer.take();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(8));
    }

    fn capture(
        &mut self,
        ctx: &egui::Context,
        photos: usize,
        scope: usize,
        cached: usize,
        errors: usize,
    ) -> Result<()> {
        ensure!(
            self.created.elapsed() < Duration::from_secs(180),
            "Recording timed out"
        );
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
                && let Some(record) = user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<FrameRecord>())
            {
                self.writer
                    .as_ref()
                    .context("Frame arrived after recording closed")?
                    .try_send((record.slot, image))
                    .context("Frame encoder queue could not accept capture")?;
                self.frames.push(record.clone());
                self.pending -= 1;
            }
        }
        if self.started.is_some() {
            let elapsed = self.elapsed;
            let slot = (elapsed * FPS as f64).floor() as usize;
            if elapsed < SECONDS && self.last_slot != Some(slot) {
                ensure!(
                    !self.show_loupe() || self.loupe_ready,
                    "Loupe photo was not decoded before capture"
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    FrameRecord {
                        slot,
                        elapsed_seconds: elapsed,
                        loupe: self.show_loupe(),
                        grid_offset: self.grid_offset,
                        grid_loading: self.grid_loading,
                        grid_unavailable: self.grid_unavailable,
                        loupe_ready: self.show_loupe() && self.loupe_ready,
                        photo_id: self
                            .show_loupe()
                            .then(|| self.stops[self.cycle()].photo_id)
                            .flatten(),
                    },
                )));
                self.last_slot = Some(slot);
                self.pending += 1;
            } else if elapsed >= SECONDS && self.pending == 0 {
                self.writer.take();
                if let Ok(result) = self.done.try_recv() {
                    let written = result.map_err(anyhow::Error::msg)?;
                    ensure!(
                        written == self.frames.len() && written > 0,
                        "Incomplete frame output"
                    );
                    let report = serde_json::json!({
                        "catalog_photos": photos, "start_position_zero_based": self.start,
                        "collection": self.collection, "scope_photos": scope,
                        "grid_row_pitch": self.row_pitch, "start_offset": self.start_offset, "end_offset": self.end_offset,
                        "cycles": CYCLES, "scroll_seconds_per_cycle": 1.0, "loupe_seconds_per_cycle": 1.0,
                        "stops": self.stops, "warmup_views": self.warm_views,
                        "duration_seconds": SECONDS, "target_fps": FPS,
                        "captured_frames": written, "cache_condition": "Native grid warm-up from top to bottom and back, waiting for each viewport; five Loupe images predecoded; normal bounded 192 MiB texture cache; filesystem cache uncontrolled",
                        "cached_textures_at_end": cached, "preview_errors_at_end": errors,
                        "input": "Automated native grid scroll offsets; five cycles of one-second rapid downward scrubbing and one-second Loupe viewing; final Loupe closes after capture",
                        "capture": "Native OpenGL framebuffer; timestamped requests; no playback speedup",
                        "frames": self.frames,
                    });
                    std::fs::write(
                        self.output.join("recording.json"),
                        serde_json::to_vec_pretty(&report)?,
                    )?;
                    self.finished = true;
                    eprintln!("Demo captured {written} frames: {}", self.output.display());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        Ok(())
    }
}

fn scroll_fraction(elapsed: f64) -> f32 {
    let elapsed = elapsed.clamp(0.0, SECONDS);
    let cycle = (elapsed / 2.0).floor();
    // Reach each stop on the last Grid capture slot before its Loupe second.
    let progress = ((elapsed % 2.0) / (1.0 - 1.0 / FPS as f64)).min(1.0);
    ((cycle + progress) / CYCLES as f64).min(1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_downward_passes_each_pause_for_one_second() {
        let offsets: Vec<_> = (0..=1000)
            .map(|i| scroll_fraction(i as f64 / 100.0))
            .collect();
        assert!(offsets.windows(2).all(|p| p[1] >= p[0]));
        assert_eq!(offsets[0], 0.0);
        for cycle in 0..CYCLES {
            let first = cycle * 200;
            assert!(offsets[first..first + 97].windows(2).all(|p| p[1] > p[0]));
            let expected = (cycle + 1) as f32 / CYCLES as f32;
            assert!(
                offsets[first + 100..=first + 200]
                    .iter()
                    .all(|&p| p == expected)
            );
            assert_eq!(
                scroll_fraction((cycle * 60 + 29) as f64 / FPS as f64),
                expected
            );
        }
        assert_eq!(offsets[1000], 1.0);
    }
}
