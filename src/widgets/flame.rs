//! Port of the Python prototype. Streak campfire: the fire grows by tiers with the current
//! streak of active days. 1-6 days small fire, 7-29 medium, 30-99 large, 100+ large with
//! a blue heart. A streak of 0 leaves cold charred logs, dying embers and a thin smoke.

use std::f64::consts::PI;

use crate::data::Data;
use crate::export::{save, Save};
use crate::pixel::{mix, panel, text_width, Canvas, Font, Rgb, BG, BORDER, FRAMES};
use crate::pymath::{py_hypot, py_imod, py_mod, py_round};
use crate::pyrand::Random;

const W: i64 = 208;
const H: i64 = 128;
const CX: i64 = 56; // Fire centre.
const BY: i64 = 104; // Ground line.

// Fire ramp, from the cool outline to the white-hot heart.
const FIRE: [Rgb; 6] = [[110, 24, 28], [186, 40, 32], [232, 90, 34], [250, 150, 46], [255, 210, 96], [255, 246, 206]];
const BLUE: [Rgb; 2] = [[96, 160, 240], [184, 226, 255]];
// Bark, from outline to lit top.
const BARK: [Rgb; 5] = [[34, 20, 18], [62, 36, 26], [94, 56, 36], [130, 82, 50], [172, 116, 68]];
const CHAR: [Rgb; 5] = [[22, 18, 22], [40, 34, 38], [62, 54, 56], [96, 88, 86], [134, 128, 124]];
// Charred wood still in the fire: dark with a red cast.
const BURNT: [Rgb; 5] = [[22, 14, 16], [40, 22, 22], [64, 34, 28], [96, 50, 34], [130, 70, 40]];
const STONE: [Rgb; 4] = [[26, 30, 40], [50, 54, 66], [76, 80, 92], [106, 108, 118]];
const TEXT_WARM: Rgb = [255, 226, 168];
const TEXT_COLD: Rgb = [150, 160, 176];
const TEXT: Rgb = [230, 237, 243];
const MUTED: Rgb = [139, 148, 158];

/// Tiers: (minimum streak, flame height in art pixels, sparks).
const TIERS: [(i64, i64, usize); 4] = [(1, 26, 3), (7, 40, 6), (30, 54, 10), (100, 64, 15)];

// Hand-drawn log end seen from the side: bark rim, sapwood, rings, heart, one crack.
const LOG_END: [&str; 9] = ["..ccc..", ".cbbbc.", "cbaaabc", "cbadabc", "cbdedbc", "cbadabc", "cbaaabc", ".cbbac.", "..ccc.."];

fn end_color(key: u8, cold: bool) -> Rgb {
    match (key, cold) {
        (b'a', false) => [196, 146, 92],
        (b'b', false) => [150, 100, 62],
        (b'c', false) => [70, 44, 30],
        (b'd', false) => [160, 112, 68],
        (b'e', false) => [120, 80, 50],
        (b'a', true) => [100, 94, 92],
        (b'b', true) => [70, 64, 66],
        (b'c', true) => [34, 28, 32],
        (b'd', true) => [84, 78, 78],
        _ => [124, 118, 114],
    }
}

// Hand-drawn stones: 0 = shadow, 1 = body, 2 = upper face, 3 = highlight.
const STONES: [&[&str]; 4] = [
    &["...1111...", ".11233211.", "1223333221", "1222332221", "1122222211", ".11111110.", "..000000.."],
    &["..1111..", ".123321.", "12333221", "12222211", ".1111110", "..0000.."],
    &["....111....", "..1123321..", ".122333221.", "12223322221", "11222222211", ".111111110.", "...00000..."],
    &[".1111.", "123321", "122221", "111110", ".0000."],
];

// Small flame icon for the tier pips.
const PIP: [&str; 6] = ["..1..", ".121.", ".121.", "12321", "12321", ".111."];

fn tier_of(streak: i64) -> usize {
    TIERS.iter().filter(|(minimum, _, _)| streak >= *minimum).count()
}

/// Loop-safe oscillation: a whole number of cycles per loop.
fn wave(t: f64, freq: f64, phase: f64) -> f64 {
    (2.0 * PI * (freq * t + phase)).sin()
}

/// Flat colour bands; a 50% checker only in a thin zone at each band edge.
fn band(colors: &[Rgb], value: f64, x: i64, y: i64, zone: f64) -> Rgb {
    let value = (colors.len() as f64 - 1.0).min(0.0f64.max(value));
    let low = value as usize;
    if low + 1 < colors.len() && value - low as f64 > 1.0 - zone && py_imod(x + y, 2) == 0 {
        return colors[low + 1];
    }
    colors[low]
}

fn band18(colors: &[Rgb], value: f64, x: i64, y: i64) -> Rgb {
    band(colors, value, x, y, 0.18)
}

/// Smooth value noise that wraps vertically, for the rising licks inside the flame.
fn noise_table(rng: &mut Random, width: usize, height: usize) -> Vec<Vec<f64>> {
    let raw: Vec<Vec<f64>> = (0..height).map(|_| (0..width).map(|_| rng.random()).collect()).collect();
    let mut smooth = vec![vec![0.0; width]; height];
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let mut total = 0.0;
            for dy in -3..4 {
                for dx in [-1, 0, 1] {
                    let row = py_imod(y + dy, height as i64) as usize;
                    let col = (width as i64 - 1).min(0.max(x + dx)) as usize;
                    total += raw[row][col];
                }
            }
            smooth[y as usize][x as usize] = total / 21.0;
        }
    }
    smooth
}

struct Log {
    start: (i64, i64),
    end: (i64, i64),
    radius: f64,
    seed: i64,
}

/// A round log between two points, shaded in bands as a cylinder lit from above.
fn draw_log(canvas: &mut Canvas, log: &Log, glow: f64, cold: bool, t: f64) {
    let burning = true;
    let seed = log.seed;
    let radius = log.radius;
    let mut rng = Random::new(seed);
    let knot = if rng.random() < 0.5 { rng.uniform(0.15, 0.3) } else { rng.uniform(0.7, 0.85) };
    let cracks: Vec<(f64, f64, f64, i64)> = (0..5)
        .map(|_| {
            let cu = rng.uniform(0.32, 0.68);
            let cs = rng.uniform(-0.5, 0.4);
            let phase = rng.random();
            let cycles = rng.choice(&[1, 2, 3]);
            (cu, cs, phase, cycles)
        })
        .collect();
    let (x0, y0) = log.start;
    let (x1, y1) = log.end;
    let length = py_hypot((x1 - x0) as f64, (y1 - y0) as f64);
    let (ux, uy) = ((x1 - x0) as f64 / length, (y1 - y0) as f64 / length);
    let (mut nx, mut ny) = (-uy, ux);
    if ny > 0.0 {
        // Normal points up, toward the fire light.
        (nx, ny) = (-nx, -ny);
    }
    let y_from = (y0.min(y1) as f64 - radius - 1.0) as i64;
    let y_to = (y0.max(y1) as f64 + radius + 2.0) as i64;
    let x_from = (x0.min(x1) as f64 - radius - 1.0) as i64;
    let x_to = (x0.max(x1) as f64 + radius + 2.0) as i64;
    for y in y_from..y_to {
        for x in x_from..x_to {
            let (px, py) = (x as f64 + 0.5 - x0 as f64, y as f64 + 0.5 - y0 as f64);
            let along = px * ux + py * uy;
            let across = px * nx + py * ny;
            if !(0.0 <= along && along <= length) || across.abs() > radius {
                continue;
            }
            let u = along / length;
            let s = across / radius; // +1 = top, lit side.
            let mut level = if s.abs() > 0.8 {
                if s < 0.0 { 0.0 } else { 1.0 }
            } else {
                1.0 + (s + 0.8) / 1.6 * 3.2
            };
            // Broken bark grooves running along the log.
            let row = py_round(across);
            if (row == -1 || row == 1) && py_imod(along as i64 + row * 3 + seed, 9) < 5 {
                level -= 1.0;
            }
            if (u - knot).abs() * length < 1.5 && s.abs() < 0.4 {
                level -= 1.5;
            }
            let mut palette: &[Rgb] = &BARK;
            let heat = if burning { 0.0f64.max(1.0 - (u - 0.5).abs() * 3.2) } else { 0.0 };
            if cold {
                palette = &CHAR;
                level -= 0.4;
            } else if heat > 0.0 {
                level += glow * 0.4 * heat;
                if heat > 0.35 {
                    palette = &BURNT; // Charred where the flames lick.
                }
            } else {
                level += if s > 0.2 { glow * 0.35 } else { 0.0 };
            }
            let mut color = band18(palette, level, x, y);
            // Short glowing cracks in the charred middle.
            if !cold && heat > 0.35 {
                for &(cu, cs, phase, cycles) in &cracks {
                    if (u - cu).abs() * length < 2.2 && (s - cs).abs() < 0.22 {
                        let lit = 0.5 + 0.5 * wave(t, cycles as f64, phase);
                        color = band18(&FIRE, 1.6 + lit * 2.2, x, y);
                    }
                }
            }
            canvas.put(x, y, color);
        }
    }
}

fn draw_end(canvas: &mut Canvas, x: i64, y: i64, cold: bool) {
    for (row, line) in LOG_END.iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            if key != b'.' {
                canvas.put(x + col as i64 - 3, y + row as i64 - 4, end_color(key, cold));
            }
        }
    }
}

fn draw_stone(canvas: &mut Canvas, sprite: &[&str], x: i64, y: i64, light: f64, facing: f64) {
    for (row, line) in sprite.iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            if key == b'.' {
                continue;
            }
            let level = (key - b'0') as usize;
            let mut color = STONE[level];
            // Faces turned to the fire catch its light.
            if light > 0.0 && level >= 2 {
                color = mix(color, [240, 150, 80], 0.6f64.min(light * facing * (0.25 + 0.12 * level as f64)));
            }
            canvas.put(x + col as i64, y + row as i64, color);
        }
    }
}

fn draw_pip(canvas: &mut Canvas, x: i64, y: i64, on: bool, blue: bool) {
    let color = |key: u8| -> Rgb {
        match key {
            b'1' => if on { [178, 40, 32] } else { [40, 46, 60] },
            b'2' => if on { [250, 142, 44] } else { [50, 56, 72] },
            _ => {
                if on {
                    if blue { [168, 216, 255] } else { [255, 214, 110] }
                } else {
                    [58, 64, 80]
                }
            }
        }
    };
    for (row, line) in PIP.iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            if key != b'.' {
                let (cx, cy) = (x + col as i64 * 2, y + row as i64 * 2);
                canvas.rect(cx, cy, cx + 1, cy + 1, color(key));
            }
        }
    }
}

struct Stone {
    sx: f64,
    sy: f64,
    sprite: &'static [&'static str],
    depth: f64,
    facing: f64,
}

pub fn render(data: &Data) -> String {
    render_named(data, "flame")
}

pub fn render_named(data: &Data, name: &str) -> String {
    let streak = data.streak.max(0);
    let tier = tier_of(streak);
    let cold = tier == 0;
    let height = if tier > 0 { TIERS[tier - 1].1 } else { 0 };
    let spark_count = if tier > 0 { TIERS[tier - 1].2 } else { 0 };
    let mut rng = Random::new(11);
    let noise = noise_table(&mut rng, 64, 64);
    let base = BY - 3; // The flame rises from the crossing of the logs.
    let hf = height as f64;
    // Tongues: (x offset, height share, width share, cycles, phase).
    let tongues: [(f64, f64, f64, f64, f64); 7] = [
        (0.0, 1.0, 0.95, 2.0, 0.0),
        (-0.5, 0.64, 0.6, 3.0, 0.31),
        (0.52, 0.72, 0.58, 3.0, 0.67),
        (-0.24, 0.84, 0.55, 4.0, 0.5),
        (0.28, 0.58, 0.5, 5.0, 0.12),
        (-0.7, 0.4, 0.42, 4.0, 0.8),
        (0.75, 0.38, 0.4, 5.0, 0.42),
    ];
    let width = 7.0 + hf * 0.24;
    let sparks: Vec<(f64, f64, i64, i64)> = (0..spark_count)
        .map(|_| {
            let offset = rng.uniform(-0.6, 0.6);
            let phase = rng.random();
            let cycles = rng.choice(&[1, 2]);
            let drift = rng.choice(&[-1, 1]);
            (offset, phase, cycles, drift)
        })
        .collect();
    let blobs: Vec<(f64, f64, i64)> = (0..if tier > 0 { 3 } else { 0 })
        .map(|_| {
            let offset = rng.uniform(-0.35, 0.35);
            let phase = rng.random();
            (offset, phase, rng.choice(&[2, 3]))
        })
        .collect();
    let smoke: Vec<(f64, f64)> = (0..8)
        .map(|index| {
            let phase = index as f64 / 8.0 + rng.uniform(0.0, 0.04);
            (phase, rng.uniform(0.8, 1.2))
        })
        .collect();
    let coals: Vec<(f64, f64, f64, i64)> = (0..34)
        .map(|_| {
            let dx = rng.uniform(-16.0, 16.0);
            let dy = rng.uniform(-1.5, 2.5);
            let phase = rng.random();
            (dx, dy, phase, rng.choice(&[1, 2, 3]))
        })
        .collect();

    // Stones around the fire pit, on a flat ellipse, sorted back to front.
    let mut ring: Vec<Stone> = (0..8)
        .map(|index| {
            let angle = PI * 2.0 * index as f64 / 8.0 + 0.2;
            Stone {
                sx: CX as f64 + 46.0 * angle.cos(),
                sy: (BY + 4) as f64 + 8.0 * angle.sin(),
                sprite: STONES[index % 4],
                depth: angle.sin(),
                facing: 0.6 + 0.4 * angle.sin().abs(),
            }
        })
        .collect();
    ring.sort_by(|a, b| a.sy.partial_cmp(&b.sy).unwrap());

    // Logs: two crossed, plus extra fuel on the bigger fires.
    let mut back_logs = vec![Log { start: (CX - 34, BY + 3), end: (CX + 31, BY - 15), radius: 4.2, seed: 3 }];
    if tier >= 3 {
        back_logs.insert(0, Log { start: (CX - 22, BY - 6), end: (CX + 24, BY - 4), radius: 3.4, seed: 9 });
    }
    let front_log = Log { start: (CX - 31, BY - 15), end: (CX + 35, BY + 4), radius: 4.4, seed: 5 };

    let flame_heat = |x: i64, y: i64, t: f64| -> f64 {
        let rise_px = base - y;
        if rise_px < -6 || rise_px as f64 > hf * 1.25 {
            return 0.0;
        }
        let mut best = 0.0f64;
        for &(offset, share, wide, cycles, phase) in &tongues {
            let tall = hf * share * (1.0 + 0.14 * wave(t, cycles, phase) + 0.06 * wave(t, cycles * 2.0 + 1.0, phase));
            let r = 0.0f64.max(rise_px as f64) / tall;
            if r >= 1.0 {
                continue;
            }
            let sway = (1.6 + hf * 0.06) * r.powf(1.5) * wave(t, cycles, phase - r * 0.45);
            let centre = CX as f64 + offset * width + sway;
            let profile = (1.0 - r).powf(0.9) * (0.7 + 0.3 * 1.0f64.min(r / 0.25));
            let half = width * wide * profile;
            let dx = (x as f64 + 0.5 - centre).abs();
            if dx >= half {
                continue;
            }
            let heat = (1.0 - r).powf(0.95) * (1.0 - dx / half).powf(0.6);
            best = best.max(heat);
        }
        if best <= 0.0 {
            return 0.0;
        }
        // Rising licks: the noise scrolls up 2 px per frame, two full turns per loop.
        let row = py_imod((y as f64 + t * FRAMES as f64 * 2.0) as i64, 64) as usize;
        let col = py_imod(x - CX + 32, 64) as usize;
        let lick = noise[row][col];
        best - (lick - 0.5) * 0.9 * (0.25 + rise_px as f64 / 1.max(height) as f64)
    };

    let draw = |t: f64| -> Canvas {
        let mut canvas = Canvas::new(W, H);
        panel(&mut canvas, BG, BORDER);
        let flicker = 1.0 + 0.07 * wave(t, 5.0, 0.0) + 0.04 * wave(t, 7.0, 0.3);
        let glow = if tier > 0 { (0.3 + 0.17 * tier as f64) * flicker } else { 0.0 };

        // Warm halo: three soft rings around the fire.
        if tier > 0 {
            let reach = 26.0 + hf * 0.85;
            let cy = base as f64 - hf * 0.4;
            let halo = [BG, [30, 22, 30], [44, 26, 30], [60, 30, 30]];
            for y in 2..H - 2 {
                for x in 2..118 {
                    let dist = py_hypot((x - CX) as f64 / 1.2, (y as f64 - cy) * 1.1);
                    let level = 0.0f64.max(1.0 - dist / reach) * glow * 3.2;
                    if level > 0.05 {
                        canvas.put(x, y, band(&halo, level, x, y, 0.25));
                    }
                }
            }
        }
        // Ground: a patch of packed earth and ash, lit near the fire.
        let ground = [[24, 26, 34], [36, 34, 40], [58, 42, 40], [96, 58, 42], [140, 82, 50]];
        for y in BY - 4..BY + 16 {
            for x in CX - 58..CX + 59 {
                let e = ((x - CX) as f64 / 56.0).powf(2.0) + ((y - BY - 4) as f64 / 10.0).powf(2.0);
                if e > 1.0 {
                    continue;
                }
                let light = (1.0 - e) * glow * 2.4;
                canvas.put(x, y, band18(&ground, 0.6 + (1.0 - e) * 0.9 + light, x, y));
            }
        }

        let stone_at = |canvas: &mut Canvas, stone: &Stone| {
            let x = py_round(stone.sx - stone.sprite[0].len() as f64 / 2.0);
            let y = py_round(stone.sy - stone.sprite.len() as f64 + 2.0);
            draw_stone(canvas, stone.sprite, x, y, glow, stone.facing);
        };
        for stone in ring.iter().filter(|stone| stone.depth < 0.0) {
            stone_at(&mut canvas, stone);
        }

        // Coal bed under the logs.
        for &(dx, dy, phase, cycles) in &coals {
            let (x, y) = (py_round(CX as f64 + dx), py_round(BY as f64 + dy));
            let lit = 0.5 + 0.5 * wave(t, cycles as f64, phase);
            let color = if cold {
                if phase > 0.45 {
                    band18(&[CHAR[1], [76, 30, 28], [130, 40, 30]], lit * 1.6 * phase, x, y)
                } else {
                    CHAR[2]
                }
            } else {
                band18(&FIRE, 1.0 + lit * 2.2 + (1.0 - dx.abs() / 16.0) * 1.4, x, y)
            };
            canvas.put(x, y, color);
            canvas.put(x + 1, y, mix(color, CHAR[0], 0.45));
        }

        for log in &back_logs {
            draw_log(&mut canvas, log, glow, cold, t);
        }
        let last = back_logs.last().unwrap();
        draw_end(&mut canvas, last.end.0 + 1, last.end.1, cold);

        if tier > 0 {
            let top = 2.max(py_round(base as f64 - hf * 1.25));
            for y in top..base + 2 {
                for x in py_round(CX as f64 - width * 1.9)..=py_round(CX as f64 + width * 1.9) {
                    let heat = flame_heat(x, y, t);
                    if heat <= 0.05 {
                        continue;
                    }
                    canvas.put(x, y, band18(&FIRE, heat * 6.0, x, y));
                }
            }
            // 100+ days: a blue heart at the root of the flame.
            if tier == 4 {
                let heart = [[255, 236, 190], BLUE[0], BLUE[1]];
                let tall = hf * 0.36 * (1.0 + 0.08 * wave(t, 3.0, 0.2));
                for y in py_round(base as f64 - tall)..base + 2 {
                    let r = 1.0f64.min(0.0f64.max((base - y) as f64 / tall));
                    let centre = CX as f64 + 1.5 * r * r * wave(t, 2.0, -r * 0.4);
                    let half = width * 0.4 * (1.0 - r).powf(0.55) * (0.6 + 0.4 * 1.0f64.min(r / 0.25));
                    for x in py_round(centre - half) - 1..py_round(centre + half) + 2 {
                        let d = (x as f64 + 0.5 - centre).abs() / 0.5f64.max(half);
                        if d < 1.0 {
                            canvas.put(x, y, band18(&heart, (1.0 - d) * 2.4 + (1.0 - r) * 0.6, x, y));
                        }
                    }
                }
            }
            // Flame bits that break off the tips and rise.
            for &(offset, phase, cycles) in &blobs {
                let p = py_mod(t * cycles as f64 + phase, 1.0);
                let y = base as f64 - hf * (0.85 + 0.5 * p);
                let x = CX as f64 + offset * width + 2.5 * wave(p, 1.0, 0.0);
                let size = 1.7 * (1.0 - p);
                if size > 0.5 {
                    let color = if p < 0.4 { FIRE[3] } else if p < 0.75 { FIRE[2] } else { FIRE[1] };
                    canvas.disc(x, y, size, color);
                }
            }
            for &(offset, phase, cycles, drift) in &sparks {
                let p = py_mod(t * cycles as f64 + phase, 1.0);
                let x = CX as f64 + offset * width * 1.4 + (drift * 7) as f64 * p + 1.5 * wave(p * 2.0, 1.0, phase);
                let y = base as f64 - hf * 0.55 - p * (30.0 + hf * 0.55);
                if y < 3.0 {
                    continue;
                }
                let color = if p < 0.25 {
                    FIRE[5]
                } else if p < 0.55 {
                    FIRE[4]
                } else if p < 0.8 {
                    FIRE[3]
                } else {
                    FIRE[1]
                };
                canvas.put(x, y, color);
            }
        } else {
            // Thin trail of smoke from the dead fire: a wavy ribbon that breaks into puffs.
            for step in 0..70i64 {
                let p = step as f64 / 70.0;
                let y = BY - 6 - step;
                let x = (CX + 1) as f64 + 10.0 * p * p + (1.0 + 3.0 * p) * wave(p * 1.5 - t, 1.0, 0.0);
                if y < 3 {
                    break;
                }
                let fade = 1.0 - p;
                let thick = 0.6 + p * 1.6;
                for xx in (x - thick).floor() as i64..=(x + thick).ceil() as i64 {
                    if (xx as f64 - x).abs() > thick {
                        continue;
                    }
                    if py_imod(xx + y, 2) == 0 {
                        canvas.put(xx, y, mix([48, 54, 66], [136, 142, 154], fade));
                    }
                }
            }
            for &(phase, size) in &smoke[..4] {
                let p = py_mod(t * 2.0 + phase * 2.0, 1.0);
                let y = (BY - 20) as f64 - p * 50.0;
                let x = (CX + 1) as f64 + 10.0 * ((y - BY as f64 + 6.0) / -70.0).powf(2.0) + 4.0 * wave(p, 1.0, phase);
                let radius = (1.2 + p * 2.6) * size;
                let fade = 1.0 - p;
                for yy in (y - radius).floor() as i64..=(y + radius).ceil() as i64 {
                    for xx in (x - radius).floor() as i64..=(x + radius).ceil() as i64 {
                        if (xx as f64 - x).powf(2.0) + (yy as f64 - y).powf(2.0) > radius * radius || yy < 3 {
                            continue;
                        }
                        if py_imod(xx + yy, 2) == 0 || fade > 0.7 {
                            canvas.put(xx, yy, mix([40, 46, 58], [110, 116, 128], fade * 0.8));
                        }
                    }
                }
            }
        }

        // Mound of embers (or ash) inside the crossing, under the flame.
        for y in base - 2..BY + 3 {
            let half = 6.0 + (y - base + 2) as f64 * 1.6;
            for x in py_round(CX as f64 - half)..=py_round(CX as f64 + half) {
                let n = noise[py_imod(y * 7, 64) as usize][py_imod(x * 3, 64) as usize];
                let edge = (x - CX).abs() as f64 / half;
                let color = if cold {
                    let level = 1.2 + n * 2.0 - edge;
                    let mut color = band18(&CHAR, level, x, y);
                    if n > 0.56 && edge < 0.7 {
                        color = band18(
                            &[CHAR[1], [90, 32, 28], [150, 48, 30]],
                            (0.5 + 0.5 * wave(t, 1.0, n * 3.0)) * 2.0,
                            x,
                            y,
                        );
                    }
                    color
                } else {
                    let pulse = 0.5 + 0.5 * wave(t, (2 + (n * 3.0) as i64) as f64, n * 5.0);
                    let level =
                        1.4 + (1.0 - edge) * 1.6 + n * 1.2 + pulse * 0.6 - (y - base) as f64 * 0.08;
                    band18(&FIRE, level, x, y)
                };
                canvas.put(x, y, color);
            }
        }

        draw_log(&mut canvas, &front_log, glow, cold, t);
        draw_end(&mut canvas, front_log.start.0 - 1, front_log.start.1, cold);
        draw_end(&mut canvas, front_log.end.0 + 1, front_log.end.1, cold);
        draw_end(&mut canvas, last.start.0 - 1, last.start.1, cold);

        for stone in ring.iter().filter(|stone| stone.depth >= 0.0) {
            stone_at(&mut canvas, stone);
        }

        // Number and labels.
        let number = streak.to_string();
        let scale = if text_width(&number, Font::F57, 4) <= 80 { 4 } else { 3 };
        let right = 163.0;
        let top_y = if scale == 4 { 20.0 } else { 24.0 };
        let (color, shadow) = if tier > 0 { (TEXT_WARM, [90, 32, 22]) } else { (TEXT_COLD, [34, 38, 50]) };
        canvas.text_center(right, top_y, &number, color, Font::F57, scale, Some(shadow));
        canvas.text_center(right, 58.0, if streak == 1 { "DAY" } else { "DAYS" }, TEXT, Font::F57, 2, None);
        canvas.text_center(right, 76.0, "STREAK", MUTED, Font::F57, 2, None);
        // Four pips: the tiers reached (1, 7, 30 and 100 days).
        for index in 0..4 {
            draw_pip(&mut canvas, 163 - 34 + index as i64 * 18, 98, index < tier, index == 3);
        }
        canvas
    };

    let frames: Vec<Canvas> = (0..FRAMES).map(|index| draw(index as f64 / FRAMES as f64)).collect();
    save(&frames, name, Save { still: Some(0), ..Save::default() })
}
