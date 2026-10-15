//! The homepage pen, standing up. Input: a cutout (his photo with the backdrop taken out, transparent
//! everywhere else). Each separate piece in it (the pen, its cap, a matching pencil) is found; a piece
//! lying down is turned upright with its thinner end (the nib or the lead) at the top; the pieces are set
//! side by side on one baseline, in the order they lay in the photo. Anything that is not long and thin
//! (a box, an inkwell, a display stand) means the photo is left exactly as it was: `None`.

use image::{imageops, RgbaImage};
use std::path::Path;

/// A pixel at or above this alpha belongs to a piece; fainter ones are its soft edge.
const ON: u8 = 40;

struct Piece {
    ids: Vec<u32>,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    area: usize,
}

impl Piece {
    fn w(&self) -> u32 {
        self.x1 - self.x0 + 1
    }
    fn h(&self) -> u32 {
        self.y1 - self.y0 + 1
    }
    fn touches(&self, o: &Piece, pad: u32) -> bool {
        self.x0 <= o.x1 + pad && o.x0 <= self.x1 + pad && self.y0 <= o.y1 + pad && o.y0 <= self.y1 + pad
    }
}

/// `uploads/x/6971-Pen.jpg` -> `uploads/x/6971-Pen-upright.png` (the file the homepage shows).
pub fn upright_rel(rel: &str) -> String {
    let p = Path::new(rel);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
    let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
    if dir.is_empty() { format!("{stem}-upright.png") } else { format!("{dir}/{stem}-upright.png") }
}

/// The solid pieces of a cutout (8-connected, specks under 2% of the biggest dropped, pieces whose boxes
/// touch joined: a clip or band the mask drew apart), with the label of every pixel.
fn find_pieces(img: &RgbaImage) -> Option<(Vec<u32>, Vec<Piece>)> {
    let (w, h) = img.dimensions();
    if w < 8 || h < 8 {
        return None;
    }
    let mut label = vec![0u32; (w * h) as usize];
    let mut pieces: Vec<Piece> = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            let i = (sy * w + sx) as usize;
            if label[i] != 0 || img.get_pixel(sx, sy)[3] < ON {
                continue;
            }
            let id = pieces.len() as u32 + 1;
            label[i] = id;
            let mut p = Piece { ids: vec![id], x0: sx, y0: sy, x1: sx, y1: sy, area: 0 };
            let mut stack = vec![(sx, sy)];
            while let Some((x, y)) = stack.pop() {
                p.area += 1;
                p.x0 = p.x0.min(x);
                p.x1 = p.x1.max(x);
                p.y0 = p.y0.min(y);
                p.y1 = p.y1.max(y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let j = (ny as u32 * w + nx as u32) as usize;
                        if label[j] == 0 && img.get_pixel(nx as u32, ny as u32)[3] >= ON {
                            label[j] = id;
                            stack.push((nx as u32, ny as u32));
                        }
                    }
                }
            }
            pieces.push(p);
        }
    }
    let biggest = pieces.iter().map(|p| p.area).max()?;
    let mut keep: Vec<Piece> = pieces.into_iter().filter(|p| p.area * 50 >= biggest).collect();
    'merge: loop {
        for i in 0..keep.len() {
            for j in i + 1..keep.len() {
                if keep[i].touches(&keep[j], 3) {
                    let o = keep.remove(j);
                    let p = &mut keep[i];
                    p.ids.extend(o.ids);
                    p.x0 = p.x0.min(o.x0);
                    p.y0 = p.y0.min(o.y0);
                    p.x1 = p.x1.max(o.x1);
                    p.y1 = p.y1.max(o.y1);
                    p.area += o.area;
                    continue 'merge;
                }
            }
        }
        break;
    }
    Some((label, keep))
}

/// Long, thin and not a filled rectangle: a pen, pencil or cap. Measured on his 238 photos: lying pens
/// are 5.5-14.5 times longer than wide, caps 3.4-4.2, and no pen or cap is thicker than 18% of the
/// longest piece; boxes and cases are 21-48%.
fn pen_like(p: &Piece, longest: f32, min_aspect: f32) -> bool {
    let (long, short) = (p.w().max(p.h()) as f32, p.w().min(p.h()) as f32);
    let fill = p.area as f32 / (p.w() as f32 * p.h() as f32);
    long / short >= min_aspect && short <= 0.2 * longest && !(fill > 0.93 && long / short < 5.0)
}

/// Each piece cut out (with its soft edge), stood up with the thinner end on top (`flip` the other way),
/// set side by side in the order they lay, bottoms on one line.
fn compose(img: &RgbaImage, label: &[u32], pieces: &[&Piece], flip: bool) -> Option<RgbaImage> {
    let w = img.width();
    let mut parts: Vec<(u32, RgbaImage)> = Vec::new();
    for p in pieces {
        let mut c = RgbaImage::new(p.w(), p.h());
        for y in p.y0..=p.y1 {
            for x in p.x0..=p.x1 {
                let l = label[(y * w + x) as usize];
                let px = img.get_pixel(x, y);
                if p.ids.contains(&l) || (l == 0 && px[3] > 0) {
                    c.put_pixel(x - p.x0, y - p.y0, *px);
                }
            }
        }
        let c = if c.width() > c.height() {
            // rotate90 turns the left end to the top, rotate270 the right end
            if thinner_end_is_left(&c) != flip { imageops::rotate90(&c) } else { imageops::rotate270(&c) }
        } else {
            c
        };
        parts.push(((p.x0 + p.x1) / 2, c));
    }
    parts.sort_by_key(|(cx, _)| *cx);
    let tallest = parts.iter().map(|(_, c)| c.height()).max()?;
    let gap = (tallest / 10).max(8);
    let pad = gap / 2;
    let width = parts.iter().map(|(_, c)| c.width()).sum::<u32>() + gap * (parts.len() as u32 - 1) + 2 * pad;
    let mut out = RgbaImage::new(width, tallest + 2 * pad);
    let mut x = pad;
    for (_, c) in &parts {
        imageops::overlay(&mut out, c, x as i64, (pad + tallest - c.height()) as i64);
        x += c.width() + gap;
    }
    Some(out)
}

/// Strict: only when every piece is a pen, pencil or cap (at most four) and at least one lies down.
/// `flip`: turn every piece that was lying down the other way up (for the rare photo where the thinner
/// end is not the nib; the admin's "Turn it the other way up").
pub fn stand_upright(img: &RgbaImage, flip: bool) -> Option<RgbaImage> {
    let (label, keep) = find_pieces(img)?;
    if keep.is_empty() || keep.len() > 4 {
        return None;
    }
    let longest = keep.iter().map(|p| p.w().max(p.h())).max()? as f32;
    if !keep.iter().all(|p| pen_like(p, longest, 2.2)) || keep.iter().all(|p| p.h() >= p.w()) {
        return None;
    }
    compose(img, &label, &keep.iter().collect::<Vec<_>>(), flip)
}

/// Always upright, for the homepage (Brittany, 2026-10-14: "the pen should display vertically always"):
/// the strict result when there is one; else only the pen-like pieces of the photo stood up (a box,
/// stand or inkwell beside them is left out); else, with no pen-like piece at all, the whole cutout
/// turned so its long side stands. Then sharpened.
pub fn stand_upright_always(img: &RgbaImage, flip: bool) -> Option<RgbaImage> {
    let up = stand_upright(img, flip).or_else(|| {
        let (label, keep) = find_pieces(img)?;
        let candidates: Vec<&Piece> = keep.iter().filter(|p| p.w().max(p.h()) as f32 / p.w().min(p.h()) as f32 >= 3.0).collect();
        let longest = candidates.iter().map(|p| p.w().max(p.h())).max().unwrap_or(0) as f32;
        let pens: Vec<&Piece> = candidates.into_iter().filter(|p| pen_like(p, longest, 3.0)).collect();
        if (1..=4).contains(&pens.len()) {
            compose(img, &label, &pens, flip)
        } else if img.width() > img.height() {
            Some(imageops::rotate270(img))
        } else {
            Some(img.clone())
        }
    })?;
    Some(sharpen(&up))
}

/// A light unsharp mask on the colour only. The see-through pixels first take the colour of the pen
/// edge next to them, so the blur inside the mask does not pull a dark or light halo onto the edge.
pub fn sharpen(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut filled = img.clone();
    let mut known: Vec<bool> = img.pixels().map(|p| p[3] >= ON).collect();
    for _ in 0..4 {
        let snapshot = known.clone();
        let src = filled.clone();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if snapshot[i] {
                    continue;
                }
                let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = (ny as u32 * w + nx as u32) as usize;
                    if snapshot[j] {
                        let q = src.get_pixel(nx as u32, ny as u32);
                        r += q[0] as u32;
                        g += q[1] as u32;
                        b += q[2] as u32;
                        n += 1;
                    }
                }
                if n > 0 {
                    let a = filled.get_pixel(x, y)[3];
                    filled.put_pixel(x, y, image::Rgba([(r / n) as u8, (g / n) as u8, (b / n) as u8, a]));
                    known[i] = true;
                }
            }
        }
    }
    let mut opaque = filled.clone();
    for p in opaque.pixels_mut() {
        p[3] = 255;
    }
    let mut out = imageops::unsharpen(&opaque, 1.1, 2);
    for (o, s) in out.pixels_mut().zip(img.pixels()) {
        o[3] = s[3];
    }
    out
}

/// For a piece lying down: is its left end the thinner one? Compares how tall the piece is between 5%
/// and 15% of its length in from each end: the nib and section are much thinner than the barrel there,
/// while a blind end (even a tapered jewel) is already near full width.
fn thinner_end_is_left(c: &RgbaImage) -> bool {
    let (w, h) = c.dimensions();
    let col = |x: u32| (0..h).filter(|&y| c.get_pixel(x, y)[3] >= ON).count();
    let (a, b) = ((w as f32 * 0.05) as u32, ((w as f32 * 0.15) as u32).max(1));
    let left: usize = (a..b).map(col).sum();
    let right: usize = (w - b..w - a).map(col).sum();
    left < right
}

/// Read a cutout PNG, write the upright (always upright, sharpened) PNG beside it. Ok(false): nothing in it.
pub fn make_upright_file(cutout: &Path, out: &Path, flip: bool) -> anyhow::Result<bool> {
    let img = image::open(cutout)?.to_rgba8();
    match stand_upright_always(&img, flip) {
        Some(up) => {
            let tmp = out.with_extension("png.tmp");
            up.save_with_format(&tmp, image::ImageFormat::Png)?;
            std::fs::rename(&tmp, out)?;
            Ok(true)
        }
        None => {
            let _ = std::fs::remove_file(out); // an older upright of a photo that is no longer pen-shaped
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn bar(img: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) {
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, Rgba([20, 20, 20, 255]));
            }
        }
    }

    #[test]
    fn lying_pen_and_standing_cap_both_stand() {
        let mut img = RgbaImage::new(400, 200);
        bar(&mut img, 10, 150, 300, 170); // barrel lying down
        bar(&mut img, 300, 157, 330, 163); // thin nib at the right end
        bar(&mut img, 360, 40, 380, 170); // cap standing
        let up = stand_upright(&img, false).expect("pen-shaped");
        assert!(up.height() > up.width(), "{}x{}", up.width(), up.height());
        // the nib end went to the top: the top rows of the first piece are narrow
        let top: usize = (0..up.width() / 2).filter(|&x| up.get_pixel(x, 20)[3] > 0).count();
        let low: usize = (0..up.width() / 2).filter(|&x| up.get_pixel(x, up.height() - 20)[3] > 0).count();
        assert!(top < low, "nib not on top: {top} vs {low}");
    }

    #[test]
    fn flip_puts_the_other_end_on_top() {
        let mut img = RgbaImage::new(400, 200);
        bar(&mut img, 10, 150, 300, 170);
        bar(&mut img, 300, 157, 330, 163);
        bar(&mut img, 360, 40, 380, 170);
        let up = stand_upright(&img, true).expect("pen-shaped");
        let top: usize = (0..up.width() / 2).filter(|&x| up.get_pixel(x, 20)[3] > 0).count();
        let low: usize = (0..up.width() / 2).filter(|&x| up.get_pixel(x, up.height() - 20)[3] > 0).count();
        assert!(top > low, "flip did not turn it: {top} vs {low}");
    }

    #[test]
    fn a_box_is_left_alone() {
        let mut img = RgbaImage::new(400, 200);
        bar(&mut img, 20, 20, 380, 120);
        assert!(stand_upright(&img, false).is_none());
    }

    #[test]
    fn always_stands_the_pen_and_leaves_the_box_out() {
        let mut img = RgbaImage::new(600, 300);
        bar(&mut img, 20, 20, 380, 140); // a box
        bar(&mut img, 20, 220, 420, 240); // the pen lying below it
        bar(&mut img, 420, 227, 450, 233);
        assert!(stand_upright(&img, false).is_none(), "strict refuses a box");
        let up = stand_upright_always(&img, false).expect("always");
        assert!(up.height() > up.width(), "{}x{}", up.width(), up.height());
        assert!(up.width() < 120, "the box was left out: {} wide", up.width());
    }

    #[test]
    fn sharpen_keeps_the_see_through_parts() {
        let mut img = RgbaImage::new(40, 40);
        bar(&mut img, 10, 10, 30, 30);
        let s = sharpen(&img);
        assert_eq!(s.get_pixel(2, 2)[3], 0);
        assert_eq!(s.get_pixel(20, 20)[3], 255);
    }

    #[test]
    fn upright_name() {
        assert_eq!(upright_rel("uploads/2026/08/6971-Pen.jpg"), "uploads/2026/08/6971-Pen-upright.png");
    }
}
