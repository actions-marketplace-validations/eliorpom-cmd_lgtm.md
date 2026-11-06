//! Port of the Python prototype. Mechanical drum counter: contributions over the last 12 months.
//! The drums roll from (total - yesterday) up to the total, then hold. Higher drums only
//! turn while the drum below goes from 9 to 0.

use crate::data::Data;
use crate::export::{save, Save};
use crate::pixel::{mix, panel, Canvas, Font, Rgb, BG, BORDER, FRAMES, GREEN};
use crate::pymath::{py_floordiv, py_imod, py_mod, py_round};

const W: i64 = 208;
const H: i64 = 128;
const TEXT: Rgb = [230, 237, 243];
const MUTED: Rgb = [139, 148, 158];

// Housing and window geometry, in art pixels.
const BOX_X0: i64 = 10;
const BOX_X1: i64 = 197;
const BOX_Y0: i64 = 24;
const BOX_Y1: i64 = 80;
const WIN_X0: i64 = BOX_X0 + 15;
const WIN_X1: i64 = BOX_X1 - 15;
const WIN_Y0: i64 = BOX_Y0 + 8;
const WIN_Y1: i64 = BOX_Y1 - 8;
const GAP: i64 = 3;

// Brushed steel plate (dark to lit), recess, drum surface and painted digits.
const PLATE: [Rgb; 5] = [[30, 34, 44], [44, 50, 62], [58, 64, 78], [78, 86, 100], [108, 116, 132]];
const RECESS: Rgb = [8, 9, 12];
const DRUM: [Rgb; 6] = [[12, 13, 17], [26, 28, 35], [42, 45, 54], [60, 64, 75], [84, 89, 102], [112, 118, 132]];
const INK: [Rgb; 6] = [[70, 66, 58], [120, 114, 100], [176, 168, 150], [226, 218, 196], [246, 240, 222], [255, 255, 246]];
// Leading zeros are painted dimmer so the real number stands out.
const INK_DIM: [Rgb; 6] = [[40, 42, 48], [62, 64, 70], [84, 86, 92], [104, 106, 112], [118, 120, 126], [130, 132, 138]];
const POINTER: [Rgb; 3] = [[120, 28, 28], [200, 54, 44], [240, 100, 80]];

const SCREW: [&str; 5] = [".122.", "12223", "14443", "12233", ".233."];
const ARROW: [&str; 5] = ["1...", "21..", "321.", "21..", "1..."];

// The roll runs from 0 to 45% of the loop, then holds.
const ROLL_START: f64 = 0.0;
const ROLL_END: f64 = 0.45;

/// Flat colour bands with a thin 50% checker in the last 20% before each band edge.
/// Port of `band(colors, value, x, y)` with the default zone.
fn band(colors: &[Rgb], value: f64, x: i64, y: i64) -> Rgb {
    let value = (colors.len() as f64 - 1.0).min(0.0f64.max(value));
    let low = value as usize;
    if low + 1 < colors.len() && value - low as f64 > 1.0 - 0.2 && py_imod(x + y, 2) == 0 {
        return colors[low + 1];
    }
    colors[low]
}

fn smooth(value: f64) -> f64 {
    let value = 1.0f64.min(0.0f64.max(value));
    value * value * (3.0 - 2.0 * value)
}

/// Odometer reading: each drum's position (0 to 10), with the carry roll.
/// Most significant drum first, as in the Python list.
fn drum_positions(value: f64, digits: i64) -> Vec<f64> {
    let mut positions = Vec::new();
    for power in 0..digits {
        if power == 0 {
            positions.push(py_mod(value, 10.0));
        } else {
            let unit = 10i64.pow(power as u32);
            let whole = py_imod((value / unit as f64).floor() as i64, 10);
            let below = py_mod(value, unit as f64);
            let carry = 0.0f64.max(below - (unit - 1) as f64);
            positions.push(whole as f64 + carry);
        }
    }
    positions.reverse();
    positions
}

fn glyph_bit(glyph: &str, col: i64, row: i64) -> bool {
    if !((0..5).contains(&col) && (0..7).contains(&row)) {
        return false;
    }
    glyph.as_bytes()[(row * 5 + col) as usize] == b'1'
}

fn screw_color(key: u8) -> Rgb {
    match key {
        b'1' => [150, 158, 172],
        b'2' => [100, 108, 122],
        b'3' => [54, 60, 72],
        _ => [24, 26, 32],
    }
}

pub fn render(data: &Data) -> String {
    let total = data.total_contributions.max(0);
    let yesterday = data.year_days().last().map_or(0, |day| day.count).min(total);
    let start = total - yesterday;
    let digits = 5.max(total.to_string().len() as i64);

    // Drum geometry.
    let drum_w = 28i64.min(py_floordiv(WIN_X1 - WIN_X0 + 1 - GAP * (digits - 1), digits));
    let scale: i64 = if drum_w >= 24 { 4 } else { 3 };
    let span = digits * drum_w + GAP * (digits - 1);
    let drums_x0 = py_floordiv(WIN_X0 + WIN_X1 + 1 - span, 2);
    let centre_y = (WIN_Y0 + WIN_Y1) as f64 / 2.0;
    let half_view = (WIN_Y1 - WIN_Y0) as f64 / 2.0;
    let radius = half_view + 3.0;
    let pitch = (7 * scale + 7) as f64; // Arc length between two digits on the drum.

    let value_at = |t: f64| -> f64 {
        start as f64 + yesterday as f64 * smooth((t - ROLL_START) / (ROLL_END - ROLL_START))
    };

    let draw = |t: f64| -> Canvas {
        let mut canvas = Canvas::new(W, H);
        panel(&mut canvas, BG, BORDER);
        canvas.text_center(W as f64 / 2.0, 5.0, "CONTRIBUTIONS", TEXT, Font::F57, 2, Some([8, 10, 14]));

        // Plate with bevels and a brushed grain.
        for y in BOX_Y0..=BOX_Y1 {
            for x in BOX_X0..=BOX_X1 {
                let corner = (x - BOX_X0).min(BOX_X1 - x).min(y - BOX_Y0).min(BOX_Y1 - y);
                if corner == 0 && (x == BOX_X0 || x == BOX_X1) && (y == BOX_Y0 || y == BOX_Y1) {
                    continue;
                }
                let level = if corner == 0 {
                    0.0
                } else if y - BOX_Y0 == 1 || x - BOX_X0 == 1 {
                    4.0
                } else if BOX_Y1 - y == 1 || BOX_X1 - x == 1 {
                    1.0
                } else {
                    2.55 + 0.35 * (y as f64 * 2.3 + py_floordiv(x, 9) as f64 * 0.7).sin()
                        - (y - BOX_Y0) as f64 / (BOX_Y1 - BOX_Y0) as f64 * 0.6
                };
                canvas.put(x, y, band(&PLATE, level, x, y));
            }
        }
        // Recessed window: dark lip, inner shadow, chrome rim.
        canvas.rect(WIN_X0 - 3, WIN_Y0 - 3, WIN_X1 + 3, WIN_Y1 + 3, PLATE[0]);
        canvas.rect(WIN_X0 - 2, WIN_Y0 - 2, WIN_X1 + 2, WIN_Y1 + 2, PLATE[4]);
        canvas.rect(WIN_X0 - 2, WIN_Y1 + 1, WIN_X1 + 2, WIN_Y1 + 2, PLATE[3]);
        canvas.rect(WIN_X0 - 1, WIN_Y0 - 1, WIN_X1 + 1, WIN_Y1 + 1, RECESS);
        for (sx, sy) in [
            (BOX_X0 + 3, BOX_Y0 + 3),
            (BOX_X1 - 7, BOX_Y0 + 3),
            (BOX_X0 + 3, BOX_Y1 - 7),
            (BOX_X1 - 7, BOX_Y1 - 7),
        ] {
            for (row, line) in SCREW.iter().enumerate() {
                for (col, key) in line.bytes().enumerate() {
                    if key != b'.' {
                        canvas.put(sx + col as i64, sy + row as i64, screw_color(key));
                    }
                }
            }
        }

        let value = value_at(t);
        let positions = drum_positions(value, digits);
        let prev = if t > 0.0 { drum_positions(value_at(t - 1.0 / FRAMES as f64), digits) } else { positions.clone() };
        for (index, &position) in positions.iter().enumerate() {
            let index = index as i64;
            let x0 = drums_x0 + index * (drum_w + GAP);
            let speed = py_mod((position - prev[index as usize]).abs(), 10.0); // Digits per frame, for the blur.
            let samples: i64 = if speed < 0.25 { 1 } else { 9.min(1 + (speed * 4.0) as i64) };
            let glyph_x = x0 + py_floordiv(drum_w - 5 * scale, 2);
            // A drum is a leading zero while it rests on 0 and every drum before it does too.
            let dim = index < digits - 1 && positions[..=index as usize].iter().all(|&p| p < 0.02 || p > 9.98);
            for y in WIN_Y0..=WIN_Y1 {
                let dy = y as f64 + 0.5 - centre_y;
                let theta = (dy / radius).min(0.999).max(-0.999).asin();
                let facing = theta.cos();
                // Specular band on the upper side of the drum.
                let spec = 0.0f64.max(1.0 - (theta + 0.5).abs() / 0.13);
                let surface = facing.powf(1.6) * 4.4 - 0.8 + spec * 2.2;
                let mut hits = vec![0.0f64; drum_w as usize];
                for k in 0..samples {
                    let s = radius * theta + (position - speed * k as f64 / samples as f64) * pitch;
                    let digit = py_round(s / pitch);
                    let local = s - digit as f64 * pitch + 3.5 * scale as f64;
                    let row = (local / scale as f64).floor() as i64;
                    let glyph = Font::F57.glyph((b'0' + py_imod(digit, 10) as u8) as char);
                    for col in 0..drum_w {
                        let gx = col - (glyph_x - x0);
                        if gx >= 0 && glyph_bit(glyph, gx / scale, row) {
                            hits[col as usize] += 1.0 / samples as f64;
                        }
                    }
                }
                for col in 0..drum_w {
                    let x = x0 + col;
                    // The drum's sides turn away: darken the outer columns.
                    let side = 1.0
                        - ((col as f64 + 0.5 - drum_w as f64 / 2.0).abs() / (drum_w as f64 / 2.0)).powf(6.0) * 0.9;
                    let hit = hits[col as usize];
                    let color = if hit > 0.05 {
                        let ink = surface * side + 0.4;
                        let glyph_color = band(if dim { &INK_DIM } else { &INK }, ink, x, y);
                        if hit < 0.95 {
                            mix(band(&DRUM, surface * side, x, y), glyph_color, hit.powf(0.7))
                        } else {
                            glyph_color
                        }
                    } else {
                        band(&DRUM, surface * side, x, y)
                    };
                    canvas.put(x, y, color);
                }
            }
            // Thin metal rings between the drums.
            if index < digits - 1 {
                for y in WIN_Y0..=WIN_Y1 {
                    let dy = (y as f64 + 0.5 - centre_y).abs() / half_view;
                    for g in 0..GAP {
                        let shade = if g == GAP - 1 { 0.8 } else { 0.0 };
                        let level = 2.6 - dy * 2.2 - shade;
                        canvas.put(x0 + drum_w + g, y, band(&PLATE, level, x0 + drum_w + g, y));
                    }
                }
            }
        }

        // Inner shadow of the window: the top lip casts onto the drums.
        for x in WIN_X0..=WIN_X1 {
            canvas.put(x, WIN_Y0, RECESS);
            if py_imod(x + WIN_Y0, 2) != 0 {
                canvas.put(x, WIN_Y0 + 1, RECESS);
            }
            canvas.put(x, WIN_Y1, mix(RECESS, DRUM[1], 0.5));
        }
        // Glass glare: a static sheen and a diagonal glint that sweeps once while the count holds.
        let sweep = (t - 0.6) / 0.28;
        for y in WIN_Y0..=WIN_Y1 {
            for x in WIN_X0..=WIN_X1 {
                let diag = (x - WIN_X0) as f64 + (y - WIN_Y0) as f64 * 0.9;
                let mut lit: Option<f64> = None;
                if (0.0..=1.0).contains(&sweep) {
                    let pos = -30.0 + sweep * (WIN_X1 - WIN_X0 + 60) as f64;
                    let d = diag - pos;
                    if (0.0..6.0).contains(&d) || (0.0 <= d - 9.0 && d - 9.0 < 2.0) {
                        lit = Some(0.22);
                    }
                }
                if (y - WIN_Y0 == 2 || y - WIN_Y0 == 3) && py_imod(x - WIN_X0, 5) != 0 {
                    lit = Some(lit.unwrap_or(0.0).max(0.12));
                }
                if let Some(amount) = lit {
                    canvas.put(x, y, mix(canvas.get(x, y), [220, 232, 255], amount));
                }
            }
        }
        // Red pointers on both sides of the reading line.
        for (row, line) in ARROW.iter().enumerate() {
            for (col, key) in line.bytes().enumerate() {
                if key != b'.' {
                    let yy = py_round(centre_y) - 2 + row as i64;
                    let color = POINTER[(key - b'1') as usize];
                    canvas.put(WIN_X0 - 6 + col as i64, yy, color);
                    canvas.put(WIN_X1 + 6 - col as i64, yy, color);
                }
            }
        }

        // Yesterday's contributions, counted up with the drums.
        let gained = py_round(value - start as f64);
        if yesterday > 0 {
            let label = format!("+{gained} YESTERDAY");
            let color = if t >= ROLL_START { GREEN } else { mix(GREEN, BG, 0.5) };
            canvas.text_center(W as f64 / 2.0, 87.0, &label, color, Font::F57, 2, Some([10, 40, 18]));
        } else {
            canvas.text_center(W as f64 / 2.0, 87.0, "+0 YESTERDAY", MUTED, Font::F57, 2, None);
        }
        canvas.text_center(W as f64 / 2.0, 108.0, "LAST 12 MONTHS", [110, 118, 130], Font::F57, 2, None);
        canvas
    };

    let frames: Vec<Canvas> = (0..FRAMES).map(|index| draw(index as f64 / FRAMES as f64)).collect();
    save(&frames, "counter", Save { still: Some(FRAMES - 1), ..Save::default() })
}
