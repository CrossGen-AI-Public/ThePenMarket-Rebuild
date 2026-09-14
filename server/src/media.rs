//! Media import: copy an original from the backup into the served media dir and write 480 and
//! 960 pixel wide variants, re-encoded (which also drops EXIF). Pure functions over paths.

use anyhow::Context;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Imported {
    pub rel: String,
    pub width: u32,
    pub height: u32,
    pub has_480: bool,
    pub has_960: bool,
}

fn variant_path(dest: &Path, w: u32) -> PathBuf {
    let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
    let ext = dest.extension().and_then(|s| s.to_str()).unwrap_or("jpg");
    dest.with_file_name(format!("{stem}-{w}.{ext}"))
}

pub fn variant_rel(rel: &str, w: u32) -> String {
    let p = Path::new(rel);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("jpg");
    let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
    if dir.is_empty() { format!("{stem}-{w}.{ext}") } else { format!("{dir}/{stem}-{w}.{ext}") }
}

fn decode_oriented(src: &Path) -> anyhow::Result<DynamicImage> {
    let reader = ImageReader::open(src)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn write_jpeg(img: &DynamicImage, dest: &Path) -> anyhow::Result<()> {
    let mut f = fs::File::create(dest)?;
    let enc = JpegEncoder::new_with_quality(&mut f, 84);
    img.to_rgb8().write_with_encoder(enc)?;
    Ok(())
}

/// Copy `src` (an original from the backup) to `media_dir/rel`, then create the 480 and 960 variants
/// when the source is wider than each. Returns the dimensions. Skips work already done.
pub fn import_image(src: &Path, media_dir: &Path, rel: &str) -> anyhow::Result<Imported> {
    let dest = media_dir.join(rel);
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    if !dest.exists() {
        fs::copy(src, &dest).with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;
    }
    let img = decode_oriented(src).with_context(|| format!("decode {}", src.display()))?;
    let (w, h) = (img.width(), img.height());
    let is_jpeg = rel.to_lowercase().ends_with(".jpg") || rel.to_lowercase().ends_with(".jpeg");
    let mut has_480 = false;
    let mut has_960 = false;
    if is_jpeg {
        for (target, flag) in [(480u32, &mut has_480), (960u32, &mut has_960)] {
            if w > target {
                let vp = variant_path(&dest, target);
                if !vp.exists() {
                    let resized = img.resize(target, u32::MAX, image::imageops::FilterType::Lanczos3);
                    write_jpeg(&resized, &vp)?;
                }
                *flag = true;
            }
        }
    }
    Ok(Imported { rel: rel.to_string(), width: w, height: h, has_480, has_960 })
}

/// Re-encode an uploaded photo (repair / sell forms) to a bounded JPEG: strips metadata, caps size.
pub fn reencode_upload(bytes: &[u8], dest: &Path) -> anyhow::Result<(u32, u32)> {
    let img = image::load_from_memory(bytes)?;
    let img = if img.width() > 1600 { img.resize(1600, u32::MAX, image::imageops::FilterType::Lanczos3) } else { img };
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    write_jpeg(&img, dest)?;
    Ok((img.width(), img.height()))
}

/// An admin photo upload: bounded JPEG original (metadata dropped) plus the 480 and 960 variants
/// the catalog uses, so an uploaded photo serves exactly like an imported one.
pub fn reencode_upload_with_variants(bytes: &[u8], dest: &Path) -> anyhow::Result<Imported> {
    let img = image::load_from_memory(bytes)?;
    let img = if img.width() > 2400 { img.resize(2400, u32::MAX, image::imageops::FilterType::Lanczos3) } else { img };
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    write_jpeg(&img, dest)?;
    let (w, h) = (img.width(), img.height());
    let mut has_480 = false;
    let mut has_960 = false;
    for (target, flag) in [(480u32, &mut has_480), (960u32, &mut has_960)] {
        if w > target {
            let resized = img.resize(target, u32::MAX, image::imageops::FilterType::Lanczos3);
            write_jpeg(&resized, &variant_path(dest, target))?;
            *flag = true;
        }
    }
    Ok(Imported { rel: dest.to_string_lossy().to_string(), width: w, height: h, has_480, has_960 })
}
