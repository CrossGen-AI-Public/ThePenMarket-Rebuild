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

/// What became of a cut-out photo he uploaded (his photo with the background removed on his Mac).
#[derive(Debug, PartialEq)]
pub enum CutoutUpload {
    Saved { width: u32, height: u32 },
    /// Under 5% of the picture is see-through: the background was not taken out.
    StillHasBackground,
}

/// Check and store an uploaded cut-out: decoded with the same limits as any upload, refused when it has
/// no see-through background, trimmed to the pen with a small margin, at most 1600 px a side, written as
/// a PNG (which also drops any metadata).
pub fn save_cutout_upload(bytes: &[u8], dest: &Path) -> anyhow::Result<CutoutUpload> {
    let img = decode_limited(bytes)?.to_rgba8();
    let (w, h) = img.dimensions();
    let clear = img.pixels().filter(|p| p[3] < 250).count();
    if (clear as u64) * 20 < (w as u64) * (h as u64) {
        return Ok(CutoutUpload::StillHasBackground);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] >= 40 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return Ok(CutoutUpload::StillHasBackground); // nothing left but background
    }
    let pad = (w.max(h) / 50).max(4);
    let (cx, cy) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
    let (cw, ch) = ((x1 + pad).min(w - 1) - cx + 1, (y1 + pad).min(h - 1) - cy + 1);
    let mut out = image::imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
    if cw.max(ch) > 1600 {
        let s = 1600.0 / cw.max(ch) as f32;
        out = image::imageops::resize(&out, ((cw as f32 * s) as u32).max(1), ((ch as f32 * s) as u32).max(1), image::imageops::FilterType::Lanczos3);
    }
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("upload.tmp");
    out.save_with_format(&tmp, image::ImageFormat::Png)?;
    fs::rename(&tmp, dest)?;
    Ok(CutoutUpload::Saved { width: out.width(), height: out.height() })
}

pub async fn save_cutout_upload_async(bytes: Vec<u8>, dest: std::path::PathBuf) -> anyhow::Result<CutoutUpload> {
    let _slot = IMAGE_SLOTS.acquire().await?;
    tokio::task::spawn_blocking(move || save_cutout_upload(&bytes, &dest)).await?
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
