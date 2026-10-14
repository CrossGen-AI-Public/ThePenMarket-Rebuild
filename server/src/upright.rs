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

/// `flip`: turn every piece that was lying down the other way up (for the rare photo where the thinner
/// end is not the nib; the admin's "Turn it the other way up").
pub fn stand_upright(img: &RgbaImage, flip: bool) -> Option<RgbaImage> {
    let (w, h) = img.dimensions();
    if w < 8 || h < 8 {
        return None;
    }
    // 1. label the solid pieces (8-connected)
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
    // 2. drop specks (under 2% of the biggest piece), then join pieces whose boxes touch (a clip or a
    //    band the mask drew apart from its cap)
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
    if keep.is_empty() || keep.len() > 4 {
        return None;
    }
    // 3. only long, thin pieces: pens, pencils, caps. A box is stubbier and fills its outline.
    // Measured on his 238 photos: lying pens are 5.5-14.5 times longer than wide, caps 3.4-4.2, and no
    // pen or cap is thicker than 18% of the longest piece; boxes and cases are 21-48%.
    let longest = keep.iter().map(|p| p.w().max(p.h())).max()? as f32;
    for p in &keep {
        let (long, short) = (p.w().max(p.h()) as f32, p.w().min(p.h()) as f32);
        let fill = p.area as f32 / (p.w() as f32 * p.h() as f32);
        if long / short < 2.2 || short > 0.2 * longest || (fill > 0.93 && long / short < 5.0) {
            return None;
        }
    }
    if keep.iter().all(|p| p.h() >= p.w()) {
        return None; // already standing
    }
    // 4. cut each piece out (with its soft edge), stand it up, nib to the top
    let mut parts: Vec<(u32, RgbaImage)> = Vec::new();
    for p in &keep {
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
    // 5. side by side, bottoms on one line
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

/// Read a cutout PNG, write the upright PNG beside it. Ok(false): not a pen-shaped photo, nothing written.
pub fn make_upright_file(cutout: &Path, out: &Path, flip: bool) -> anyhow::Result<bool> {
    let img = image::open(cutout)?.to_rgba8();
    match stand_upright(&img, flip) {
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
    fn upright_name() {
        assert_eq!(upright_rel("uploads/2026/08/6971-Pen.jpg"), "uploads/2026/08/6971-Pen-upright.png");
    }
}
