//! Port of the Python prototype. Language donut: a glazed ring doughnut on a plate, the icing cut
//! in one sector per language, turning slowly under a fixed light. The legend on the right
//! gives names and percentages.
//!
//! Every formula keeps the Python operation order: floating-point results depend on it.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use crate::data::{Data, Language};
use crate::export::{save, Save};
use crate::pixel::{dither_on, mix, panel, rgb, text_width, Canvas, Font, Rgb, BG, BORDER, MUTED, TEXT};
use crate::pymath::{py_hypot, py_imod, py_floordiv, py_mod, py_round, py_sum};
use crate::pyrand::Random;

const W: i64 = 320;
const H: i64 = 128;

// Doughnut geometry (art pixels): ring radius, tube radius, view tilt.
const CX: i64 = 66;
const CY: i64 = 56;
const R0: f64 = 36.0;
const TUBE: f64 = 15.5;

// Slow turn: one revolution per loop.
const LOOP_FRAMES: usize = 120;
const FRAME_MS: u32 = 100;

// Fried dough, from the shaded underside to the lit top.
const DOUGH: [Rgb; 5] = [[74, 36, 18], [118, 60, 24], [166, 96, 38], [204, 136, 58], [228, 170, 90]];
// Pale ring where it floated in oil.
const BAND: [Rgb; 3] = [[184, 132, 76], [214, 168, 108], [232, 196, 140]];
const VANILLA: Rgb = [238, 226, 208];

const PLATE: Rgb = [38, 44, 57];
const PLATE_WELL: Rgb = [31, 36, 47];
const PLATE_RIM: Rgb = [60, 68, 84];
const PLATE_RIM_HI: Rgb = [84, 94, 112];
const SHADOW: Rgb = [18, 21, 29];

const WHITE: Rgb = [255, 255, 255];
const BLACK: Rgb = [0, 0, 0];

// Legend.
const LX: i64 = 134;
const ROW_Y0: i64 = 9;
const ROW_STEP: i64 = 19;
const RIGHT: i64 = W - 9;

// Legend chip: a drop of icing.
const DROP: [&str; 12] = [
    "....hh.....",
    "...hbbb....",
    "...hbbb....",
    "..hbbbbb...",
    ".hbbbbbbb..",
    ".hbbbbbbbd.",
    "hbwbbbbbbbd",
    "hwbbbbbbbbd",
    "hbbbbbbbbbd",
    ".bbbbbbbbd.",
    "..dbbbbdd..",
    "...dddd....",
];

const HIDDEN_LANGUAGES: [&str; 7] = ["HTML", "CSS", "SCSS", "MDX", "Jupyter Notebook", "Dockerfile", "Makefile"];

/// `math.radians(x)`: CPython multiplies by `pi / 180`.
fn py_radians(x: f64) -> f64 {
    x * (PI / 180.0)
}

/// `math.degrees(x)`: CPython multiplies by `180 / pi`.
fn py_degrees(x: f64) -> f64 {
    x * (180.0 / PI)
}

/// `max(0.0, v)`: Python keeps the first argument unless the second is strictly greater.
fn max_zero(v: f64) -> f64 {
    if v > 0.0 { v } else { 0.0 }
}

fn lighten(color: Rgb, ratio: f64) -> Rgb {
    mix(color, WHITE, ratio)
}

fn darken(color: Rgb, ratio: f64) -> Rgb {
    mix(color, BLACK, ratio)
}

/// `(COS_T, SIN_T)`: the view tilt is 56 degrees from the vertical.
fn tilt() -> (f64, f64) {
    let tilt = py_radians(56.0);
    (tilt.cos(), tilt.sin())
}

/// `(LIGHT, HALF)`: the fixed light and the half vector between light and view.
fn lighting() -> ([f64; 3], [f64; 3]) {
    let raw = [-0.5, -0.25, 0.83];
    let norm = py_sum(raw.iter().map(|c| c * c)).sqrt();
    let light = [raw[0] / norm, raw[1] / norm, raw[2] / norm];
    let (cos_t, sin_t) = tilt();
    let view = [0.0, sin_t, cos_t];
    let sum = [light[0] + view[0], light[1] + view[1], light[2] + view[2]];
    let sum_norm = py_sum(sum.iter().map(|c| c * c)).sqrt();
    let half = [sum[0] / sum_norm, sum[1] / sum_norm, sum[2] / sum_norm];
    (light, half)
}

/// One visible pixel of the torus: its screen position and shading.
struct Sample {
    x: i64,
    y: i64,
    /// Depth used to keep the front-most surface point of each screen pixel.
    depth: f64,
    /// Degrees clockwise around the ring from the back.
    theta: f64,
    /// Degrees around the tube, 0 on top, positive towards the outside.
    phi: f64,
    diffuse: f64,
    spec: f64,
}

/// Z-buffer the torus once. Turning the doughnut only shifts theta, so the geometry and
/// the lighting never need recomputing. Samples keep the Python dict's insertion order.
fn build_surface(cos_t: f64, sin_t: f64, light: [f64; 3], half: [f64; 3]) -> Vec<Sample> {
    let steps_t: usize = 1100;
    let steps_p: usize = 420;
    let mut index: HashMap<(i64, i64), usize> = HashMap::new();
    let mut best: Vec<Sample> = Vec::new();
    for i in 0..steps_t {
        let theta = 2.0 * PI * (i as f64) / (steps_t as f64);
        let (ux, uy) = (theta.sin(), -theta.cos());
        for j in 0..steps_p {
            let phi = 2.0 * PI * (j as f64) / (steps_p as f64) - PI;
            let (sp, cp) = (phi.sin(), phi.cos());
            let rho = R0 + TUBE * sp;
            let (x, y, z) = (rho * ux, rho * uy, TUBE * cp);
            let sx = py_round(CX as f64 + x);
            let sy = py_round(CY as f64 + y * cos_t - z * sin_t);
            let depth = y * sin_t + z * cos_t;
            let normal = [sp * ux, sp * uy, cp];
            let diffuse = max_zero(py_sum([normal[0] * light[0], normal[1] * light[1], normal[2] * light[2]]));
            let spec = max_zero(py_sum([normal[0] * half[0], normal[1] * half[1], normal[2] * half[2]])).powf(70.0);
            let sample = Sample {
                x: sx,
                y: sy,
                depth,
                theta: py_degrees(theta),
                phi: py_degrees(phi),
                diffuse,
                spec,
            };
            match index.get(&(sx, sy)) {
                Some(&slot) => {
                    // Keep the existing key position; only the front-most sample wins.
                    if best[slot].depth >= depth {
                        continue;
                    }
                    best[slot] = sample;
                }
                None => {
                    index.insert((sx, sy), best.len());
                    best.push(sample);
                }
            }
        }
    }
    best
}

/// Per degree of the ring: how far the icing reaches outside (drips) and inside.
fn icing_edges() -> (Vec<f64>, Vec<f64>) {
    let mut rng = Random::new(5i64);
    let mut drips: Vec<(f64, f64, f64)> = Vec::new();
    let mut angle = rng.uniform(0.0, 30.0);
    while angle < 360.0 {
        let width = rng.uniform(5.0, 9.0);
        let length = rng.uniform(14.0, 34.0);
        drips.push((angle, width, length));
        angle += rng.uniform(26.0, 44.0);
    }
    let mut outer = Vec::with_capacity(360);
    let mut inner = Vec::with_capacity(360);
    for deg in 0..360i64 {
        let mut reach = 58.0 + 4.0 * py_radians((deg * 5 + 20) as f64).sin();
        for &(center, width, length) in &drips {
            let d = py_mod((deg as f64 - center) + 180.0, 360.0) - 180.0;
            let d = d.abs() / width;
            if d < 1.0 {
                // A rounded tongue of icing running down the side.
                let candidate = 58.0 + length * (1.0 - d * d).sqrt();
                if candidate > reach {
                    reach = candidate;
                }
            }
        }
        outer.push(if reach < 118.0 { reach } else { 118.0 });
        inner.push(-62.0 - 6.0 * py_radians((deg * 4 + 70) as f64).sin() - 3.0 * py_radians((deg * 9) as f64).sin());
    }
    (outer, inner)
}

fn dough_color(x: i64, y: i64, phi: f64, diffuse: f64) -> Rgb {
    // The pale band is thin and wavy, never a ruler-straight stripe.
    let wobble = 2.0 * py_radians((x * 11 + y * 5) as f64).sin();
    let in_band = (80.0 + wobble <= phi && phi <= 100.0 + wobble) || (-100.0 <= phi && phi <= -82.0);
    let tones: &[Rgb] = if in_band { &BAND } else { &DOUGH };
    // Fried dough bounces a lot of warm light: lift the shadows.
    let level = (0.3 + 0.7 * diffuse) * ((tones.len() - 1) as f64);
    let low = level as i64;
    let frac = level - low as f64;
    // Flat tones with a narrow dithered seam between them, not a checkerboard everywhere.
    let step = if frac > 0.62 {
        1
    } else if frac < 0.38 {
        0
    } else if dither_on(x, y, 0.5) {
        1
    } else {
        0
    };
    let mut color = tones[((low + step) as usize).min(tones.len() - 1)];
    // Fried crumb texture: a few darker pores.
    if py_imod(x * 73 + y * 151 + x * y, 23) == 0 {
        color = darken(color, 0.18);
    }
    color
}

fn icing_color(base: Rgb, x: i64, y: i64, diffuse: f64, spec: f64) -> Rgb {
    if spec > 0.55 {
        return lighten(base, 0.75);
    }
    if spec > 0.25 {
        return lighten(base, 0.38);
    }
    if diffuse > 0.86 {
        return lighten(base, 0.14);
    }
    if diffuse < 0.3 {
        return darken(base, 0.42);
    }
    if diffuse < 0.42 {
        return if dither_on(x, y, 0.5) { darken(base, 0.42) } else { darken(base, 0.22) };
    }
    if diffuse < 0.58 {
        return darken(base, 0.22);
    }
    base
}

fn draw_plate(canvas: &mut Canvas, cos_t: f64, sin_t: f64) {
    // The plate lies under the doughnut's base plane, seen at the same tilt.
    let pcx = CX;
    let pcy = CY + py_round(TUBE * sin_t) + 2;
    let prx: i64 = 58;
    let pry = py_round(prx as f64 * cos_t);
    for y in (pcy - pry - 1)..(pcy + pry + 4) {
        for x in (pcx - prx - 1)..(pcx + prx + 2) {
            let ex = ((x - pcx) as f64 / prx as f64).powf(2.0);
            let d = ex + ((y - pcy) as f64 / pry as f64).powf(2.0);
            let under = ex + ((y - pcy - 3) as f64 / pry as f64).powf(2.0);
            if d <= 1.0 {
                let color = if d > 0.82 {
                    if y < pcy && d > 0.9 { PLATE_RIM_HI } else { PLATE_RIM }
                } else if d < 0.55 {
                    PLATE_WELL
                } else {
                    PLATE
                };
                canvas.put(x, y, color);
            } else if under <= 1.0 {
                canvas.put(x, y, darken(PLATE_RIM, 0.35)); // Plate edge thickness.
            }
        }
    }
    // Soft shadow of the doughnut on the plate, offset away from the light.
    for y in (pcy - pry + 2)..(pcy + pry) {
        for x in (pcx - prx + 2)..(pcx + prx - 2) {
            // Ring-shaped shadow, offset away from the light; the plate shows in the hole.
            let dx = (x - pcx - 5) as f64;
            let dy = (y - pcy - 3) as f64 / cos_t;
            let rho = py_hypot(dx, dy);
            let d = (rho - R0).abs() / (TUBE + 2.0);
            let ex = ((x - pcx) as f64 / prx as f64).powf(2.0);
            let ey = ((y - pcy) as f64 / pry as f64).powf(2.0);
            if d <= 1.0 && ex + ey <= 0.9 {
                canvas.put(x, y, if d < 0.75 { SHADOW } else { mix(SHADOW, PLATE, 0.5) });
            }
        }
    }
}

fn draw_drop(canvas: &mut Canvas, x: i64, y: i64, color: Rgb) {
    let keys = |key: u8| match key {
        b'h' => lighten(color, 0.4),
        b'b' => color,
        b'd' => darken(color, 0.35),
        _ => lighten(color, 0.8), // 'w'
    };
    for (row, line) in DROP.iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            if key != b'.' {
                canvas.put(x + col as i64, y + row as i64, keys(key));
            }
        }
    }
}

fn fit_name(name: &str, max_width: i64) -> String {
    let upper = name.to_uppercase();
    if text_width(&upper, Font::F57, 2) <= max_width {
        return upper;
    }
    // Python's split()[0]: an all-blank name would raise there, it cannot happen with real names.
    let mut first = upper.split_whitespace().next().unwrap_or("").to_string();
    while text_width(&first, Font::F57, 2) > max_width {
        first.pop();
    }
    first
}

/// `top_languages(data, count)` from the prototype, copied as is.
fn top_languages(data: &Data, count: usize) -> Vec<Language> {
    let shown: Vec<&Language> = data.languages.iter().filter(|lang| !HIDDEN_LANGUAGES.contains(&lang.name.as_str())).collect();
    // `sum(...) or 1`: an empty or zero total falls back to 1.
    let total = py_sum(shown.iter().map(|lang| lang.share));
    let total = if total == 0.0 { 1.0 } else { total };
    shown
        .iter()
        .take(count)
        .map(|lang| Language { share: lang.share / total, ..(*lang).clone() })
        .collect()
}

/// Languages whose GitHub colours look alike get colours that can be told apart,
/// the most used language keeping its own.
fn with_distinct_colors(languages: Vec<Language>) -> Vec<Language> {
    let ranked: Vec<(String, Rgb)> = languages.iter().map(|lang| (lang.name.clone(), color_of(lang))).collect();
    let colors = crate::pixel::distinct_colors(&ranked);
    languages
        .into_iter()
        .zip(colors)
        .map(|(lang, (_, color))| Language { color: Some(crate::pixel::hex(color)), ..lang })
        .collect()
}

fn color_of(lang: &Language) -> Rgb {
    rgb(lang.color.as_deref().expect("language without a colour"))
}

/// Everything that does not change between frames.
struct Scene {
    background: Canvas,
    surface: Vec<Sample>,
    surface_keys: HashSet<(i64, i64)>,
    outer: Vec<f64>,
    inner: Vec<f64>,
    languages: Vec<Language>,
    sectors: Vec<(f64, f64, Rgb)>,
    /// Icing colour for each whole degree of the ring, `None` where there is no sector.
    sector_of: Vec<Option<Rgb>>,
    other: f64,
}

fn draw(scene: &Scene, frame: usize) -> Canvas {
    let mut canvas = scene.background.clone();
    let turn = frame as f64 / LOOP_FRAMES as f64 * 360.0;
    let mut glazed: HashSet<(i64, i64)> = HashSet::new();
    for sample in &scene.surface {
        let deg = py_imod(py_mod(sample.theta - turn, 360.0) as i64, 360) as usize;
        let icing = match scene.sector_of[deg] {
            Some(color) if scene.inner[deg] <= sample.phi && sample.phi <= scene.outer[deg] => Some(color),
            _ => None,
        };
        match icing {
            Some(color) => {
                canvas.put(sample.x, sample.y, icing_color(color, sample.x, sample.y, sample.diffuse, sample.spec));
                glazed.insert((sample.x, sample.y));
            }
            None => canvas.put(sample.x, sample.y, dough_color(sample.x, sample.y, sample.phi, sample.diffuse)),
        }
    }
    // Icing edge: a darker lip where it ends, and a thin shadow on the dough below.
    for sample in &scene.surface {
        let (x, y) = (sample.x, sample.y);
        if !glazed.contains(&(x, y)) {
            continue;
        }
        let below = (x, y + 1);
        if scene.surface_keys.contains(&below) && !glazed.contains(&below) {
            let lip = canvas.get(x, y);
            canvas.put(x, y, darken(lip, 0.25));
            let shade = canvas.get(x, y + 1);
            canvas.put(x, y + 1, darken(shade, 0.35));
        }
    }
    // Dark outline where the doughnut meets the plate or the hole.
    for sample in &scene.surface {
        for (nx, ny) in [(sample.x, sample.y + 1), (sample.x + 1, sample.y), (sample.x - 1, sample.y)] {
            if !scene.surface_keys.contains(&(nx, ny)) && ny > CY + 8 {
                let edge = canvas.get(nx, ny);
                canvas.put(nx, ny, darken(edge, 0.45));
            }
        }
    }

    // Legend.
    if scene.languages.is_empty() {
        canvas.text(LX as f64, 36.0, "NO LANGUAGES", TEXT, Font::F57, 2, None);
        canvas.text(LX as f64, 58.0, "NO CODE IN YOUR", MUTED, Font::F35, 2, None);
        canvas.text(LX as f64, 72.0, "REPOS YET", MUTED, Font::F35, 2, None);
        return canvas;
    }
    let rows = scene.sectors.len() as i64;
    let y0 = ROW_Y0 + py_floordiv((6 - rows) * ROW_STEP, 2);
    for (index, lang) in scene.languages.iter().enumerate() {
        let y = y0 + index as i64 * ROW_STEP;
        let pct = format!("{}%", py_round(lang.share * 100.0));
        let name_max = RIGHT - text_width(&pct, Font::F57, 2) - 4 - (LX + 17);
        draw_drop(&mut canvas, LX, y + 1, color_of(lang));
        canvas.text((LX + 17) as f64, y as f64, &fit_name(&lang.name, name_max), TEXT, Font::F57, 2, None);
        canvas.text_right(RIGHT as f64, y as f64, &pct, mix(MUTED, TEXT, 0.5), Font::F57, 2, None);
    }
    if scene.sectors.len() > scene.languages.len() {
        let y = y0 + scene.languages.len() as i64 * ROW_STEP;
        draw_drop(&mut canvas, LX, y + 1, VANILLA);
        canvas.text((LX + 17) as f64, y as f64, "OTHER", MUTED, Font::F57, 2, None);
        let pct = format!("{}%", py_round(scene.other * 100.0));
        canvas.text_right(RIGHT as f64, y as f64, &pct, MUTED, Font::F57, 2, None);
    }
    canvas
}

pub fn render(data: &Data) -> String {
    let languages = with_distinct_colors(top_languages(data, 5));
    let (cos_t, sin_t) = tilt();
    let (light, half) = lighting();
    let surface = build_surface(cos_t, sin_t, light, half);
    let surface_keys: HashSet<(i64, i64)> = surface.iter().map(|sample| (sample.x, sample.y)).collect();
    let (outer, inner) = icing_edges();

    // Icing sectors, in degrees around the ring.
    let mut sectors: Vec<(f64, f64, Rgb)> = Vec::new();
    let mut cursor = 0.0f64;
    for lang in &languages {
        let span = lang.share * 360.0;
        sectors.push((cursor, cursor + span, color_of(lang)));
        cursor += span;
    }
    let other = max_zero(1.0 - py_sum(languages.iter().map(|lang| lang.share)));
    if !languages.is_empty() && other * 360.0 >= 1.0 {
        sectors.push((cursor, 360.0, VANILLA));
    }
    let sector_of: Vec<Option<Rgb>> = (0..360)
        .map(|deg| {
            let at = deg as f64 + 0.5;
            let color = sectors.iter().find(|(start, end, _)| *start <= at && at < *end).map(|sector| sector.2);
            color.or(sectors.last().map(|sector| sector.2))
        })
        .collect();

    let mut background = Canvas::new(W, H);
    panel(&mut background, BG, BORDER);
    draw_plate(&mut background, cos_t, sin_t);

    let scene = Scene {
        background,
        surface,
        surface_keys,
        outer,
        inner,
        languages,
        sectors,
        sector_of,
        other,
    };
    let frames: Vec<Canvas> = (0..LOOP_FRAMES).map(|index| draw(&scene, index)).collect();
    save(&frames, "donut", Save { still: Some(0), duration: FRAME_MS, ..Save::default() })
}
