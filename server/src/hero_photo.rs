//! The homepage photo of the featured pen: his lead photo with the backdrop taken out (the cutout) and
//! the pen stood upright beside its cap (the upright). Made in the background when he features a pen,
//! or the first time the homepage shows a pen that has none yet, so a page view never waits for it.
//! What the homepage shows, best first: the upright, the cutout, his photo as photographed.

use crate::app::State;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use thepenmarket::media::{cutout_rel, variant_rel};
use thepenmarket::upright::{make_upright_file, upright_rel};

/// Photos being worked on now, so two page views never start the same job.
static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Product ids whose upright is turned the other way up (`site_setting` "hero_upright_flip").
pub const FLIP_SETTING: &str = "hero_upright_flip";

fn file(media: &Path, rel: &str) -> Option<PathBuf> {
    let p = media.join(rel);
    p.is_file().then_some(p)
}

/// `/media/<rel>?v=<modified>`, so a remade upright is not served from the browser's old copy.
fn url(path: &Path, rel: &str) -> String {
    let v = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    format!("/media/{rel}?v={v}")
}

/// The best homepage image for a photo (`uploads/…/x.jpg`, relative to the media folder) as a URL, or
/// None for his photo as it is. Starts making what is missing.
pub async fn best(state: &State, product_id: i32, photo_rel: &str) -> Option<String> {
    if photo_rel.contains("..") {
        return None;
    }
    let media = state.cfg.media_dir.clone();
    let (up, cut) = (upright_rel(photo_rel), cutout_rel(photo_rel));
    if let Some(p) = file(&media, &up) {
        return Some(url(&p, &up));
    }
    // Nothing upright yet: make it now from the cutout (a fraction of a second), so the homepage never
    // shows the flat version; without a cutout, make that first in the background (CUTOUT_CMD).
    if file(&media, &cut).is_some() {
        if let Err(e) = make(state, product_id, photo_rel, false).await {
            tracing::warn!(photo = %photo_rel, error = %e, "homepage photo: could not stand it up");
        }
        if let Some(p) = file(&media, &up) {
            return Some(url(&p, &up));
        }
        return file(&media, &cut).map(|p| url(&p, &cut));
    }
    spawn_make(state.clone(), product_id, photo_rel.to_string(), false);
    None
}

/// Remake the upright now (after he turns it the other way up). Ok(true): there is one to show.
pub async fn remake(state: &State, product_id: i32, photo_rel: &str) -> anyhow::Result<bool> {
    if photo_rel.contains("..") {
        return Ok(false);
    }
    make(state, product_id, photo_rel, true).await?;
    Ok(state.cfg.media_dir.join(upright_rel(photo_rel)).is_file())
}

/// The lead photo of a pen, relative to the media folder.
pub async fn lead_photo(state: &State, product_id: i32) -> anyhow::Result<Option<String>> {
    Ok(crate::db::product_images(&state.pool, product_id).await?.into_iter().next().and_then(|i| i.full.strip_prefix("/media/").map(str::to_string)))
}

/// Make the cutout (when missing and a CUTOUT_CMD is set) and the upright, in the background.
/// `force` remakes the upright (after "turn it the other way up").
pub fn spawn_make(state: State, product_id: i32, photo_rel: String, force: bool) {
    {
        let mut g = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        let set = g.get_or_insert_with(HashSet::new);
        if !set.insert(photo_rel.clone()) {
            return;
        }
    }
    tokio::spawn(async move {
        if let Err(e) = make(&state, product_id, &photo_rel, force).await {
            tracing::warn!(photo = %photo_rel, error = %e, "homepage photo: could not make the cutout or upright");
        }
        if let Some(set) = BUSY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&photo_rel);
        }
    });
}

async fn make(state: &State, product_id: i32, photo_rel: &str, force: bool) -> anyhow::Result<()> {
    let media = state.cfg.media_dir.clone();
    let cut = media.join(cutout_rel(photo_rel));
    if !cut.is_file() {
        if state.cfg.cutout_cmd.is_empty() {
            return Ok(());
        }
        // the full-size photo: the homepage shows the pen large, and detail is what makes it look good
        let src = file(&media, photo_rel).or_else(|| file(&media, &variant_rel(photo_rel, 960))).ok_or_else(|| anyhow::anyhow!("photo missing"))?;
        let tmp = cut.with_extension("tmp.png");
        let run = tokio::process::Command::new(&state.cfg.cutout_cmd).arg(&src).arg(&tmp).kill_on_drop(true).output();
        let out = tokio::time::timeout(Duration::from_secs(180), run).await.map_err(|_| anyhow::anyhow!("CUTOUT_CMD took over 3 minutes"))??;
        if !out.status.success() || !tmp.is_file() {
            let _ = std::fs::remove_file(&tmp);
            anyhow::bail!("CUTOUT_CMD failed: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or(""));
        }
        std::fs::rename(&tmp, &cut)?;
    }
    let up = media.join(upright_rel(photo_rel));
    let none = media.join(format!("{}.none", upright_rel(photo_rel)));
    if up.is_file() && !force {
        return Ok(());
    }
    let flip = flipped(state, product_id).await;
    let (c, u) = (cut.clone(), up.clone());
    let made = tokio::task::spawn_blocking(move || make_upright_file(&c, &u, flip)).await??;
    if made {
        let _ = std::fs::remove_file(&none);
    } else {
        std::fs::write(&none, b"not pen-shaped: the homepage shows the cutout as photographed\n")?;
    }
    Ok(())
}

pub async fn flipped(state: &State, product_id: i32) -> bool {
    crate::settings::get(&state.pool, FLIP_SETTING)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().map(|a| a.iter().any(|x| x.as_i64() == Some(product_id as i64))))
        .unwrap_or(false)
}

pub enum Stored {
    Done,
    StillHasBackground,
    NoPhoto,
    Unreadable,
}

/// Store a cut-out of a pen's lead photo (his upload, or the helper on his Mac) and stand it up again.
pub async fn store_cutout(state: &State, product_id: i32, bytes: Vec<u8>) -> anyhow::Result<Stored> {
    let Some(rel) = lead_photo(state, product_id).await? else {
        return Ok(Stored::NoPhoto);
    };
    let media = state.cfg.media_dir.clone();
    let dest = media.join(cutout_rel(&rel));
    match thepenmarket::media::save_cutout_upload_async(bytes, dest.clone()).await {
        Ok(thepenmarket::media::CutoutUpload::Saved { .. }) => {}
        Ok(thepenmarket::media::CutoutUpload::StillHasBackground) => return Ok(Stored::StillHasBackground),
        Err(_) => return Ok(Stored::Unreadable),
    }
    let _ = std::fs::remove_file(media.join(format!("{}.none", cutout_rel(&rel))));
    let _ = std::fs::remove_file(media.join(format!("{}.none", upright_rel(&rel))));
    if let Err(e) = remake(state, product_id, &rel).await {
        tracing::warn!("homepage photo remake failed: {e:#}");
    }
    Ok(Stored::Done)
}
