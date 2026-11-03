//! Port of the Python prototype. Fireworks over a lake: one rocket per pull request merged in
//! the last 7 days (`render`), or one rocket per N commits of the last 7 days (`render_commits`).
//! Same draw order, same random draws, same float formulas as the Python.

use std::f64::consts::PI;

use indexmap::IndexMap;

use crate::data::{Data, LanguageCount, MergedPr};
use crate::export::{save, Save};
use crate::pixel::{dither_on, mix, rgb, text_width, Canvas, Font, Rgb, FRAMES};
use crate::pymath::{py_hypot, py_imod, py_mod, py_round, py_sum};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 176;
const WATER: i64 = 130; // First row of the lake.
const MAX_ROCKETS: usize = 24;
const LAUNCH: f64 = 0.11; // Share of the loop spent rising.
const BURST: f64 = 0.42; // Share of the loop the sparks live.

const SKY: [Rgb; 5] = [[6, 7, 20], [10, 12, 32], [16, 18, 46], [26, 26, 62], [38, 34, 76]];
const STAR: [Rgb; 2] = [[120, 126, 170], [210, 214, 240]];
const MOON: [Rgb; 2] = [[240, 236, 210], [196, 190, 170]];
const TOWN: Rgb = [10, 11, 24];
const TOWN_EDGE: Rgb = [22, 22, 44];
const HILL: Rgb = [13, 15, 30];
const WINDOW: [Rgb; 3] = [[255, 196, 96], [226, 150, 70], [150, 96, 60]];
const RED_LIGHT: Rgb = [255, 70, 60];
const LAKE: Rgb = [8, 12, 30];
const SMOKE: Rgb = [70, 72, 104];
const TEXT: Rgb = [240, 240, 250];
const MUTED: Rgb = [160, 164, 200];
const SHADOW: Rgb = [2, 3, 10];
const WHITE: Rgb = [255, 255, 255];
const GOLD: Rgb = [255, 214, 140];
const OTHER: Rgb = [190, 190, 205];

const KINDS: [&str; 9] = ["peony", "willow", "ring", "crackle", "chrysanthemum", "peony", "ring", "willow", "crackle"];
const COMMIT_STEPS: [i64; 13] = [1, 2, 5, 10, 20, 25, 50, 100, 200, 500, 1000, 2000, 5000];
const MAX_COMMIT_SHELLS: i64 = 20;

// Town silhouette, hand-drawn row by row from the waterline up: (x0, x1, top) blocks.
const BUILDINGS: [(i64, i64, i64); 17] = [
    (150, 159, 112), (160, 167, 106), (168, 177, 114), (178, 183, 100), (184, 193, 109),
    (194, 201, 116), (202, 209, 104), (210, 221, 111), (222, 227, 96), (228, 237, 108),
    (238, 247, 113), (248, 255, 102), (256, 265, 110), (266, 273, 117), (274, 283, 107),
    (284, 291, 112), (292, 299, 119),
];
// Church spire on the 178-183 block, water tower on the left, radio mast on 222-227.
const SPIRE: [&str; 5] = ["..o..", "..o..", ".ooo.", ".ooo.", "ooooo"];
const TOWER: [&str; 9] = [".ooooo.", "ooooooo", "ooooooo", ".ooooo.", ".o...o.", ".o...o.", ".oo.oo.", ".o...o.", ".o...o."];
const BOAT: [&str; 7] = [
    "......l.......",
    "......p.......",
    "..o..ppp......",
    "..o..ppp......",
    "hhhhhhhhhhhhhh",
    ".hhhhhhhhhhhh.",
    "..hhhhhhhhhh..",
];

/// Pull request mode (`fireworks`).
pub fn render(data: &Data) -> String {
    render_mode(data, false)
}

/// Commit mode (`fireworks-commits`).
pub fn render_commits(data: &Data) -> String {
    render_mode(data, true)
}

/// Language name as Python reads it: `pr.get("language") or "Other"`.
fn name_of(name: Option<&str>) -> String {
    match name {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "Other".to_string(),
    }
}

/// Language colour as Python reads it: `rgb(color) if color else OTHER`.
fn color_of(color: Option<&str>) -> Rgb {
    match color {
        Some(hex) if !hex.is_empty() => rgb(hex),
        _ => OTHER,
    }
}

/// Upper-cased, first ten characters: `lang.upper()[:10]`.
fn upper10(text: &str) -> String {
    text.to_uppercase().chars().take(10).collect()
}

/// Lift dark language colours (C, Ruby...) so their burst still reads on the night sky.
fn bright_enough(color: Rgb) -> Rgb {
    let lum = 0.299 * color[0] as f64 + 0.587 * color[1] as f64 + 0.114 * color[2] as f64;
    if lum >= 120.0 {
        return color;
    }
    mix(color, WHITE, 0.6f64.min((120.0 - lum) / 160.0))
}

/// Pull requests merged in the 7 days ending on the last day of the calendar, oldest first.
fn recent_prs(data: &Data) -> Vec<&MergedPr> {
    let last = match data.year_days().last() {
        Some(day) => crate::dates::day_number(&day.date),
        None => crate::dates::today(),
    };
    // since = last + 1 day - 7 days.
    let since = last - 6;
    let mut prs: Vec<&MergedPr> =
        data.merged_prs.iter().filter(|pr| crate::dates::day_number(&pr.merged_at[..10]) >= since).collect();
    prs.sort_by(|a, b| a.merged_at.cmp(&b.merged_at));
    prs
}

/// One shell per N commits of a language, N picked from round steps to keep at most 20 shells.
/// A language's leftover commits (fewer than N) still fire, as a smaller burst sized by the share
/// of N they stand for. Returns the shells (colour, size) and N.
fn commit_shells(languages: &[LanguageCount]) -> (Vec<(Rgb, f64)>, i64) {
    let mut active: Vec<&LanguageCount> = languages.iter().filter(|lang| lang.count > 0).collect();
    let step = COMMIT_STEPS
        .iter()
        .copied()
        .find(|&n| active.iter().map(|lang| (lang.count + n - 1) / n).sum::<i64>() <= MAX_COMMIT_SHELLS)
        .unwrap_or(COMMIT_STEPS[COMMIT_STEPS.len() - 1]);
    // Stable sort by descending count, as Python's sorted(key=-count).
    active.sort_by(|a, b| b.count.cmp(&a.count));
    let queues: Vec<Vec<(Rgb, f64)>> = active
        .iter()
        .map(|lang| {
            let color = color_of(lang.color.as_deref());
            let (full, rest) = (lang.count / step, lang.count % step);
            let mut sizes = vec![1.0f64; full as usize];
            if rest > 0 {
                sizes.push(0.45f64.max((rest as f64 / step as f64).sqrt()));
            }
            sizes.into_iter().map(|size| (color, size)).collect()
        })
        .collect();
    // Round robin over languages so colours alternate in the sky.
    let mut shells = Vec::new();
    let mut cursor = vec![0usize; queues.len()];
    loop {
        let mut taken = false;
        for (q, queue) in queues.iter().enumerate() {
            if let Some(&shell) = queue.get(cursor[q]) {
                shells.push(shell);
                cursor[q] += 1;
                taken = true;
            }
        }
        if !taken {
            break;
        }
    }
    (shells, step)
}

struct Rocket {
    start: f64,
    x: f64,
    launch_x: f64,
    peak: f64,
    kind: &'static str,
    color: Rgb,
    core: Rgb,
    radius: f64,
    /// (x, y, speed) of each spark, on a unit sphere or ring.
    sparks: Vec<(f64, f64, f64)>,
}

/// shells: list of (colour, size), size 1 = a full burst.
fn plan_rockets(shells: &[(Rgb, f64)]) -> Vec<Rocket> {
    let mut rng = Random::new(7);
    let count = shells.len().min(MAX_ROCKETS);
    let shrink = if count <= 6 { 1.0 } else { 0.7f64.max(1.0 - (count - 6) as f64 * 0.02) };
    let mut rockets = Vec::new();
    for (index, &(shell_color, size)) in shells[shells.len() - count..].iter().enumerate() {
        let color = bright_enough(shell_color);
        let kind = KINDS[index % KINDS.len()];
        // Golden-ratio spread keeps bursts from piling up on one spot.
        let x = 46.0 + py_mod(index as f64 * 0.618 + 0.27, 1.0) * 308.0;
        let start = index as f64 / count.max(1) as f64 + rng.uniform(-0.02, 0.02);
        let launch_x = x + rng.uniform(-10.0, 10.0);
        let peak = rng.uniform(58.0, 84.0);
        let base = match kind {
            "willow" => 44.0,
            "peony" | "chrysanthemum" => 40.0,
            _ => 34.0,
        };
        let radius = base * shrink * size;
        let mut rocket = Rocket {
            start,
            x,
            launch_x,
            peak,
            kind,
            color,
            core: mix(color, WHITE, 0.65),
            radius,
            sparks: Vec::new(),
        };
        if kind == "ring" {
            let tilt = rng.uniform(0.72, 0.95);
            let turn = rng.uniform(0.0, PI);
            for (n, speed) in [(44usize, 1.0f64), (22usize, 0.5f64)] {
                for k in 0..n {
                    let angle = 2.0 * PI * k as f64 / n as f64;
                    let (ux, uy) = (angle.cos(), angle.sin() * tilt);
                    rocket.sparks.push((
                        ux * turn.cos() - uy * turn.sin(),
                        ux * turn.sin() + uy * turn.cos(),
                        speed,
                    ));
                }
            }
        } else {
            let n = match kind {
                "peony" => 96,
                "willow" => 60,
                "crackle" => 56,
                _ => 70,
            };
            for _ in 0..n {
                // Points on a sphere seen from the side: denser rim, filled centre.
                let z = rng.uniform(-1.0, 1.0);
                let angle = rng.uniform(0.0, 2.0 * PI);
                let spread = (1.0 - z * z).sqrt();
                let speed = rng.uniform(0.9, 1.05);
                rocket.sparks.push((angle.cos() * spread, angle.sin() * spread, speed));
            }
        }
        rockets.push(rocket);
    }
    rockets
}

fn spark_position(rocket: &Rocket, sx: f64, sy: f64, speed: f64, age: f64) -> (f64, f64) {
    let willow = rocket.kind == "willow";
    let tau = if willow { 9.0 } else { 5.0 };
    let gravity = if willow { 0.05 } else { 0.022 };
    let reach = rocket.radius * speed * (1.0 - (-age / tau).exp());
    (rocket.x + sx * reach, rocket.peak + sy * reach + gravity * age.powf(2.0))
}

fn draw_burst(canvas: &mut Canvas, rocket: &Rocket, age: f64, frame: i64, glows: &mut Vec<(f64, f64, Rgb, f64)>) {
    let life = BURST * FRAMES as f64;
    if age < 0.0 || age >= life {
        return;
    }
    let p = age / life;
    let kind = rocket.kind;
    let color = rocket.color;
    if age < 8.0 {
        glows.push((rocket.x, rocket.peak, color, 1.0 - age / 8.0));
    }
    if age < 3.0 {
        canvas.disc(rocket.x, rocket.peak, 4.0 - age * 1.2, WHITE);
    }
    let trail: i64 = match kind {
        "willow" => 12,
        "chrysanthemum" => 6,
        "peony" => 3,
        _ => 2,
    };
    for (index, &(sx, sy, speed)) in rocket.sparks.iter().enumerate() {
        // Burning out: sparks flicker, then thin out.
        if p > 0.55 && py_imod(index as i64 * 7 + frame, 3) == 0 {
            continue;
        }
        let (x, y) = spark_position(rocket, sx, sy, speed, age);
        if p > 0.78 && !dither_on(py_round(x), py_round(y), (1.0 - p) * 4.5) {
            continue;
        }
        if kind == "crackle" && p > 0.4 {
            // Each spark pops into a white glitter cross at its own moment.
            let pop = 0.4 + py_mod(index as f64 * 0.37, 1.0) * 0.4;
            if pop <= p && p < pop + 0.07 {
                for (dx, dy) in [(0i64, 0i64), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let colour = if (dx, dy) == (0, 0) { WHITE } else { GOLD };
                    canvas.put(x + dx as f64, y + dy as f64, colour);
                }
            }
            if p >= pop {
                continue;
            }
        }
        // Tail: segments back along the path, fading into the sky.
        let mut prev = (x, y);
        for back in 1..=trail {
            let a = age - back as f64;
            if a < 0.0 {
                break;
            }
            let (tx, ty) = spark_position(rocket, sx, sy, speed, a);
            let fade = back as f64 / (trail + 1) as f64;
            let tone = if kind == "willow" {
                mix(mix(GOLD, color, 0.75), SKY[2], 0.2 + fade * 0.7)
            } else {
                mix(color, SKY[2], 0.25 + fade * 0.6)
            };
            if kind != "willow" || dither_on(py_round(tx), py_round(ty), 1.0 - fade * 0.6) {
                canvas.line(prev.0, prev.1, tx, ty, tone);
            }
            prev = (tx, ty);
        }
        let head = if age < 3.0 {
            rocket.core
        } else if kind == "willow" && p > 0.4 {
            mix(color, GOLD, 0.3)
        } else if p < 0.3 {
            mix(color, WHITE, 0.3)
        } else {
            color
        };
        canvas.put(x, y, head);
    }
}

/// The rising shell: a hot head and a sputtering gold tail.
fn draw_rocket(canvas: &mut Canvas, rocket: &Rocket, u: f64, frame: i64) {
    let rise = 1.0 - (1.0 - u / LAUNCH).powf(2.0);
    let base = (WATER - 4) as f64;
    let x = rocket.launch_x + (rocket.x - rocket.launch_x) * rise;
    let y = base + (rocket.peak - base) * rise;
    canvas.put(x, y, WHITE);
    canvas.put(x, y + 1.0, GOLD);
    for k in 2..9i64 {
        if py_imod(k + frame, 3) != 0 {
            let back = rise - k as f64 * 0.012;
            let tx = rocket.launch_x + (rocket.x - rocket.launch_x) * back;
            let wobble = (py_imod(k * 5 + frame, 3) - 1) as f64 * 0.6;
            canvas.put(tx + wobble, y + k as f64, mix(GOLD, SKY[3], k as f64 / 9.0));
        }
    }
}

fn build_background() -> Canvas {
    let mut canvas = Canvas::new(W, H);
    for y in 0..WATER {
        let level = (SKY.len() as f64 - 1.001).min((y as f64 / WATER as f64).powf(1.2) * (SKY.len() - 1) as f64);
        let low = level as i64;
        for x in 0..W {
            let colour = if dither_on(x, y, level - low as f64) { SKY[(low + 1) as usize] } else { SKY[low as usize] };
            canvas.put(x, y, colour);
        }
    }
    // A faint milky way across the sky, a haze with tiny stars.
    let mut band = Random::new(8);
    for y in 0..110i64 {
        for x in 0..W {
            let d = ((y - 100) as f64 + x as f64 * 0.22).abs() / 22.0;
            if d < 1.0 {
                if dither_on(x, y, (1.0 - d) * 0.22) {
                    let colour = mix(canvas.get(x, y), [60, 58, 108], 0.3);
                    canvas.put(x, y, colour);
                }
                if band.random() < (1.0 - d) * 0.018 {
                    canvas.put(x, y, [96, 98, 146]);
                }
            }
        }
    }
    // Moon with a soft halo.
    let (mx, my) = (236i64, 20i64);
    for y in my - 12..=my + 12 {
        for x in mx - 12..=mx + 12 {
            let d = py_hypot((x - mx) as f64, (y - my) as f64);
            if 5.0 < d && d < 11.0 && dither_on(x, y, (11.0 - d) / 6.0 * 0.4) {
                let colour = mix(canvas.get(x, y), MOON[0], 0.15);
                canvas.put(x, y, colour);
            }
            if d <= 5.2 && py_hypot((x - mx) as f64 - 2.6, (y - my) as f64 + 1.4) > 4.4 {
                canvas.put(x, y, if d < 4.3 { MOON[0] } else { MOON[1] });
            }
        }
    }
    // The town's lights warm the low sky above it.
    for y in 78..WATER {
        for x in 90..360i64 {
            let glow = 0.0f64.max(1.0 - py_hypot((x - 222) as f64 / 130.0, (y - WATER) as f64 / 44.0));
            if glow > 0.0 && dither_on(x, y, glow * 0.9) {
                let colour = mix(canvas.get(x, y), [96, 52, 84], 0.45 * glow + 0.15);
                canvas.put(x, y, colour);
            }
        }
    }
    // Hills on both sides, with a row of pines on the left one.
    for x in 0..W {
        let left = if x < 150 { 112.0 + 0.0016 * ((x - 20) * (x - 20)) as f64 } else { 999.0 };
        let right = if x > 280 { 116.0 + 0.0022 * ((x - 380) * (x - 380)) as f64 } else { 999.0 };
        let top = py_round(left.min(right).min(WATER as f64));
        canvas.rect(x, top, x, WATER - 1, HILL);
        if top < WATER - 1 {
            canvas.put(x, top, TOWN_EDGE);
        }
    }
    let mut rng = Random::new(5);
    for x in (4i64..132).step_by(5).chain((304i64..398).step_by(6)) {
        let ground = if x < 150 {
            112.0 + 0.0016 * ((x - 20) * (x - 20)) as f64
        } else {
            116.0 + 0.0022 * ((x - 380) * (x - 380)) as f64
        };
        let h = rng.randint(6, 12);
        for k in 0..h {
            let half = (k as f64 / h as f64) * 2.6;
            let row = ground - h as f64 + k as f64;
            canvas.rect(x as f64 - half, row, x as f64 + half, row, HILL);
        }
    }
    // Town: blocks, windows, spire, water tower, radio mast.
    for &(x0, x1, top) in &BUILDINGS {
        canvas.rect(x0, top, x1, WATER - 1, TOWN);
        canvas.rect(x0, top, x1, top, TOWN_EDGE);
    }
    for &(x0, x1, top) in &BUILDINGS {
        for wy in (top + 3..WATER - 3).step_by(4) {
            for wx in (x0 + 2..x1 - 1).step_by(3) {
                let roll = rng.random();
                if roll < 0.32 {
                    let colour = if roll < 0.12 {
                        WINDOW[0]
                    } else if roll < 0.24 {
                        WINDOW[1]
                    } else {
                        WINDOW[2]
                    };
                    canvas.rect(wx, wy, wx, wy + 1, colour);
                }
            }
        }
    }
    for (dy, row) in SPIRE.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            if key == b'o' {
                canvas.put(178 + dx as i64, 95 + dy as i64, TOWN);
            }
        }
    }
    canvas.rect(180, 86, 180, 94, TOWN);
    canvas.rect(179, 88, 181, 88, TOWN);
    for (dy, row) in TOWER.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            if key == b'o' {
                canvas.put(136 + dx as i64, 104 + dy as i64, TOWN);
            }
        }
    }
    canvas.rect(136, 113, 150, WATER - 1, TOWN);
    canvas.rect(224, 78, 224, 96, TOWN);
    canvas.rect(223, 84, 225, 84, TOWN);
    canvas.rect(222, 90, 226, 90, TOWN);
    // Quay along the water.
    canvas.rect(130, WATER - 2, 304, WATER - 1, TOWN_EDGE);
    canvas
}

/// The lake mirrors the sky and the town, rippling and darker with depth.
fn draw_reflection(canvas: &mut Canvas, t: f64) {
    for k in 0..(H - WATER) {
        let y = WATER + k;
        let src_y = WATER - 1 - py_round(k as f64 * 1.15);
        if src_y < 0 {
            break;
        }
        let depth = k as f64 / (H - WATER) as f64;
        let ripple = py_round((0.6 + 2.2 * depth) * (2.0 * PI * 2.0 * t + k as f64 * 0.9).sin());
        let darken = 0.32 + depth * 0.33;
        for x in 0..W {
            let sx = (x + ripple).max(0).min(W - 1);
            let mut color = mix(canvas.get(sx, src_y), LAKE, darken);
            // Thin horizontal glints break the mirror into lines of water.
            if py_imod(k + x / 23, 6) == 0 && dither_on(x, y, 0.5) {
                color = mix(color, LAKE, 0.5);
            }
            canvas.put(x, y, color);
        }
    }
    canvas.rect(0, WATER, W - 1, WATER, mix(TOWN_EDGE, LAKE, 0.3));
}

/// A puff that grows, drifts right and thins out until the rocket launches again.
fn draw_smoke(canvas: &mut Canvas, rocket: &Rocket, u: f64) {
    if u < LAUNCH + 0.04 {
        return;
    }
    let s = (u - LAUNCH - 0.04) / (1.0 - LAUNCH - 0.04);
    let density = 0.42 * (1.0 - s).powf(1.4);
    let radius = 9.0 + 16.0 * s.sqrt();
    let cx = rocket.x + 20.0 * s;
    let cy = rocket.peak + 4.0 + 10.0 * s;
    let tint = mix(SMOKE, rocket.color, 0.18 * (1.0 - s));
    for y in (cy - radius * 0.7).floor() as i64..=(cy + radius * 0.7).ceil() as i64 {
        if y >= WATER - 6 || y < 0 {
            continue;
        }
        for x in (cx - radius).floor() as i64..=(cx + radius).ceil() as i64 {
            if !(0..W).contains(&x) {
                continue;
            }
            let nx = (x as f64 - cx) / radius;
            let ny = (y as f64 - cy) / (radius * 0.7);
            let d = nx * nx + ny * ny;
            if d > 1.0 {
                continue;
            }
            let lump = 0.75 + 0.25 * (x as f64 * 0.35 + y as f64 * 0.5 + rocket.x).sin();
            if dither_on(x, y, density * (1.0 - d) * lump * 1.6) {
                let colour = mix(canvas.get(x, y), tint, 0.35);
                canvas.put(x, y, colour);
            }
        }
    }
}

fn sky_glow(canvas: &mut Canvas, rocket: &Rocket, power: f64) {
    let (cx, cy, radius) = (rocket.x, rocket.peak, 58.0);
    for y in 0i64.max(py_round(cy - radius))..WATER.min(py_round(cy + radius)) {
        for x in 0i64.max(py_round(cx - radius))..W.min(py_round(cx + radius)) {
            let d = py_hypot(x as f64 - cx, y as f64 - cy) / radius;
            if d < 1.0 && canvas.get(x, y) != TOWN && dither_on(x, y, (1.0 - d) * power * 0.9) {
                let ratio = 0.22 * (1.0 - d) + 0.06;
                let colour = mix(canvas.get(x, y), rocket.color, ratio);
                canvas.put(x, y, colour);
            }
        }
    }
}

/// A rowing boat with a lantern, rocking slowly.
fn draw_boat(canvas: &mut Canvas, x: i64, y: i64, frame: i64) {
    for (dy, row) in BOAT.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            let (px, py) = (x + dx as i64, y + dy as i64);
            match key {
                b'h' => canvas.put(px, py, if dy == 4 { [74, 70, 116] } else { TOWN }),
                b'p' | b'o' => canvas.put(px, py, TOWN),
                b'l' => canvas.put(px, py, WINDOW[0]),
                _ => {}
            }
        }
    }
    // Lantern halo and its streak on the water.
    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1)] {
        let (px, py) = (x + 6 + dx, y + dy);
        let colour = mix(canvas.get(px, py), WINDOW[1], 0.5);
        canvas.put(px, py, colour);
    }
    for k in 1..7i64 {
        if (k + frame / 4) % 2 != 0 {
            canvas.put(x + 6 + (k % 3 - 1), y + 7 + k * 2, mix(WINDOW[1], LAKE, 0.3 + k as f64 * 0.08));
        }
    }
}

/// Short moonlit dashes on the lake, widening with depth and sliding as the water moves.
fn draw_moon_glitter(canvas: &mut Canvas, frame: i64) {
    let mut rng = Random::new(frame.div_euclid(2));
    for k in (3..H - WATER).step_by(3) {
        let y = WATER + k;
        let spread = 2 + k / 5;
        for _ in 0..2 {
            let x = 236 + rng.randint(-spread, spread);
            let length = rng.randint(1, 2 + k / 12);
            let colour = mix(MOON[1], LAKE, 0.3 + k as f64 / 110.0);
            canvas.rect(x, y, x + length, y, colour);
        }
    }
}

fn render_mode(data: &Data, commits: bool) -> String {
    let mut tally: IndexMap<String, (Rgb, i64)> = IndexMap::new();
    let (shells, title, subtitle, name) = if commits {
        let languages = &data.commits_7d;
        let (shells, step) = commit_shells(languages);
        // Later duplicates overwrite the value and keep the first position, as a Python dict does.
        for lang in languages.iter().filter(|lang| lang.count > 0) {
            tally.insert(name_of(Some(&lang.name)), (color_of(lang.color.as_deref()), lang.count));
        }
        let count: i64 = languages.iter().map(|lang| lang.count).sum();
        let title = if count == 1 { format!("{count} COMMIT") } else { format!("{count} COMMITS") };
        let subtitle = if count == 0 {
            "LAST 7 DAYS".to_string()
        } else if step == 1 {
            "LAST 7 DAYS - 1 BURST EACH".to_string()
        } else {
            format!("LAST 7 DAYS - {step} PER BURST")
        };
        (shells, title, subtitle, "fireworks-commits")
    } else {
        let prs = recent_prs(data);
        let shells: Vec<(Rgb, f64)> = prs.iter().map(|pr| (color_of(pr.color.as_deref()), 1.0)).collect();
        for pr in &prs {
            let name = name_of(pr.language.as_deref());
            let color = color_of(pr.color.as_deref());
            tally.entry(name).or_insert((color, 0)).1 += 1;
        }
        let count = prs.len() as i64;
        let title = if count == 1 { format!("{count} PR MERGED") } else { format!("{count} PRS MERGED") };
        (shells, title, "LAST 7 DAYS".to_string(), "fireworks")
    };

    let rockets = plan_rockets(&shells);
    let background = build_background();
    let mut rng = Random::new(3);
    let stars: Vec<(i64, i64, i64)> = (0..90)
        .map(|_| {
            let x = rng.randrange_to(W);
            let y = rng.randrange_to(96);
            let speed = rng.choice(&[1i64, 2, 2, 4]);
            (x, y, speed)
        })
        .collect();
    let windows_blink: Vec<((i64, i64, i64), i64)> = (0..6)
        .map(|_| {
            let block = rng.choice(&BUILDINGS);
            let when = rng.randrange_to(64);
            (block, when)
        })
        .collect();

    // Legend: the four biggest languages, sorted by count (stable: ties keep insertion order).
    let mut ranked: Vec<(String, Rgb, i64)> =
        tally.into_iter().map(|(lang, (color, n))| (lang, color, n)).collect();
    ranked.sort_by(|a, b| b.2.cmp(&a.2));
    let hidden = ranked.len() as i64 - 4;
    let legend: Vec<(String, Rgb, i64)> = ranked.iter().take(4).cloned().collect();
    let widest = legend
        .iter()
        .map(|(lang, _, n)| {
            text_width(&upper10(lang), Font::F35, 2) + 8 + text_width(&n.to_string(), Font::F35, 2)
        })
        .fold(0, i64::max);
    let legend_x = W - 9 - widest - 12;

    let mut frames: Vec<Canvas> = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES as i64 {
        let t = frame as f64 / FRAMES as f64;
        let mut canvas = Canvas::new(W, H);
        canvas.pixels.copy_from_slice(&background.pixels);
        for &(x, y, speed) in &stars {
            if SKY.contains(&canvas.get(x, y)) {
                let twinkle = (2.0 * PI * t * speed as f64 + x as f64 * 0.7).sin();
                if twinkle > -0.2 {
                    let colour = if twinkle > 0.7 && py_imod(x + y, 3) == 0 { STAR[1] } else { STAR[0] };
                    canvas.put(x, y, colour);
                }
            }
        }
        // A shooting star crosses once per loop when the sky is quiet.
        let shooting: &[(f64, i64, i64)] = if rockets.is_empty() {
            &[(0.3, 40, 44), (0.78, 250, 50)]
        } else {
            &[(0.3, 40, 44)]
        };
        for &(begin, x0, y0) in shooting {
            if begin <= t && t < begin + 0.1 {
                let s = (t - begin) / 0.1;
                for k in 0..16i64 {
                    canvas.put(
                        x0 as f64 + s * 120.0 - (k * 2) as f64,
                        y0 as f64 + s * 26.0 - k as f64 * 0.45,
                        mix(WHITE, SKY[2], k as f64 / 16.0),
                    );
                }
            }
        }
        // A fresh burst briefly lights the sky around it.
        for rocket in &rockets {
            let age = (py_mod(t - rocket.start, 1.0) - LAUNCH) * FRAMES as f64;
            if 0.0 <= age && age < 8.0 {
                sky_glow(&mut canvas, rocket, 1.0 - age / 8.0);
            }
        }
        for rocket in &rockets {
            draw_smoke(&mut canvas, rocket, py_mod(t - rocket.start, 1.0));
        }
        let mut glows: Vec<(f64, f64, Rgb, f64)> = Vec::new();
        for rocket in &rockets {
            let u = py_mod(t - rocket.start, 1.0);
            if u < LAUNCH {
                draw_rocket(&mut canvas, rocket, u, frame);
            } else {
                draw_burst(&mut canvas, rocket, (u - LAUNCH) * FRAMES as f64, frame, &mut glows);
            }
        }
        // Fresh bursts light up the roofs below them.
        for &(gx, _gy, color, power) in &glows {
            for &(x0, x1, top) in &BUILDINGS {
                if ((x0 + x1) as f64 / 2.0 - gx).abs() < 60.0 {
                    for x in x0..=x1 {
                        let colour = mix(TOWN_EDGE, color, 0.6 * power);
                        canvas.put(x, top, colour);
                    }
                }
            }
        }
        // Radio mast light and a few windows switching on and off.
        if frame % 16 < 8 {
            canvas.put(224, 77, RED_LIGHT);
        }
        for &((x0, _x1, top), when) in &windows_blink {
            let on = (frame + when) % 64 < 40;
            canvas.rect(x0 + 2, top + 3, x0 + 2, top + 4, if on { WINDOW[0] } else { TOWN });
        }
        draw_reflection(&mut canvas, t);
        draw_moon_glitter(&mut canvas, frame);
        let bob = if frame % 32 >= 16 { 1 } else { 0 };
        draw_boat(&mut canvas, 318, 150 + bob, frame);
        // Text last, with a shadow so bursts behind it never hide it.
        canvas.text(9.0, 8.0, &title, TEXT, Font::F57, 2, Some(SHADOW));
        canvas.text(9.0, 27.0, &subtitle, MUTED, Font::F35, 2, Some(SHADOW));
        // Legend as a small table: swatch, language, count right-aligned.
        for (row, (lang, color, n)) in legend.iter().enumerate() {
            let y = 8 + row as i64 * 12;
            canvas.rect(legend_x - 1, y, legend_x + 7, y + 9, SHADOW);
            canvas.rect(legend_x, y + 1, legend_x + 6, y + 8, *color);
            canvas.rect(legend_x, y + 1, legend_x + 6, y + 1, mix(*color, WHITE, 0.35));
            canvas.text((legend_x + 12) as f64, y as f64, &upper10(lang), MUTED, Font::F35, 2, Some(SHADOW));
            canvas.text_right((W - 9) as f64, y as f64, &n.to_string(), TEXT, Font::F35, 2, Some(SHADOW));
        }
        if hidden > 0 {
            canvas.text(
                (legend_x + 12) as f64,
                (8 + legend.len() as i64 * 12) as f64,
                &format!("+{hidden} MORE"),
                MUTED,
                Font::F35,
                2,
                Some(SHADOW),
            );
        }
        frames.push(canvas);
    }

    // Still frame: the moment with the most sparks in the air.
    let mut still = 0usize;
    if !rockets.is_empty() {
        let mut best = f64::NEG_INFINITY;
        for frame in 0..FRAMES as i64 {
            let t = frame as f64 / FRAMES as f64;
            let mut score = py_sum(rockets.iter().map(|r| {
                r.radius * 0.0f64.max(1.0 - ((py_mod(t - r.start, 1.0) - LAUNCH) / BURST - 0.2).abs() / 0.12)
            }));
            // Burning-out bursts look ragged in a still picture.
            score -= py_sum(rockets.iter().filter_map(|r| {
                let v = (py_mod(t - r.start, 1.0) - LAUNCH) / BURST;
                if 0.3 < v && v < 1.0 { Some(0.5 * r.radius) } else { None }
            }));
            if score > best {
                best = score;
                still = frame as usize;
            }
        }
    }
    save(&frames, name, Save { still: Some(still), duration: 100, ..Save::default() })
}
