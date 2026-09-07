use eframe::egui::{self, scroll_area::ScrollAreaOutput};
use std::ops::Range;

/// Virtualized photo rows, optionally revealing the row selected by navigation.
pub fn show_rows<R>(
    ui: &mut egui::Ui,
    row_pitch: f32,
    total_rows: usize,
    reveal_row: Option<usize>,
    add_rows: impl FnOnce(&mut egui::Ui, Range<usize>) -> R,
) -> ScrollAreaOutput<R> {
    let spacing = ui.spacing().item_spacing.y;
    let row_height = row_pitch - spacing;
    let viewport_height = ui.available_rect_before_wrap().height().max(0.0);
    let content_height = (row_pitch * total_rows as f32 - spacing).max(0.0);
    let max_offset = (content_height - viewport_height).max(0.0);
    let id = ui.make_persistent_id(egui::Id::new("photo-grid"));
    let mut offset = egui::scroll_area::State::load(ui.ctx(), id)
        .unwrap_or_default()
        .offset
        .y
        .clamp(0.0, max_offset);

    if let Some(row) = reveal_row.filter(|&row| row < total_rows) {
        let top = row as f32 * row_pitch;
        // For cards taller than the viewport, reveal their top and keep that
        // position when navigating horizontally within the same row.
        let bottom = top + row_height.min(viewport_height);
        if top < offset {
            offset = top;
        } else if bottom > offset + viewport_height {
            offset = bottom - viewport_height;
        }
    }

    // egui clamps offsets after painting. Clamp before layout as well, so a
    // bottom-row reveal or a shrinking filter cannot paint an overscrolled frame.
    let scroll = egui::ScrollArea::vertical()
        .id_salt("photo-grid")
        .auto_shrink([false, false])
        .min_scrolled_height(0.0)
        .vertical_scroll_offset(offset.clamp(0.0, max_offset));
    scroll.show_rows(ui, row_height, total_rows, add_rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Frame {
        viewport: egui::Rect,
        rows: Vec<(usize, egui::Rect)>,
        offset: f32,
        content_height: f32,
    }

    impl Frame {
        fn row(&self, index: usize) -> egui::Rect {
            self.rows.iter().find(|(i, _)| *i == index).unwrap().1
        }
    }

    // Use real egui layout across frames: an offset can be clamped in the saved
    // state yet still have painted an overscrolled frame before that clamp.
    fn frame(ctx: &egui::Context, total_rows: usize, reveal_row: Option<usize>) -> Frame {
        frame_at_height(ctx, total_rows, reveal_row, 600.0)
    }

    fn frame_at_height(
        ctx: &egui::Context,
        total_rows: usize,
        reveal_row: Option<usize>,
        height: f32,
    ) -> Frame {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, height),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.add_space(35.0);
                    let output = show_rows(ui, 180.0, total_rows, reveal_row, |ui, range| {
                        range
                            .map(|row| {
                                let rect = ui
                                    .horizontal(|ui| {
                                        ui.allocate_exact_size(
                                            egui::vec2(100.0, 170.0),
                                            egui::Sense::click(),
                                        )
                                        .0
                                    })
                                    .inner;
                                (row, rect)
                            })
                            .collect()
                    });
                    result = Some(Frame {
                        viewport: output.inner_rect,
                        rows: output.inner,
                        offset: output.state.offset.y,
                        content_height: output.content_size.y,
                    });
                });
            },
        );
        result.unwrap()
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1.0, "expected {a} to equal {b}");
    }

    #[test]
    fn navigation_within_visible_lower_rows_keeps_grid_still() {
        let ctx = egui::Context::default();
        let initial = frame(&ctx, 8, None);
        for row in [1, 1, 2, 2, 1, 0] {
            let navigated = frame(&ctx, 8, Some(row));
            close(navigated.offset, 0.0);
            close(navigated.row(row).top(), initial.row(row).top());
            let next = frame(&ctx, 8, None);
            close(navigated.row(row).top(), next.row(row).top());
        }
    }

    #[test]
    fn offscreen_row_scrolls_only_enough_to_reveal_it() {
        let ctx = egui::Context::default();
        frame(&ctx, 8, None);
        let revealed = frame(&ctx, 8, Some(3));
        close(revealed.row(3).bottom(), revealed.viewport.bottom());
        for target in [Some(3), None, Some(3)] {
            let next = frame(&ctx, 8, target);
            close(next.offset, revealed.offset);
            close(next.row(3).top(), revealed.row(3).top());
        }
        let back = frame(&ctx, 8, Some(0));
        close(back.offset, 0.0);
        close(back.row(0).top(), back.viewport.top());
    }

    #[test]
    fn last_row_does_not_paint_one_overscrolled_frame() {
        let ctx = egui::Context::default();
        frame(&ctx, 8, None);
        let bottom = frame(&ctx, 8, Some(7));
        close(bottom.row(7).bottom(), bottom.viewport.bottom());
        close(
            bottom.offset,
            bottom.content_height - bottom.viewport.height(),
        );
        for target in [None, Some(7), None, Some(7)] {
            let next = frame(&ctx, 8, target);
            close(next.row(7).top(), bottom.row(7).top());
        }
    }

    #[test]
    fn short_or_filtered_grid_never_overscrolls() {
        let ctx = egui::Context::default();
        frame(&ctx, 8, Some(7));
        let shortened = frame(&ctx, 2, Some(1));
        close(shortened.offset, 0.0);
        close(shortened.row(0).top(), shortened.viewport.top());
        let next = frame(&ctx, 2, None);
        close(next.row(1).top(), shortened.row(1).top());
        close(frame(&ctx, 0, None).offset, 0.0);
    }

    #[test]
    fn tall_cards_and_viewport_resize_keep_frames_stable() {
        let ctx = egui::Context::default();
        let first = frame_at_height(&ctx, 8, Some(7), 180.0);
        close(first.row(7).top(), first.viewport.top());
        for target in [None, Some(7), None, Some(7)] {
            let next = frame_at_height(&ctx, 8, target, 180.0);
            close(next.row(7).top(), first.row(7).top());
        }
        let enlarged = frame(&ctx, 8, None);
        close(enlarged.row(7).bottom(), enlarged.viewport.bottom());
        let next = frame(&ctx, 8, None);
        close(next.row(7).top(), enlarged.row(7).top());
    }
}
