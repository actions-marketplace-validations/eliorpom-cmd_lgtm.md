//! Port of the Python prototype. Records: three engraved brass plaques on velvet in a walnut frame.
//! Best day (with its date), longest streak and longest break over the last 12 months.
//! A glint sweeps across each plaque in turn; everything else stays still, like a trophy case.

use std::collections::HashSet;

use crate::data::{Data, Day};
use crate::export::{save, Save};
use crate::pixel::{best_day, counts, dither_on, longest_run, short_date, text_width, Canvas, Font, Rgb, BAYER, FRAMES};
use crate::pymath::{py_floordiv, py_imod, py_round};

const W: i64 = 400;
const H: i64 = 128;

// Palette: walnut frame, crimson velvet, brass ramp, engraving.
const WOOD: [Rgb; 6] = [[34, 20, 13], [52, 31, 19], [70, 43, 26], [92, 58, 34], [118, 78, 44], [146, 102, 60]];
const VELVET: [Rgb; 5] = [[30, 6, 14], [46, 10, 22], [64, 15, 30], [84, 22, 40], [104, 30, 50]];
const BRASS: [Rgb; 7] = [[62, 42, 16], [100, 74, 30], [142, 108, 46], [186, 144, 62], [214, 172, 86], [238, 206, 128], [255, 238, 186]];
const INK: Rgb = [36, 21, 8]; // Engraving filled with dark lacquer.
const INK_DEEP: Rgb = [20, 11, 4]; // Upper wall of each groove, in shadow.
const SPARK: Rgb = [255, 252, 230];

const PLAQUE_W: i64 = 118;
const PLAQUE_H: i64 = 92;
const GAP: i64 = 10;
const PLAQUES_Y: i64 = 17;

/// Stable pseudo-random value in [0, 1) for texture, so frames never flicker.
/// Python computes exact integers and masks them; the low 32 bits of wrapping `u64`
/// products are the same, and every argument here is non-negative.
fn hash01(x: i64, y: i64, seed: i64) -> f64 {
    const MASK: u64 = 0xFFFF_FFFF;
    let n = (x as u64)
        .wrapping_mul(374761393)
        .wrapping_add((y as u64).wrapping_mul(668265263))
        .wrapping_add((seed as u64).wrapping_mul(2147483647))
        & MASK;
    let n = ((n ^ (n >> 13)).wrapping_mul(1274126177)) & MASK;
    ((n ^ (n >> 16)) & 0xFFFF) as f64 / 65536.0
}

/// Pick from a ramp with Bayer dithering between neighbouring tones.
fn ramp(colors: &[Rgb], value: f64, x: i64, y: i64) -> Rgb {
    let last = colors.len() as f64 - 1.0;
    let value = 0.0f64.max(last.min(value));
    let mut low = value as i64;
    if (low as usize) < colors.len() - 1
        && (value - low as f64) * 16.0 > BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64
    {
        low += 1;
    }
    colors[low as usize]
}

/// Glyph pixels of `text`; `spacing` is the gap between letters in pixels.
/// Not `Canvas::text`: this one spaces letters by `spacing`, not by one font pixel.
fn text_mask(text: &str, font: Font, scale: i64, spacing: i64) -> HashSet<(i64, i64)> {
    let (width, _) = font.size();
    let mut mask = HashSet::new();
    for (offset, ch) in text.to_uppercase().chars().enumerate() {
        let glyph = font.glyph(ch);
        for (index, bit) in glyph.bytes().enumerate() {
            if bit == b'1' {
                let index = index as i64;
                let gx = offset as i64 * (width * scale + spacing) + (index % width) * scale;
                let gy = (index / width) * scale;
                for sy in 0..scale {
                    for sx in 0..scale {
                        mask.insert((gx + sx, gy + sy));
                    }
                }
            }
        }
    }
    mask
}

/// Text cut into the metal: lit lower lip, dark lacquer, shadowed upper wall.
fn engrave(canvas: &mut Canvas, cx: f64, y: i64, text: &str, font: Font, scale: i64, lip: Rgb, spacing: Option<i64>) {
    let spacing = spacing.unwrap_or(scale);
    let mask = text_mask(text, font, scale, spacing);
    let x = if mask.is_empty() {
        0
    } else {
        let max_x = mask.iter().map(|&(mx, _)| mx).max().unwrap_or(0);
        py_round(cx - (max_x + 1) as f64 / 2.0)
    };
    for &(mx, my) in &mask {
        if !mask.contains(&(mx, my + 1)) {
            canvas.put(x + mx, y + my + 1, lip);
        }
        if !mask.contains(&(mx + 1, my)) && !mask.contains(&(mx + 1, my + 1)) {
            canvas.put(x + mx + 1, y + my, lip);
        }
    }
    for &(mx, my) in &mask {
        let top = !mask.contains(&(mx, my - 1)) || !mask.contains(&(mx - 1, my));
        canvas.put(x + mx, y + my, if top { INK_DEEP } else { INK });
    }
}

fn draw_frame_and_velvet(canvas: &mut Canvas) {
    let border: i64 = 7;
    for y in 0..H {
        for x in 0..W {
            let inner = border <= x && x < W - border && border <= y && y < H - border;
            if inner {
                // Velvet: soft pool of light in the middle, short pile noise.
                let dx = (x as f64 - W as f64 / 2.0) / (W as f64 / 2.0);
                let dy = (y as f64 - H as f64 * 0.42) / (H as f64 / 2.0);
                let mut light = 3.1 - 1.9 * (dx * dx * 0.7 + dy * dy);
                light += (hash01(x, y, 3) - 0.5) * 0.9;
                canvas.put(x, y, ramp(&VELVET, light, x, y));
                continue;
            }
            // Walnut rails with grain running along each rail, mitred at the corners.
            let (d_top, d_bottom, d_left, d_right) = (y, H - 1 - y, x, W - 1 - x);
            let depth = d_top.min(d_bottom).min(d_left).min(d_right);
            let horizontal = d_top.min(d_bottom) <= d_left.min(d_right);
            let (along, across) = if horizontal { (x, y) } else { (y, x + 1000) };
            let grain = hash01(across, py_floordiv(along, 9), 11) * 0.8 + hash01(across, py_floordiv(along, 3), 12) * 0.35;
            let stripe = if hash01(across, py_floordiv(along + across * 7, 23), 13) > 0.82 { 1.0 } else { 0.0 };
            let mut value = 2.2 + grain - stripe * 0.9;
            // Bevel: outer edge dark, top/left facets lit, bottom/right facets in shade.
            if depth == 0 {
                value = 0.0;
            } else if depth == border - 1 {
                value = 0.6; // Inner lip in shadow where the velvet starts.
            } else if depth == border - 2 {
                // Thin gilt fillet around the velvet, lit on the lower right edge facing up-left light.
                let color = if d_bottom == border - 2 || d_right == border - 2 { BRASS[4] } else { BRASS[2] };
                canvas.put(x, y, color);
                continue;
            } else if depth == 1 {
                value += if d_top == 1 || d_left == 1 { 1.1 } else { -0.6 };
            }
            if d_bottom < border || d_right < border {
                value -= 0.5;
            }
            canvas.put(x, y, ramp(&WOOD, value, x, y));
        }
    }
}

fn plaque_shadow(canvas: &mut Canvas, x0: i64, y0: i64) {
    for y in y0 + 2..y0 + PLAQUE_H + 4 {
        for x in x0 + 2..x0 + PLAQUE_W + 3 {
            let soft = x > x0 + PLAQUE_W || y >= y0 + PLAQUE_H + 2;
            if soft && !dither_on(x, y, 0.5) {
                continue;
            }
            canvas.put(x, y, VELVET[0]);
        }
    }
}

/// Position (0..1) of the glint sweeping plaque `index`, or None when it rests.
fn glint_at(t: f64, index: i64) -> Option<f64> {
    let start = 0.06 + index as f64 * 0.13;
    let length = 0.2;
    let p = (t - start) / length;
    if (0.0..=1.0).contains(&p) { Some(p) } else { None }
}

fn draw_plaque(canvas: &mut Canvas, x0: i64, y0: i64, plaque: &(String, String, String), t: f64, index: i64, screws_low: bool) {
    let (label, value, unit) = (plaque.0.as_str(), plaque.1.as_str(), plaque.2.as_str());
    let x1 = x0 + PLAQUE_W - 1;
    let y1 = y0 + PLAQUE_H - 1;
    let sweep = glint_at(t, index);
    let band = sweep.map(|s| (x0 - 40) as f64 + s * (PLAQUE_W + 80) as f64);
    for y in y0..=y1 {
        // Brushed metal: each row gets its own faint streak.
        let v = (y - y0) as f64 / PLAQUE_H as f64;
        // Polished metal reflects the room: bright top, dark band, second soft highlight, dark foot.
        let profile = 4.3 - 2.4 * 1.0f64.min((v - 0.05).abs() / 0.4) + 1.3 * 0.0f64.max(1.0 - (v - 0.68).abs() / 0.18)
            - 0.6 * 0.0f64.max((v - 0.85) / 0.15);
        let streak = (hash01(0, y, 5) - 0.5) * 0.35;
        for x in x0..=x1 {
            let ex = (x - x0).min(x1 - x);
            let ey = (y - y0).min(y1 - y);
            // Cut corners, 2 px chamfer.
            if (x - x0) + (y - y0) < 3 || (x1 - x) + (y - y0) < 3 || (x - x0) + (y1 - y) < 3 || (x1 - x) + (y1 - y) < 3 {
                if (x - x0) + (y - y0) == 2 || (x1 - x) + (y - y0) == 2 || (x - x0) + (y1 - y) == 2 || (x1 - x) + (y1 - y) == 2 {
                    canvas.put(x, y, BRASS[0]);
                }
                continue;
            }
            let mut level = profile + streak + (hash01(x / 7, y, 7) - 0.5) * 0.3;
            let edge = ex.min(ey);
            if edge == 0 {
                level = 0.0;
            } else if edge <= 2 {
                // Bevel: top and left facets catch the light, bottom and right fall away.
                let lit = (y - y0) <= 2 || (x - x0) <= 2;
                let shade = (y1 - y) <= 2 || (x1 - x) <= 2;
                level = if lit && !shade { 5.4 - edge as f64 * 0.4 } else { 1.3 + edge as f64 * 0.3 };
                if lit && shade {
                    level = 3.0;
                }
            } else if edge == 3 {
                level -= 0.6; // Thin step between bevel and face.
            }
            if let Some(band) = band {
                let d = (x as f64 - band) + (y - y0) as f64 * 0.55;
                if d.abs() < 2.0 {
                    level = 6.0;
                } else if d.abs() < 5.0 {
                    level += 2.6;
                } else if d.abs() < 9.0 {
                    level += 1.1;
                } else if (d - 14.0).abs() < 1.5 {
                    level += 1.8;
                }
            }
            canvas.put(x, y, ramp(&BRASS, level, x, y));
        }
    }

    let cx = x0 as f64 + PLAQUE_W as f64 / 2.0;
    let lip = BRASS[5];
    engrave(canvas, cx, y0 + 9, label, Font::F35, 2, lip, Some(1));
    // Engraved double rule under the title.
    for x in x0 + 14..x1 - 13 {
        canvas.put(x, y0 + 24, INK);
        canvas.put(x, y0 + 25, lip);
    }
    for x in [x0 + 12, x1 - 12] {
        canvas.put(x, y0 + 24, INK);
    }
    let number_scale = if text_width(value, Font::F57, 4) <= PLAQUE_W - 34 { 4 } else { 3 };
    let number_y = if number_scale == 4 { y0 + 32 } else { y0 + 35 };
    engrave(canvas, cx, number_y, value, Font::F57, number_scale, lip, None);
    engrave(canvas, cx, y0 + 69, unit, Font::F57, 2, lip, None);

    // Two slotted screws, left and right of the number.
    // Screws sit beside the number, or in the lower corners when a long number needs the room.
    for sx in [x0 + 10, x1 - 9] {
        let sy = if screws_low { y1 - 9 } else { y0 + 45 };
        for dy in -4..=4i64 {
            for dx in -4..=4i64 {
                let r = dx * dx + dy * dy;
                if r <= 18 {
                    let shade = if r > 12 {
                        // Countersink: dark on the upper left, lit lip on the lower right.
                        if dx + dy < 0 { INK } else { BRASS[5] }
                    } else if r >= 7 && dx + dy > 0 {
                        BRASS[1]
                    } else if dx + dy < -1 {
                        BRASS[5]
                    } else {
                        BRASS[3]
                    };
                    canvas.put(sx + dx, sy + dy, shade);
                }
            }
        }
        let slant = index % 2; // Screws turned a little differently, as if set by hand.
        for k in -3..=3i64 {
            canvas.put(sx + k, sy + py_floordiv(k * slant, 3), INK);
            canvas.put(sx + k, sy + 1 + py_floordiv(k * slant, 3), BRASS[2]);
        }
        canvas.put(sx - 1, sy - 2, BRASS[6]);
    }

    // A small four-point sparkle where the glint leaves the plaque.
    if let Some(s) = sweep
        && 0.7 < s && s < 1.0 {
            let size: i64 = if s < 0.85 { 3 } else { 1 };
            let (sx, sy) = (x1 - 6, y0 + 5);
            canvas.put(sx, sy, SPARK);
            for k in 1..=size {
                for (ddx, ddy) in [(k, 0), (-k, 0), (0, k), (0, -k)] {
                    canvas.put(sx + ddx, sy + ddy, if k < size { SPARK } else { BRASS[6] });
                }
            }
        }
}

fn plaques_of(data: &Data) -> Vec<(String, String, String)> {
    let days: &[Day] = data.year_days();
    let values = counts(days);
    let (best_value, best_date) = if !days.is_empty() && values.iter().copied().max().unwrap_or(0) > 0 {
        let best = best_day(days);
        (best.count.to_string(), short_date(&best.date))
    } else {
        ("0".to_string(), "NONE YET".to_string())
    };
    let streak = longest_run(days, true);
    let pause = longest_run(days, false); // Ignores the days before the first contribution.

    let days_word = |n: i64| if n == 1 { "DAY" } else { "DAYS" };

    vec![
        ("BEST DAY".to_string(), best_value, best_date),
        ("LONGEST STREAK".to_string(), streak.to_string(), days_word(streak).to_string()),
        ("LONGEST BREAK".to_string(), pause.to_string(), days_word(pause).to_string()),
    ]
}

pub fn render(data: &Data) -> String {
    let plaques = plaques_of(data);
    let mut base = Canvas::new(W, H);
    draw_frame_and_velvet(&mut base);
    let total = 3 * PLAQUE_W + 2 * GAP;
    let xs: Vec<i64> = (0..3).map(|i| (W - total) / 2 + i * (PLAQUE_W + GAP)).collect();
    for &x0 in &xs {
        plaque_shadow(&mut base, x0, PLAQUES_Y);
    }

    let screws_low = plaques.iter().any(|(_, value, _)| text_width(value, Font::F57, 4) > 70);
    let mut frames: Vec<Canvas> = Vec::with_capacity(FRAMES);
    for f in 0..FRAMES {
        let t = f as f64 / FRAMES as f64;
        let mut canvas = base.clone();
        for (index, &x0) in xs.iter().enumerate() {
            draw_plaque(&mut canvas, x0, PLAQUES_Y, &plaques[index], t, index as i64, screws_low);
        }
        frames.push(canvas);
    }
    // Still: a frame where no glint crosses the numbers.
    save(&frames, "records", Save { still: Some(FRAMES - 1), ..Save::default() })
}
