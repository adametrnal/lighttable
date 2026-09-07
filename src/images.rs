use crate::model::Photo;
use anyhow::{Context, Result, bail, ensure};
use image::{
    DynamicImage, ImageDecoder, RgbaImage, codecs::jpeg::JpegDecoder, metadata::Orientation,
};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

pub struct Decoded {
    pub image: RgbaImage,
    pub source: String,
}

pub fn decode_photo(photo: &Photo, size: u32) -> Result<Decoded> {
    let mut previews: Vec<_> = photo.previews.iter().collect();
    previews.sort_by_key(|p| {
        if p.long_side >= size {
            (0, p.long_side)
        } else {
            (1, u32::MAX - p.long_side)
        }
    });
    let mut last_error = String::new();
    for p in previews {
        match decode_jpeg_file(&p.path, size, Some(&photo.orientation), p.adobe_rgb) {
            Ok(image) => {
                return Ok(Decoded {
                    image,
                    source: format!("Lightroom preview · {} px", p.long_side),
                });
            }
            Err(e) => last_error = format!("{e:#}"),
        }
    }
    for p in std::iter::once(&photo.original).chain(photo.companions.iter()) {
        if p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
            && let Ok(image) = decode_jpeg_file(p, size, None, false)
        {
            return Ok(Decoded {
                image,
                source: "Original/companion JPEG · Lightroom edits may differ".into(),
            });
        }
    }
    if photo
        .original
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dng"))
        && let Ok(bytes) = embedded_jpeg(&photo.original)
        && let Ok(image) = decode_jpeg(bytes, size, Some(&photo.orientation), false)
    {
        return Ok(Decoded {
            image,
            source: "Embedded DNG preview · Lightroom edits may differ".into(),
        });
    }
    bail!(
        "No supported preview. Connect the originals or build/copy Standard Previews in Lightroom. {last_error}"
    )
}

fn decode_jpeg_file(
    path: &Path,
    size: u32,
    orientation: Option<&str>,
    adobe_rgb: bool,
) -> Result<RgbaImage> {
    let f = File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
    ensure!(
        f.metadata()?.len() <= 64 * 1024 * 1024,
        "JPEG exceeds the 64 MiB input limit"
    );
    let mut bytes = Vec::new();
    f.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Image grew while reading");
    decode_jpeg(bytes, size, orientation, adobe_rgb)
}

fn decode_jpeg(
    bytes: Vec<u8>,
    size: u32,
    orientation: Option<&str>,
    adobe_rgb: bool,
) -> Result<RgbaImage> {
    ensure!(
        bytes.starts_with(&[0xff, 0xd8]),
        "Unsupported preview format (expected JPEG)"
    );
    let mut decoder = JpegDecoder::new(Cursor::new(bytes))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20000);
    limits.max_image_height = Some(20000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    decoder.set_limits(limits)?;
    ensure!(
        u64::from(decoder.dimensions().0) * u64::from(decoder.dimensions().1) <= 60_000_000,
        "Image exceeds the 60 MP decoding limit"
    );
    let icc = decoder.icc_profile()?;
    let orient = match orientation {
        Some(code) => lr_orientation(code)?,
        None => decoder.orientation()?,
    };
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orient);
    let mut rgb = decoded.thumbnail(size.max(1), size.max(1)).to_rgb8();
    let profile = if let Some(data) = icc {
        Some(lcms2::Profile::new_icc(&data).context("Invalid image color profile")?)
    } else if adobe_rgb {
        Some(adobe_profile()?)
    } else {
        None
    };
    if let Some(profile) = profile {
        let transform: lcms2::Transform<u8, u8> = lcms2::Transform::new(
            &profile,
            lcms2::PixelFormat::RGB_8,
            &lcms2::Profile::new_srgb(),
            lcms2::PixelFormat::RGB_8,
            lcms2::Intent::RelativeColorimetric,
        )?;
        transform.transform_in_place(rgb.as_mut());
    }
    Ok(DynamicImage::ImageRgb8(rgb).into_rgba8())
}

fn adobe_profile() -> Result<lcms2::Profile> {
    let xy = |x, y| lcms2::CIExyY { x, y, Y: 1.0 };
    let curve = lcms2::ToneCurve::new(563.0 / 256.0);
    Ok(lcms2::Profile::new_rgb(
        &xy(0.3127, 0.3290),
        &lcms2::CIExyYTRIPLE {
            Red: xy(0.6400, 0.3300),
            Green: xy(0.2100, 0.7100),
            Blue: xy(0.1500, 0.0600),
        },
        &[&curve, &curve, &curve],
    )?)
}

pub fn lr_orientation(code: &str) -> Result<Orientation> {
    match code {
        "AB" | "" => Ok(Orientation::NoTransforms),
        "BC" => Ok(Orientation::Rotate90),
        "CD" => Ok(Orientation::Rotate180),
        "DA" => Ok(Orientation::Rotate270),
        _ => bail!(
            "Unsupported Lightroom orientation {code}; mirrored fixtures still need validation"
        ),
    }
}

// Bounded classic-TIFF IFD walk: never treat raw strips as rendered JPEG previews.
pub fn embedded_jpeg(path: &Path) -> Result<Vec<u8>> {
    let mut f = BufReader::new(File::open(path)?);
    let total = f.get_ref().metadata()?.len();
    let mut header = [0; 8];
    f.read_exact(&mut header)?;
    let little = &header[..2] == b"II";
    ensure!(little || &header[..2] == b"MM", "Not a TIFF/DNG file");
    let u16_at = |b: &[u8]| {
        if little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        }
    };
    let u32_at = |b: &[u8]| {
        if little {
            u32::from_le_bytes(b[..4].try_into().unwrap())
        } else {
            u32::from_be_bytes(b[..4].try_into().unwrap())
        }
    };
    ensure!(u16_at(&header[2..]) == 42, "BigTIFF is not supported yet");
    let mut pending = vec![u32_at(&header[4..])];
    let mut seen = std::collections::HashSet::new();
    let mut best: Option<(u32, u32, u32)> = None;
    while let Some(offset) = pending.pop() {
        ensure!(seen.len() < 32, "Too many TIFF image directories");
        if offset == 0 || !seen.insert(offset) {
            continue;
        }
        ensure!(
            u64::from(offset) + 2 <= total,
            "Invalid TIFF directory offset"
        );
        f.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut n = [0; 2];
        f.read_exact(&mut n)?;
        let n = u16_at(&n);
        ensure!(n <= 512, "TIFF directory is too large");
        let mut tags = std::collections::HashMap::new();
        for _ in 0..n {
            let mut b = [0; 12];
            f.read_exact(&mut b)?;
            let tag = u16_at(&b);
            let ty = u16_at(&b[2..]);
            let count = u32_at(&b[4..]);
            let raw = u32_at(&b[8..]);
            if count == 1 && (ty == 3 || ty == 4) {
                tags.insert(
                    tag,
                    if ty == 3 {
                        u32::from(u16_at(&b[8..]))
                    } else {
                        raw
                    },
                );
            }
            if tag == 330 && ty == 4 && count <= 16 {
                if count == 1 {
                    pending.push(raw);
                } else {
                    let back = f.stream_position()?;
                    ensure!(
                        u64::from(raw) + u64::from(count) * 4 <= total,
                        "Invalid subdirectory offsets"
                    );
                    f.seek(SeekFrom::Start(u64::from(raw)))?;
                    for _ in 0..count {
                        let mut v = [0; 4];
                        f.read_exact(&mut v)?;
                        pending.push(u32_at(&v));
                    }
                    f.seek(SeekFrom::Start(back))?;
                }
            }
        }
        let mut next = [0; 4];
        f.read_exact(&mut next)?;
        pending.push(u32_at(&next));
        if tags.get(&254).is_some_and(|v| v & 1 == 1)
            && tags.get(&259) == Some(&7)
            && let (Some(&start), Some(&len), Some(&width)) =
                (tags.get(&273), tags.get(&279), tags.get(&256))
            && u64::from(start) + u64::from(len) <= total
            && len <= 64 * 1024 * 1024
            && best.is_none_or(|(_, _, w)| width > w)
        {
            best = Some((start, len, width));
        }
    }
    let (start, len, _) = best.context("No supported embedded JPEG in this DNG")?;
    f.seek(SeekFrom::Start(u64::from(start)))?;
    let mut bytes = vec![0; len as usize];
    f.read_exact(&mut bytes)?;
    Ok(bytes)
}
