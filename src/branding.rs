use eframe::egui::{self, TextureHandle};

pub fn window_icon() -> Result<egui::IconData, image::ImageError> {
    let image = image::load_from_memory(include_bytes!(
        "../assets/icons/hicolor/256x256/apps/lighttable-catalog.png"
    ))?
    .into_rgba8();
    Ok(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

pub fn load_mark(ctx: &egui::Context) -> TextureHandle {
    let image = image::load_from_memory(include_bytes!("../assets/branding/lighttable-mark.png"))
        .expect("embedded Lighttable mark must be a valid PNG")
        .into_rgba8();
    ctx.load_texture(
        "lighttable-mark",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    )
}
