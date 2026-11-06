//! Port of the Python prototype: the duel against one friend over 7, 30 or 365 days (`render(data, login, days)`).
//!
//! Output names: `duel-<login>` (30 days), `duel-<login>-7` and `duel-<login>-365`.
//! Two camps, the account on the left in blue and the friend on the right in red, each with
//! a portrait, a name, a power bar and a pillar chart on the same scale. The portraits are the
//! GitHub avatars: the Pillow pipeline (Lanczos, contrast, colour, 14-colour quantisation) is
//! re-implemented approximately here, so only the portrait rectangles may differ from Python.

use std::collections::HashSet;

use crate::data::{Data, Day};
use crate::export::{save, Save};
use crate::pixel::{dither_on, mix, text_width, Canvas, Font, Rgb, BAYER, BORDER};
use crate::pymath::{py_floordiv, py_imod, py_round};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 176;
// Timeline, at 100 ms a frame (10 s loop).
const LOOP: i64 = 100;
const FRAME_MS: u32 = 100;
const GROW_START: i64 = 8;
const GROW_FRAMES: i64 = 40;
const RISE: i64 = 6;
const VERDICT: i64 = 52;
const SLAM: i64 = 8;

// Layout.
const PORTRAIT: i64 = 32;
const BAR_Y: i64 = 9;
const BAR_H: i64 = 12;
const LEFT_BAR: (i64, i64) = (40, 178);
const RIGHT_BAR: (i64, i64) = (222, 360);
const CHART_BASE: i64 = 156;
const CHART_TOP: i64 = 88;
const LEFT_AREA: (i64, i64) = (8, 188);
const RIGHT_AREA: (i64, i64) = (212, 392);

/// Layout of one period, as `PERIODS` in the prototype.
#[derive(Clone, Copy)]
struct Period {
    /// Number of pillars.
    pillars: i64,
    /// Days per pillar: 1 for 7 and 30 days, 7 for a year (one pillar per week).
    group: i64,
    pitch: i64,
    width: i64,
    floor: &'static str,
}

fn period(days: i64) -> Period {
    match days {
        7 => Period { pillars: 7, group: 1, pitch: 25, width: 19, floor: "CONTRIBUTIONS IN 7 DAYS" },
        30 => Period { pillars: 30, group: 1, pitch: 6, width: 5, floor: "CONTRIBUTIONS IN 30 DAYS" },
        // A year does not fit day by day: one pillar per week, 52 weeks.
        365 => Period { pillars: 52, group: 7, pitch: 3, width: 2, floor: "WEEKLY CONTRIBUTIONS IN 12 MONTHS" },
        _ => panic!("no duel period of {days} days"),
    }
}

const OUTLINE: Rgb = [12, 10, 22];
const TEXT: Rgb = [240, 238, 250];
const MUTED: Rgb = [160, 162, 190];
const SHADOW: Rgb = [8, 6, 16];
const WHITE: Rgb = [255, 255, 255];
const GOLD: [Rgb; 4] = [[255, 248, 200], [255, 214, 72], [226, 150, 34], [130, 74, 24]];

/// Camp palette: the bar and portrait ramp (dark to highlight) and the background sky.
struct Colors {
    ramp: [Rgb; 4],
    sky: [Rgb; 3],
}

const LEFT: Colors = Colors {
    ramp: [[20, 48, 120], [40, 110, 220], [90, 190, 255], [210, 244, 255]],
    sky: [[10, 14, 40], [22, 34, 86], [40, 62, 140]],
};
const RIGHT: Colors = Colors {
    ramp: [[110, 22, 34], [210, 54, 60], [255, 122, 92], [255, 222, 200]],
    sky: [[36, 8, 22], [84, 20, 44], [140, 36, 62]],
};

// Hand-drawn portraits, both facing right (the right camp is mirrored). Used only when the
// avatar cannot be read. o outline, h/H hair, b/B headband or beard, s/S skin, w eye white,
// e pupil, m mouth, c/C top.
const FIGHTER_A: [&str; 22] = [
    "....o..o...o........",
    "...oho.oho.oho......",
    "..ohhhohhhohhhoo....",
    ".ohhHhhhHhhhhhhho...",
    ".ohhhhhhhhhhhhhhho..",
    "ohhhhhhhhhhhhhhhhho.",
    "obbbbbbbbbbbbbbbbbo.",
    "oBBBBBBBBBBBBBBBBBo.",
    "ohhssssssssssssssso.",
    "ohssooooossssoooooo.",
    "ohssswweossssswweso.",
    "ohssswweossssswweso.",
    ".osssssssssssssssSo.",
    ".osssssssssssssSsSo.",
    ".oSssssssssssssssso.",
    "..oSsssssommmmossso.",
    "..oSssssssssssssso..",
    "...oSSsssssssSSso...",
    "....ooSSSSSSSSoo....",
    "...occooooooooocco..",
    "..occccccccccccccco.",
    ".occcCccccccccCcccco",
];
const FIGHTER_B: [&str; 22] = [
    "........oooo........",
    "......oohhhhoo......",
    "....oohhhhHHhhoo....",
    "...ohhhhhhhhhhhhho..",
    "..ohhhhhHHhhhhhhhho.",
    "..ohhhhhhhhhhhhhhho.",
    "..ohhsssssssshhhhho.",
    "..ohssssssssssshhho.",
    "..osssssssssssssho..",
    "..osooooossoooooso..",
    "..osswweossswweosso.",
    "..osswweossswweosso.",
    "..osssssssssssssso..",
    "..oSsssssssssSssso..",
    "..oSsbbbbbbbbbbsSo..",
    "..oSbbbommmmobbbSo..",
    "...obbbbbbbbbbbbo...",
    "....obbbbbbbbbbo....",
    ".....oobbbbbboo.....",
    "...occoooooooocco...",
    "..occccccccccccccco.",
    ".occcCccccccccCcccco",
];
const HAIRS: [Rgb; 5] = [[40, 34, 46], [110, 64, 40], [230, 190, 90], [190, 190, 204], [200, 80, 50]];
const CROWN: [&str; 8] = [
    "o.....o.....o",
    "oo...oyo...oo",
    "oyo..oyo..oyo",
    "oyyooyyyooyyo",
    "oyyyyyyyyyyyo",
    "oyWyyyRyyyWyo",
    "oyyyyyyyyyyyo",
    "ooooooooooooo",
];

fn blit(canvas: &mut Canvas, sprite: &[&str], x: i64, y: i64, colors: &dyn Fn(u8) -> Rgb, flip: bool) {
    for (row, line) in sprite.iter().enumerate() {
        let bytes = line.as_bytes();
        for col in 0..bytes.len() {
            let key = if flip { bytes[bytes.len() - 1 - col] } else { bytes[col] };
            if key != b'.' {
                canvas.put(x + col as i64, y + row as i64, colors(key));
            }
        }
    }
}

/// `hair_for(login)`: the hair colour is picked from the sum of the login's code points.
fn hair_for(login: &str) -> Rgb {
    let sum: i64 = login.chars().map(|c| c as i64).sum();
    HAIRS[(sum % HAIRS.len() as i64) as usize]
}

fn outlined_text(canvas: &mut Canvas, x: i64, y: i64, text: &str, color: Rgb, scale: i64) {
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, 1), (1, -1), (-1, -1)] {
        canvas.text((x + dx) as f64, (y + dy) as f64, text, OUTLINE, Font::F57, scale, None);
    }
    canvas.text(x as f64, y as f64, text, color, Font::F57, scale, None);
}

/// Pillar values and total for the period: daily counts, or weekly sums over 12 months.
fn period_values(account: &Data, days: i64, period: &Period) -> (Vec<i64>, i64) {
    // Python: `year_days` when days > 90, `days` otherwise; a missing list counts as empty.
    let source: &[Day] = if days > 90 { account.year_days.as_deref().unwrap_or(&[]) } else { &account.days };
    let tail = &source[source.len().saturating_sub(days as usize)..];
    let mut counts = vec![0i64; days as usize - tail.len()];
    counts.extend(tail.iter().map(|day| day.count));
    let total = counts.iter().sum();
    if period.group == 1 {
        return (counts, total);
    }
    // Weeks counted back from yesterday; the one day left over joins the oldest week.
    let group = period.group as usize;
    let pillars = period.pillars as usize;
    let len = counts.len();
    let mut weeks: Vec<i64> = (0..pillars)
        .map(|k| counts[len - group * (k + 1)..len - group * k].iter().sum::<i64>())
        .collect();
    weeks.reverse();
    weeks[0] += counts[..len - group * pillars].iter().sum::<i64>();
    (weeks, total)
}

// Static backdrop -------------------------------------------------------------------

fn backdrop() -> Canvas {
    let mut canvas = Canvas::with_background(W, H, OUTLINE);
    let mut rng = Random::new(5);
    for y in 0..H {
        for x in 0..W {
            // Diagonal split between the two camps, leaning like a slash.
            let split = W as f64 / 2.0 + (y as f64 - H as f64 / 2.0) * 0.18;
            let camp = if (x as f64) < split { &LEFT } else { &RIGHT };
            let distance = (x as f64 - split).abs() / (W as f64 / 2.0);
            let ratio = 0.0f64.max(1.0 - distance * 1.6) * 0.9 + (1.0 - y as f64 / H as f64) * 0.15;
            let mut level = if dither_on(x, y, 1.0f64.min(ratio)) { 1 } else { 0 };
            if ratio > 0.8 && dither_on(x, y, (ratio - 0.8) * 4.0) {
                level = 2;
            }
            canvas.put(x, y, camp.sky[level]);
        }
    }
    // Speed lines streaming towards the centre.
    for _ in 0..26 {
        let y = rng.randrange(60, 150);
        let side = rng.choice(&[-1i64, 1]);
        let length = rng.randrange(20, 60);
        let start = rng.randrange(10, 120);
        for k in 0..length {
            let x = W / 2 + side * (start + k);
            if (0..W).contains(&x) && dither_on(x, y, 0.5 + 0.5 * (1.0 - k as f64 / length as f64)) {
                let color = mix(canvas.get(x, y), WHITE, 0.12);
                canvas.put(x, y, color);
            }
        }
    }
    // Stage floor: two-tone tiles with a lit front edge. Every operand is non-negative here,
    // so Rust's `/` and `%` match Python's floor division and modulo.
    for y in CHART_BASE..H {
        for x in 0..W {
            let camp = if x < W / 2 { &LEFT } else { &RIGHT };
            let tile = ((x + (y - CHART_BASE) * 2) / 16 + (y - CHART_BASE) / 4) % 2;
            let mut base = if tile != 0 { mix(camp.sky[0], OUTLINE, 0.2) } else { camp.sky[1] };
            if y == CHART_BASE {
                base = camp.ramp[1];
            } else if y == CHART_BASE + 1 {
                base = camp.sky[2];
            }
            canvas.put(x, y, base);
        }
    }
    canvas
}

// Pieces -------------------------------------------------------------------------------

fn draw_portrait(canvas: &mut Canvas, x: i64, y: i64, fighter_a: bool, colors: &Colors, hair: Rgb, flip: bool, photo: Option<&[Rgb]>) {
    let ramp = &colors.ramp;
    canvas.rect(x, y, x + PORTRAIT - 1, y + PORTRAIT - 1, OUTLINE);
    canvas.rect(x + 1, y + 1, x + PORTRAIT - 2, y + PORTRAIT - 2, ramp[2]);
    canvas.rect(x + 2, y + 2, x + PORTRAIT - 3, y + PORTRAIT - 3, colors.sky[1]);
    for yy in y + 2..y + PORTRAIT - 2 {
        for xx in x + 2..x + PORTRAIT - 2 {
            if dither_on(xx, yy, (yy - y) as f64 / PORTRAIT as f64 * 0.8) {
                canvas.put(xx, yy, colors.sky[2]);
            }
        }
    }
    match photo {
        Some(pixels) => {
            // The 28 x 28 picture pasted inside the frame, at (x + 2, y + 2).
            for py in 0..28 {
                for px in 0..28 {
                    canvas.put(x + 2 + px, y + 2 + py, pixels[(py * 28 + px) as usize]);
                }
            }
        }
        None => {
            let sprite: &[&str] = if fighter_a { &FIGHTER_A } else { &FIGHTER_B };
            let colour = |key: u8| -> Rgb {
                match key {
                    b'o' => OUTLINE,
                    b'h' => hair,
                    b'H' => mix(hair, WHITE, 0.35),
                    b'b' => if fighter_a { ramp[1] } else { mix(hair, OUTLINE, 0.15) },
                    b'B' => if fighter_a { ramp[0] } else { mix(hair, OUTLINE, 0.35) },
                    b's' => [246, 196, 150],
                    b'S' => [204, 136, 104],
                    b'w' => WHITE,
                    b'e' => OUTLINE,
                    b'm' => [150, 60, 60],
                    b'c' => ramp[1],
                    b'C' => ramp[2],
                    _ => panic!("unknown sprite key {}", key as char),
                }
            };
            // Python: x + 5 + (PORTRAIT - 30) // 2 and y + 6 + (PORTRAIT - 30).
            blit(canvas, sprite, x + 5 + py_floordiv(PORTRAIT - 30, 2), y + 6 + (PORTRAIT - 30), &colour, flip);
        }
    }
    // Corner rivets.
    for (cx, cy) in [(x + 1, y + 1), (x + PORTRAIT - 2, y + 1), (x + 1, y + PORTRAIT - 2), (x + PORTRAIT - 2, y + PORTRAIT - 2)] {
        canvas.put(cx, cy, ramp[3]);
    }
}

/// The GitHub avatar as pixel art: 28 x 28, 14 colours, light Bayer dither, tinted to its camp.
/// Approximation of the Pillow pipeline (Lanczos resize, ImageEnhance contrast and colour,
/// median-cut quantisation): this is the only part of the widget that may differ from Python.
fn pixel_avatar(path: &str, colors: &Colors) -> Option<Vec<Rgb>> {
    const SIZE: u32 = 28;
    // Path as written in data.json: next to it, else from the working directory (see data::resolve).
    // The format is read from the content, not the extension: the avatars are JPEG files named
    // `.png`, as Pillow's Image.open also handles.
    let source = image::ImageReader::open(crate::data::resolve(path)).ok()?.with_guessed_format().ok()?.decode().ok()?.to_rgb8();
    let side = source.width().min(source.height());
    let (left, top) = ((source.width() - side) / 2, (source.height() - side) / 2);
    let cropped = image::imageops::crop_imm(&source, left, top, side, side).to_image();
    let small = image::imageops::resize(&cropped, SIZE, SIZE, image::imageops::FilterType::Lanczos3);

    let mut pixels: Vec<Rgb> = small.pixels().map(|p| p.0).collect();
    // ImageEnhance.Contrast(1.15): blend with a flat grey at the mean luminance.
    let mean = pixels.iter().map(luma).sum::<i64>() as f64 / pixels.len() as f64;
    let mean = (mean + 0.5).floor();
    for p in pixels.iter_mut() {
        *p = blend_channels(p, |_| mean, 1.15);
    }
    // ImageEnhance.Color(1.2): blend with the greyscale version of each pixel.
    for p in pixels.iter_mut() {
        let grey = luma(p) as f64;
        *p = blend_channels(p, |_| grey, 1.2);
    }

    let palette: Vec<Rgb> = median_cut(&pixels, 14).into_iter().map(|c| mix(c, colors.ramp[0], 0.12)).collect();
    let mut out = vec![[0u8; 3]; (SIZE * SIZE) as usize];
    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            // Ordered dither: nudge each pixel by its Bayer threshold before picking the nearest colour.
            let nudge = (BAYER[y % 4][x % 4] as f64 / 16.0 - 0.5) * 10.0;
            let src = pixels[y * SIZE as usize + x];
            let (r, g, b) = (src[0] as f64 + nudge, src[1] as f64 + nudge, src[2] as f64 + nudge);
            // Python min(): the first colour wins a tie.
            let mut best: Option<(f64, Rgb)> = None;
            for c in &palette {
                let d = (c[0] as f64 - r).powf(2.0) + (c[1] as f64 - g).powf(2.0) + (c[2] as f64 - b).powf(2.0);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, *c));
                }
            }
            out[y * SIZE as usize + x] = best.unwrap().1;
        }
    }
    // Dark inner rim so the picture sits inside the frame like a sprite.
    let last = SIZE as usize - 1;
    for k in 0..SIZE as usize {
        for (x, y) in [(k, 0), (k, last), (0, k), (last, k)] {
            let i = y * SIZE as usize + x;
            out[i] = mix(out[i], OUTLINE, 0.6);
        }
    }
    Some(out)
}

/// Pillow's `L = (R*19595 + G*38470 + B*7471 + 0x8000) >> 16`.
fn luma(p: &Rgb) -> i64 {
    (p[0] as i64 * 19595 + p[1] as i64 * 38470 + p[2] as i64 * 7471 + 0x8000) >> 16
}

/// `Image.blend(degenerate, pixel, factor)` per channel, clipped to 0..255.
fn blend_channels(p: &Rgb, degenerate: impl Fn(usize) -> f64, factor: f64) -> Rgb {
    let mut out = [0u8; 3];
    for c in 0..3 {
        let base = degenerate(c);
        out[c] = (base + (p[c] as f64 - base) * factor + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Median cut to `colors` boxes, each box averaged. Approximates Pillow's MEDIANCUT.
fn median_cut(pixels: &[Rgb], colors: usize) -> Vec<Rgb> {
    fn span(values: &[Rgb]) -> (i64, usize) {
        let mut best = (0, 0);
        for ch in 0..3 {
            let lo = values.iter().map(|p| p[ch]).min().unwrap_or(0);
            let hi = values.iter().map(|p| p[ch]).max().unwrap_or(0);
            if (hi - lo) as i64 > best.0 {
                best = ((hi - lo) as i64, ch);
            }
        }
        best
    }
    let mut boxes: Vec<Vec<Rgb>> = vec![pixels.to_vec()];
    while boxes.len() < colors {
        // Split the box with the widest colour range; the first one wins a tie.
        let mut pick: Option<(usize, i64)> = None;
        for (i, b) in boxes.iter().enumerate() {
            let (range, _) = span(b);
            if range > 0 && pick.is_none_or(|(_, r)| range > r) {
                pick = Some((i, range));
            }
        }
        let Some((i, _)) = pick else { break };
        let mut current = boxes.remove(i);
        let (_, channel) = span(&current);
        current.sort_by_key(|p| p[channel]);
        let right = current.split_off(current.len() / 2);
        boxes.insert(i, right);
        boxes.insert(i, current);
    }
    boxes
        .iter()
        .map(|b| {
            let n = b.len() as f64;
            let mut mean = [0u8; 3];
            for (ch, slot) in mean.iter_mut().enumerate() {
                let sum: i64 = b.iter().map(|p| p[ch] as i64).sum();
                *slot = py_round(sum as f64 / n) as u8;
            }
            mean
        })
        .collect()
}

fn draw_bar(canvas: &mut Canvas, x0: i64, x1: i64, fill: f64, colors: &Colors, from_left: bool, flash: bool) {
    let ramp = &colors.ramp;
    let (y0, y1) = (BAR_Y, BAR_Y + BAR_H - 1);
    canvas.rect(x0 - 2, y0 - 2, x1 + 2, y1 + 2, OUTLINE);
    canvas.rect(x0 - 1, y0 - 1, x1 + 1, y1 + 1, GOLD[2]);
    canvas.rect(x0, y0, x1, y1, [26, 22, 40]);
    let mut x = x0;
    while x <= x1 {
        canvas.rect(x, y0, x, y1, [34, 30, 52]);
        x += 6;
    }
    let length = py_round((x1 - x0 + 1) as f64 * fill);
    if length <= 0 {
        return;
    }
    let xs = if from_left { x0..x0 + length } else { x1 - length + 1..x1 + 1 };
    for x in xs {
        for y in y0..=y1 {
            let row = y - y0;
            let mut color = if row == 0 {
                ramp[3]
            } else if row < 4 {
                ramp[2]
            } else if row < BAR_H - 3 {
                if !dither_on(x, y, 0.25) { ramp[1] } else { ramp[2] }
            } else {
                ramp[0]
            };
            if flash {
                color = mix(color, WHITE, 0.55);
            }
            canvas.put(x, y, color);
        }
    }
    // Bright leading edge.
    let edge = if from_left { x0 + length - 1 } else { x1 - length + 1 };
    for y in y0..=y1 {
        canvas.put(edge, y, ramp[3]);
    }
}

fn draw_pillar(canvas: &mut Canvas, x: i64, height: i64, colors: &Colors, glow: bool, shine: bool, width: i64) {
    let ramp = &colors.ramp;
    if height <= 0 {
        // A day without contributions: an unlit socket on the floor.
        canvas.rect(x, CHART_BASE - 2, x + width - 1, CHART_BASE - 1, OUTLINE);
        canvas.rect(x + 1, CHART_BASE - 2, x + width - 2, CHART_BASE - 2, mix(ramp[0], OUTLINE, 0.4));
        return;
    }
    let top = CHART_BASE - height;
    for y in top..CHART_BASE {
        for dx in 0..width {
            let mut color = if dx == 0 {
                ramp[2]
            } else if dx == width - 1 && width >= 4 {
                ramp[0]
            } else {
                ramp[1]
            };
            // Darker towards the floor, dithered (thin weekly pillars keep a flat colour).
            if 0 < dx && width >= 4 && dither_on(x + dx, y, (y - top) as f64 / 1i64.max(height) as f64 * 0.7) {
                color = ramp[0];
            }
            if shine && dx < 1i64.max(width / 2 + 1) {
                color = mix(color, WHITE, 0.5);
            }
            canvas.put(x + dx, y, color);
        }
    }
    // Lit cap.
    canvas.rect(x, top, x + width - 1, top, if glow { WHITE } else { ramp[3] });
    if glow {
        for dx in 0..width {
            canvas.put(x + dx, top - 2, mix(ramp[3], WHITE, 0.5));
        }
    }
}

/// A zigzag lightning bolt from y0 to y1 around cx, white core and coloured glow.
fn draw_bolt(canvas: &mut Canvas, cx: i64, y0: i64, y1: i64, seed: i64, bright: bool) {
    let mut rng = Random::new(seed);
    let mut points = vec![(cx, y0)];
    let mut y = y0;
    while y < y1 {
        y = y1.min(y + rng.randrange(6, 12));
        let x = cx + rng.randrange(-7, 8);
        points.push((x, y));
    }
    let glow = if bright { GOLD[1] } else { GOLD[2] };
    for pair in points.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        for offset in [-1, 1] {
            canvas.line(ax + offset, ay, bx + offset, by, glow);
        }
        canvas.line(ax, ay, bx, by, if bright { WHITE } else { GOLD[0] });
    }
}

/// Big outlined VS: the 5 x 7 font at scale 3, filled with a gold ramp from top to bottom.
/// Returns the filled pixels (with their colour) and the outline pixels, in art coordinates.
fn vs_sprite() -> (Vec<(i64, i64, Rgb)>, HashSet<(i64, i64)>, i64, i64) {
    let width = text_width("VS", Font::F57, 3) + 4;
    let height = 7 * 3 + 4;
    let mut layer = Canvas::with_background(width, height, [0, 0, 0]);
    let marker = [1, 2, 3];
    layer.text(2.0, 2.0, "VS", marker, Font::F57, 3, None);
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if layer.get(x, y) == marker {
                let ramp = if y < 7 { GOLD[0] } else if y < 15 { GOLD[1] } else { GOLD[2] };
                pixels.push((x, y, ramp));
            }
        }
    }
    let filled: HashSet<(i64, i64)> = pixels.iter().map(|&(x, y, _)| (x, y)).collect();
    let mut outline = HashSet::new();
    for &(x, y) in &filled {
        for dx in -2..=2i64 {
            for dy in -2..=2i64 {
                if dx.abs() + dy.abs() <= 2 && !filled.contains(&(x + dx, y + dy)) {
                    outline.insert((x + dx, y + dy));
                }
            }
        }
    }
    (pixels, outline, width, height)
}

/// Progress of pillar `index` (0 to 1, small overshoot when it lands).
fn day_progress(f: i64, index: i64, pillars: i64) -> f64 {
    let start = GROW_START as f64 + (index * (GROW_FRAMES - RISE)) as f64 / (pillars - 1).max(1) as f64;
    let t = (f as f64 - start) / RISE as f64;
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.6 {
        return 1.0;
    }
    if t < 1.0 {
        return (1.0 - (1.0 - t).powf(3.0)) * 1.15;
    }
    1.15 - (t - 1.0) / 0.6 * 0.15
}

struct Camp {
    is_left: bool,
    login: String,
    days: Vec<i64>,
    total: i64,
    x0: i64,
    center: i64,
    hair: Rgb,
    photo: Option<Vec<Rgb>>,
}

impl Camp {
    fn colors(&self) -> &'static Colors {
        if self.is_left { &LEFT } else { &RIGHT }
    }
}

/// Renders the duel against the friend `login` from `data.friends` over `days` (7, 30 or 365),
/// written under the output name the user asked for.
pub fn render_as(data: &Data, login: &str, days: i64, name: &str) -> String {
    let period = period(days);
    let friend = data.friends.get(login).unwrap_or_else(|| panic!("no friend {login} in data.json"));
    let accounts = [(true, data.login.clone(), data), (false, friend.login.clone(), friend)];
    let span = period.pillars * period.pitch - (period.pitch - period.width);
    let camps: Vec<Camp> = accounts
        .iter()
        .map(|(is_left, name_in_data, account)| {
            let (days_values, total) = period_values(account, days, &period);
            let (lo, hi) = if *is_left { LEFT_AREA } else { RIGHT_AREA };
            let x0 = lo + py_floordiv(hi - lo - span, 2);
            let camp_login = if name_in_data.is_empty() { login.to_string() } else { name_in_data.clone() };
            let colors = if *is_left { &LEFT } else { &RIGHT };
            let photo = account.avatar.as_deref().and_then(|path| pixel_avatar(path, colors));
            Camp {
                is_left: *is_left,
                hair: hair_for(&camp_login),
                login: camp_login,
                days: days_values,
                total,
                x0,
                center: x0 + span / 2,
                photo,
            }
        })
        .collect();
    let peak = camps.iter().flat_map(|c| c.days.iter().copied()).max().unwrap_or(0);
    let best = camps.iter().map(|c| c.total).max().unwrap_or(0);
    // None for a draw.
    let winner: Option<usize> = match camps[0].total.cmp(&camps[1].total) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(0),
        std::cmp::Ordering::Less => Some(1),
    };
    let pillar_height = |count: i64| -> i64 {
        // Same scale for both camps: square root of the share of the busiest day of the two.
        if count <= 0 || peak <= 0 {
            return 0;
        }
        py_round((CHART_BASE - CHART_TOP) as f64 * (count as f64 / peak as f64).sqrt()).max(3)
    };

    let back = backdrop();
    let (vs_pixels, vs_outline, vs_w, _vs_h) = vs_sprite();

    let mut frames = Vec::with_capacity(LOOP as usize);
    for f in 0..LOOP {
        let mut canvas = Canvas::new(W, H);
        canvas.pixels.copy_from_slice(&back.pixels);
        let verdict = f >= VERDICT;

        // Centre lightning, crackling on a 4-frame beat, brighter at the slam and the verdict.
        let slam = (5..SLAM).contains(&f) || (VERDICT..VERDICT + 3).contains(&f);
        if slam || f % 4 == 0 || f % 4 == 1 {
            draw_bolt(&mut canvas, W / 2, 44, CHART_BASE, (f / 2) * 7 + 1, slam || f % 4 == 0);
        }

        // Charts.
        let mut shown = Vec::new();
        for (ci, camp) in camps.iter().enumerate() {
            let x0 = camp.x0;
            let mut total = 0;
            for (index, &count) in camp.days.iter().enumerate() {
                let index = index as i64;
                let grow = day_progress(f, index, period.pillars);
                if grow <= 0.0 {
                    continue;
                }
                if grow > 0.9 {
                    total += count;
                }
                let height = py_round(pillar_height(count) as f64 * grow);
                let is_winner = verdict && winner == Some(ci);
                // After the verdict a light sweeps across the winning chart, one pass every 16 frames.
                let band = py_imod(f - VERDICT, 16) * period.pillars / 16;
                let sweep = is_winner && band <= index && index < band + 1i64.max(period.pillars / 15);
                let glow = grow > 1.0 && count > 0;
                draw_pillar(&mut canvas, x0 + index * period.pitch, height, camp.colors(), glow, sweep, period.width);
            }
            shown.push(camp.total.min(total));
        }

        // Labels on the floor.
        canvas.text_center((W / 2) as f64, (CHART_BASE + 6) as f64, period.floor, TEXT, Font::F35, 2, Some(SHADOW));

        // Top band: portraits, bars and the VS emblem.
        for (ci, camp) in camps.iter().enumerate() {
            let is_left = camp.is_left;
            let px = if is_left { 4 } else { W - 4 - PORTRAIT };
            draw_portrait(&mut canvas, px, 4, is_left, camp.colors(), camp.hair, !is_left, camp.photo.as_deref());
            let (bx0, bx1) = if is_left { LEFT_BAR } else { RIGHT_BAR };
            let value = shown[ci];
            let fill = if best != 0 { value as f64 / best as f64 } else { 0.0 };
            let flash = (VERDICT..VERDICT + 2).contains(&f) && winner == Some(ci);
            // The bars fill from the portrait towards the centre.
            draw_bar(&mut canvas, bx0, bx1, fill, camp.colors(), is_left, flash);
            // Total under the bar, name under the portrait.
            let number = value.to_string();
            let ny = BAR_Y + BAR_H + 6;
            let number_color = if verdict && winner == Some(ci) { GOLD[0] } else { TEXT };
            if is_left {
                canvas.text(bx0 as f64, ny as f64, &number, number_color, Font::F57, 2, Some(SHADOW));
            } else {
                canvas.text_right(bx1 as f64, ny as f64, &number, number_color, Font::F57, 2, Some(SHADOW));
            }
            let login_text: String = camp.login.to_uppercase().chars().take(15).collect();
            let ly = 4 + PORTRAIT + 10;
            let ramp = camp.colors().ramp;
            if is_left {
                outlined_text(&mut canvas, 6, ly, &login_text, ramp[3], 2);
            } else {
                let x = W - 6 - text_width(&login_text, Font::F57, 2);
                outlined_text(&mut canvas, x, ly, &login_text, ramp[3], 2);
            }
            // Crown on the winner's portrait, dropping in at the verdict.
            if verdict && winner == Some(ci) {
                let drop = 0i64.max(12 - (f - VERDICT) * 2);
                let cx = px + PORTRAIT / 2 - 6;
                let cy = -1 - drop;
                let crown_colour = |key: u8| -> Rgb {
                    match key {
                        b'o' => OUTLINE,
                        b'y' => GOLD[1],
                        b'R' => [230, 60, 70],
                        b'W' => [120, 220, 255],
                        _ => panic!("unknown crown key {}", key as char),
                    }
                };
                blit(&mut canvas, &CROWN, cx, cy, &crown_colour, false);
                let beat = py_imod(f - VERDICT, 8);
                if beat < 3 {
                    let reach = [1, 2, 1][beat as usize];
                    for d in 1..=reach {
                        for (dx, dy) in [(d, 0), (-d, 0), (0, d), (0, -d)] {
                            canvas.put(cx + 13 + dx, cy + 3 + dy, GOLD[0]);
                        }
                    }
                }
            }
        }

        // VS emblem: slams in large, then settles. It drops in from above the frame and lands
        // at frame 5 with a flash.
        let vx = W / 2 - vs_w / 2;
        let fall = if f < 5 { py_round(34.0 * (1.0 - f as f64 / 5.0).powf(2.0)) } else { 0 };
        let vy = 4 - fall + if f == 5 { 1 } else { 0 };
        for &(x, y) in &vs_outline {
            canvas.put(vx + x, vy + y, OUTLINE);
        }
        for &(x, y, color) in &vs_pixels {
            canvas.put(vx + x, vy + y, if slam && f % 2 == 0 { WHITE } else { color });
        }

        // Verdict banner over the winner's chart, or DRAW in the centre.
        if verdict {
            let age = f - VERDICT;
            let (text, cx, color) = match winner {
                None => ("DRAW", W / 2, TEXT),
                Some(w) => ("WINNER", camps[w].center, GOLD[0]),
            };
            let scale = if age >= 2 { 3 } else { 4 };
            let tw = text_width(text, Font::F57, scale);
            let ty = if scale == 3 { 64 } else { 62 };
            outlined_text(&mut canvas, cx - tw / 2, ty, text, if age >= 1 { color } else { WHITE }, scale);
        }

        // An account with nothing in 30 days gets a short note instead of an empty chart.
        for camp in &camps {
            if camp.total == 0 && f >= GROW_START + GROW_FRAMES {
                canvas.text_center(camp.center as f64, 120.0, "NO ACTIVITY", MUTED, Font::F57, 2, Some(SHADOW));
            }
        }

        // Frame.
        for x in 0..W {
            canvas.put(x, 0, BORDER);
            canvas.put(x, H - 1, BORDER);
        }
        for y in 0..H {
            canvas.put(0, y, BORDER);
            canvas.put(W - 1, y, BORDER);
        }
        frames.push(canvas);
    }
    save(&frames, name, Save { still: Some((LOOP - 4) as usize), duration: FRAME_MS, ..Save::default() })
}
