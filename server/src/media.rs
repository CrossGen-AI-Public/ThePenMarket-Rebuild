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

/// His photo with the backdrop taken out (only the pen and cap, transparent PNG), made on a Mac by
/// `scripts/make-cutouts.sh`: `uploads/x/6971-Pen.jpg` -> `uploads/x/6971-Pen-cutout.png`. It may not exist.
pub fn cutout_rel(rel: &str) -> String {
    let p = Path::new(rel);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
    let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
    if dir.is_empty() { format!("{stem}-cutout.png") } else { format!("{dir}/{stem}-cutout.png") }
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

/// Decode an uploaded image with hard limits: at most 8000 px a side and 256 MB of memory, so a small
/// file that claims to be enormous (a decompression bomb) is refused instead of filling the server.
fn decode_limited(bytes: &[u8]) -> anyhow::Result<DynamicImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8000);
    limits.max_image_height = Some(8000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?)
}

/// Image work is slow and CPU-bound: it runs on the blocking pool, two at a time, never on the
/// threads that answer page requests.
static IMAGE_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub async fn reencode_upload_async(bytes: Vec<u8>, dest: std::path::PathBuf) -> anyhow::Result<(u32, u32)> {
    let _slot = IMAGE_SLOTS.acquire().await?;
    tokio::task::spawn_blocking(move || reencode_upload(&bytes, &dest)).await?
}

pub async fn reencode_upload_with_variants_async(bytes: Vec<u8>, dest: std::path::PathBuf) -> anyhow::Result<Imported> {
    let _slot = IMAGE_SLOTS.acquire().await?;
    tokio::task::spawn_blocking(move || reencode_upload_with_variants(&bytes, &dest)).await?
}

/// Re-encode an uploaded photo (repair / sell forms) to a bounded JPEG: strips metadata, caps size.
pub fn reencode_upload(bytes: &[u8], dest: &Path) -> anyhow::Result<(u32, u32)> {
    let img = decode_limited(bytes)?;
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
    let img = decode_limited(bytes)?;
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
