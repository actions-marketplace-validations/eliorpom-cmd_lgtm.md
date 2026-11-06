//! Port of the Python prototype. Last 7 days of contributions as a heartbeat on a retro CRT monitor:
//! one PQRST complex per day (one beat for a quiet day, two for a busy one, none for an
//! empty day), a phosphor sweep that loops, the week's total and average on the side.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::data::Data;
use crate::export::{save, Save};
use crate::pixel::{counts, dither_on, log_level, mix, text_width, Canvas, Font, Rgb};
use crate::pymath::{py_floordiv, py_imod, py_mod, py_round};

const W: i64 = 400;
const H: i64 = 128;

// Page background, used for the cut corners of the casing.
const PAGE: Rgb = [13, 17, 23];

// Casing: dark slate plastic, lit from the top left.
const CASE: Rgb = [46, 50, 57];
const CASE_LIGHT: Rgb = [70, 75, 83];
const CASE_DARK: Rgb = [30, 33, 38];
const CASE_EDGE: Rgb = [22, 24, 28];
const BEZEL_SHADE: Rgb = [14, 16, 19];

// Phosphor palette (P1 green), from the dark glass to the white-hot spot.
const GLASS: Rgb = [6, 19, 13];
const GLASS_DARK: Rgb = [3, 11, 8];
const GRID_DOT: Rgb = [16, 48, 31];
const GRID_MAJOR: Rgb = [18, 54, 34];
const PHOSPHOR: [Rgb; 5] = [[26, 92, 54], [40, 140, 80], [62, 196, 110], [120, 240, 160], [196, 255, 214]];
const SPOT: Rgb = [236, 255, 240];
const LABEL: Rgb = [70, 180, 106];
const LABEL_DIM: Rgb = [42, 118, 72];
const AMBER: Rgb = [255, 184, 64];
const AMBER_DIM: Rgb = [176, 116, 34];

// Screen and plot geometry.
const SX0: i64 = 9;
const SY0: i64 = 8;
const SX1: i64 = 390;
const SY1: i64 = 119;
const DAYS: i64 = 7;
const DAY_W: i64 = 40; // One slot per day.
const TX0: i64 = 18;
const BASE: i64 = 80;
const PEAK: i64 = 46; // Height of the R wave for a day at full level (100+ contributions).
const GAP: i64 = 12; // Columns erased ahead of the spot.
const PANEL_X: i64 = 308;
const NAMES_Y: i64 = 98;

// Slow sweep, like a real monitor: one pass over the week per loop.
const LOOP_FRAMES: usize = 128;
const FRAME_MS: u32 = 90;

// One heartbeat spans 19 columns.
const BEAT_W: i64 = 19;

// Heart icon, 2 x 2 px per cell. "2" is the lit core.
const HEART: [&str; 7] = [".11.11.", "1221111", "1211111", "1111111", ".11111.", "..111..", "...1..."];

/// Screen area with rounded CRT corners.
fn in_screen(x: i64, y: i64) -> bool {
    if !((SX0..=SX1).contains(&x) && (SY0..=SY1).contains(&y)) {
        return false;
    }
    let r = 5;
    for (cx, cy) in [(SX0 + r, SY0 + r), (SX1 - r, SY0 + r), (SX0 + r, SY1 - r), (SX1 - r, SY1 - r)] {
        if (x < SX0 + r || x > SX1 - r) && (y < SY0 + r || y > SY1 - r)
            && (x - cx).abs() <= r && (y - cy).abs() <= r
            && (x - cx).pow(2) + (y - cy).pow(2) > r * r + 2
        {
            return false;
        }
    }
    true
}

/// Polyline of one PQRST complex starting at x0, sized by the day's level.
fn beat_points(x0: i64, level: f64) -> (Vec<(i64, i64)>, i64) {
    let r = 12.max(py_round(PEAK as f64 * level));
    let p = 2.max(py_round(r as f64 * 0.13));
    let t = 3.max(py_round(r as f64 * 0.2));
    let q = 1.max(py_round(r as f64 * 0.06));
    let s = 3.max(py_round(r as f64 * 0.22));
    let mut pts = vec![(x0, BASE)];
    // P wave: a small rounded bump over 5 px.
    for (dx, k) in [(1, 0.6), (2, 1.0), (3, 1.0), (4, 0.6)] {
        pts.push((x0 + dx, BASE - py_round(p as f64 * k)));
    }
    pts.push((x0 + 5, BASE));
    pts.push((x0 + 6, BASE));
    // QRS: Q notch, tall thin R, deep S, back to the baseline.
    pts.push((x0 + 7, BASE + q));
    pts.push((x0 + 8, BASE - r));
    pts.push((x0 + 9, BASE + s));
    pts.push((x0 + 10, BASE));
    pts.push((x0 + 11, BASE));
    // T wave: a wider, softer dome.
    for (dx, k) in [(12, 0.35), (13, 0.75), (14, 1.0), (15, 1.0), (16, 0.75), (17, 0.35)] {
        pts.push((x0 + dx, BASE - py_round(t as f64 * k)));
    }
    pts.push((x0 + BEAT_W - 1, BASE));
    (pts, x0 + 8)
}

fn beats_for(count: i64) -> usize {
    if count == 0 {
        0
    } else if count < 10 {
        1
    } else {
        2
    }
}

/// The week's polyline, plus (x of R peak, day index) for each beat.
fn trace_points(values: &[i64]) -> (Vec<(i64, i64)>, Vec<(i64, usize)>) {
    let mut points = vec![(TX0, BASE)];
    let mut peaks = Vec::new();
    for (day, &count) in values.iter().enumerate() {
        let slot = TX0 + day as i64 * DAY_W;
        let offsets: Vec<i64> = match beats_for(count) {
            0 => vec![],
            1 => vec![(DAY_W - BEAT_W) / 2],
            _ => vec![1, DAY_W / 2],
        };
        for off in offsets {
            let (pts, peak) = beat_points(slot + off, log_level(count, 100));
            points.extend(pts);
            peaks.push((peak, day));
        }
    }
    points.push((TX0 + DAYS * DAY_W, BASE));
    (points, peaks)
}

/// Pixels of the polyline, grouped by column, plus the y the spot sits at per column.
fn raster(points: &[(i64, i64)]) -> (BTreeMap<i64, BTreeSet<i64>>, HashMap<i64, i64>) {
    let mut columns: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let mut spot_y: HashMap<i64, i64> = HashMap::new();
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let (mut x, mut y) = (x0, y0);
        loop {
            columns.entry(x).or_default().insert(y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
        spot_y.insert(x1, y1);
    }
    for x in columns.keys() {
        spot_y.entry(*x).or_insert(BASE);
    }
    (columns, spot_y)
}

/// Python `format_average`: "0", one decimal below 10 (".0" dropped), else rounded.
fn format_average(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value < 10.0 {
        let text = format!("{value:.1}");
        return text.strip_suffix(".0").unwrap_or(&text).to_string();
    }
    py_round(value).to_string()
}

fn draw_casing(canvas: &mut Canvas) {
    canvas.rect(0, 0, W - 1, H - 1, CASE);
    // Bevel: light top and left, dark bottom and right.
    for x in 0..W {
        canvas.put(x, 0, CASE_EDGE);
        canvas.put(x, 1, CASE_LIGHT);
        canvas.put(x, H - 2, CASE_DARK);
        canvas.put(x, H - 1, CASE_EDGE);
    }
    for y in 0..H {
        canvas.put(0, y, CASE_EDGE);
        canvas.put(1, y, CASE_LIGHT);
        canvas.put(W - 2, y, CASE_DARK);
        canvas.put(W - 1, y, CASE_EDGE);
    }
    // Faint moulding texture on the plastic.
    for y in 2..H - 2 {
        for x in 2..W - 2 {
            if (x * 7 + y * 13) % 29 == 0 {
                canvas.put(x, y, mix(CASE, CASE_LIGHT, 0.35));
            }
        }
    }
    // Rounded outer corners.
    for (cx, cy, sx, sy) in [(0, 0, 1, 1), (W - 1, 0, -1, 1), (0, H - 1, 1, -1), (W - 1, H - 1, -1, -1)] {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (2, 0), (0, 2)] {
            canvas.put(cx + dx * sx, cy + dy * sy, PAGE);
        }
        canvas.put(cx + sx, cy + sy, CASE_EDGE);
    }
    // Recessed bezel around the glass: dark on top/left, catching light at the bottom/right.
    for x in SX0 - 2..SX1 + 3 {
        canvas.put(x, SY0 - 2, CASE_DARK);
        canvas.put(x, SY0 - 1, CASE_EDGE);
        canvas.put(x, SY1 + 1, BEZEL_SHADE);
        canvas.put(x, SY1 + 2, CASE_LIGHT);
    }
    for y in SY0 - 2..SY1 + 3 {
        canvas.put(SX0 - 2, y, CASE_DARK);
        canvas.put(SX0 - 1, y, CASE_EDGE);
        canvas.put(SX1 + 1, y, BEZEL_SHADE);
        canvas.put(SX1 + 2, y, CASE_LIGHT);
    }
    canvas.put(SX0 - 2, SY1 + 2, CASE_DARK);
    canvas.put(SX1 + 2, SY0 - 2, CASE_DARK);
    // Screws in the corners of the bezel strip.
    for (x, y) in [(4, 4), (W - 6, 4), (4, H - 6), (W - 6, H - 6)] {
        canvas.rect(x, y, x + 1, y + 1, CASE_DARK);
        canvas.put(x, y, CASE_LIGHT);
    }
    // Power LED on the bottom strip.
    canvas.rect(SX1 - 12, H - 5, SX1 - 9, H - 4, [80, 230, 120]);
    canvas.put(SX1 - 12, H - 5, [200, 255, 210]);
}

/// Glass, vignette, plot grid and the separator before the numbers.
fn draw_glass(canvas: &mut Canvas) {
    let cx = (SX0 + SX1) as f64 / 2.0;
    let cy = (SY0 + SY1) as f64 / 2.0;
    for y in SY0..=SY1 {
        for x in SX0..=SX1 {
            if !in_screen(x, y) {
                canvas.put(x, y, CASE_EDGE);
                continue;
            }
            // Elliptic vignette: the tube is brighter at the center.
            let d = ((x as f64 - cx) / ((SX1 - SX0) as f64 / 2.0)).powf(2.0)
                + ((y as f64 - cy) / ((SY1 - SY0) as f64 / 2.0)).powf(2.0);
            let color = if dither_on(x, y, 0.0f64.max((d - 0.55) * 1.4)) { GLASS_DARK } else { GLASS };
            canvas.put(x, y, color);
        }
    }
    let plot_x1 = TX0 + DAYS * DAY_W;
    // Minor grid dots every 5 px, major dotted lines every 20 px.
    for y in SY0 + 4..NAMES_Y - 3 {
        for x in TX0..=plot_x1 {
            let gx = x - TX0;
            let gy = BASE - y;
            let major_x = py_imod(gx, 20) == 0 && py_imod(y, 2) == 0;
            let major_y = py_imod(gy, 20) == 0 && py_imod(x, 2) == 0;
            if major_x || major_y {
                canvas.put(x, y, GRID_MAJOR);
            } else if py_imod(gx, 5) == 0 && py_imod(gy, 5) == 0 {
                canvas.put(x, y, GRID_DOT);
            }
        }
    }
    // Dotted separator between the trace and the numbers.
    for y in (SY0 + 6..SY1 - 5).step_by(2) {
        canvas.put(PANEL_X - 6, y, GRID_MAJOR);
    }
}

/// Draws text as emissive pixels (glowing), not on the canvas directly.
fn text_pixels(emissive: &mut HashMap<(i64, i64), Rgb>, x: i64, y: i64, text: &str, color: Rgb, font: Font, scale: i64) {
    let (width, _) = font.size();
    for (offset, ch) in text.to_uppercase().chars().enumerate() {
        let glyph = font.glyph(ch);
        for (index, bit) in glyph.bytes().enumerate() {
            if bit != b'1' {
                continue;
            }
            let index = index as i64;
            let gx = x + (offset as i64 * (width + 1) + index % width) * scale;
            let gy = y + (index / width) * scale;
            for sy in 0..scale {
                for sx in 0..scale {
                    emissive.insert((gx + sx, gy + sy), color);
                }
            }
        }
    }
}

/// Phosphor bloom around every lit pixel, then the lit pixels, then scanlines.
fn glow_and_scan(canvas: &mut Canvas, emissive: &HashMap<(i64, i64), Rgb>) {
    let mut halo: HashMap<(i64, i64), f64> = HashMap::new();
    for (&(x, y), color) in emissive {
        let sum = color[0] as i64 + color[1] as i64 + color[2] as i64;
        for (dx, dy, k) in [
            (-1, 0, 0.34), (1, 0, 0.34), (0, -1, 0.34), (0, 1, 0.34),
            (-1, -1, 0.18), (1, -1, 0.18), (-1, 1, 0.18), (1, 1, 0.18),
        ] {
            let key = (x + dx, y + dy);
            if emissive.contains_key(&key) {
                continue;
            }
            let value = k * sum as f64 / (3.0 * 255.0);
            let entry = halo.entry(key).or_insert(0.0);
            *entry = entry.max(value);
        }
    }
    for (&(x, y), &strength) in &halo {
        if in_screen(x, y) {
            let base = canvas.get(x, y);
            canvas.put(x, y, mix(base, PHOSPHOR[1], 0.8f64.min(strength * 1.3)));
        }
    }
    for (&(x, y), &color) in emissive {
        if in_screen(x, y) {
            canvas.put(x, y, color);
        }
    }
    // Scanlines: every other row of the tube is slightly darker.
    for y in (SY0 + 1..=SY1).step_by(2) {
        for x in SX0..=SX1 {
            if in_screen(x, y) {
                let base = canvas.get(x, y);
                canvas.put(x, y, mix(base, [0, 0, 0], 0.16));
            }
        }
    }
    // A faint reflection of the room on the curved glass, top left.
    for y in SY0 + 2..SY0 + 16 {
        for x in SX0 + 3..SX0 + 60 {
            let band = (x - SX0) as f64 + (y - SY0) as f64 * 2.2;
            if 18.0 < band && band < 30.0 && dither_on(x, y, 0.22) && in_screen(x, y) {
                let base = canvas.get(x, y);
                canvas.put(x, y, mix(base, [190, 230, 210], 0.10));
            }
        }
    }
}

/// Everything that does not change between frames.
struct Scene {
    background: Canvas,
    values: Vec<i64>,
    names: Vec<String>,
    columns: BTreeMap<i64, BTreeSet<i64>>,
    spot_y: HashMap<i64, i64>,
    peaks: Vec<(i64, usize)>,
    best: Option<usize>,
    total: i64,
    span: i64,
}

fn draw_frame(scene: &Scene, frame: usize) -> Canvas {
    let values = &scene.values;
    let span = scene.span;
    let mut canvas = scene.background.clone();
    let mut emissive: HashMap<(i64, i64), Rgb> = HashMap::new();
    let head = frame as f64 / LOOP_FRAMES as f64 * span as f64; // One full sweep per loop.

    for (&x, ys) in &scene.columns {
        let column = x - TX0;
        if py_mod(column as f64 - head, span as f64) < GAP as f64 {
            continue; // Erased just ahead of the spot.
        }
        let age = py_mod(head - column as f64, span as f64);
        // Fresh phosphor is bright, then decays to a steady afterglow.
        let strength = 0.45 + 0.55 * (-age / 55.0).exp();
        let level = ((strength * (PHOSPHOR.len() as f64 - 0.01)) as i64).min(PHOSPHOR.len() as i64 - 1) as usize;
        let day = py_floordiv(column, DAY_W);
        for &y in ys {
            if let Some(best) = scene.best
                && day == best as i64 && y != BASE {
                    // The best day's beats glow amber, like its name.
                    let ratio = (strength - 0.45) / 0.55 * 0.8 + 0.2;
                    emissive.insert((x, y), mix(AMBER_DIM, [255, 226, 160], ratio));
                    continue;
                }
            emissive.insert((x, y), PHOSPHOR[level]);
        }
    }

    // The spot: white-hot core, short comet tail and a dithered bloom.
    let hx = TX0 + head as i64;
    let hy = *scene.spot_y.get(&hx).unwrap_or(&BASE);
    for (dx, color) in [(-2, PHOSPHOR[3]), (-1, PHOSPHOR[4]), (0, SPOT)] {
        match scene.columns.get(&(hx + dx)) {
            Some(ys) => {
                for &y in ys {
                    emissive.insert((hx + dx, y), color);
                }
            }
            None => {
                emissive.insert((hx + dx, hy), color);
            }
        }
    }
    for dy in -3..=3 {
        for dx in -3..=3 {
            let d2 = dx * dx + dy * dy;
            let key = (hx + dx, hy + dy);
            if emissive.contains_key(&key) || d2 > 10 {
                continue;
            }
            if d2 <= 2 {
                emissive.insert(key, PHOSPHOR[3]);
            } else if dither_on(key.0, key.1, 0.5) {
                emissive.insert(key, PHOSPHOR[1]);
            }
        }
    }

    // Labels on the tube.
    text_pixels(&mut emissive, TX0, SY0 + 5, "LAST 7 DAYS", LABEL, Font::F57, 2);
    let after_title = TX0 + text_width("LAST 7 DAYS", Font::F57, 2) + 8;
    text_pixels(&mut emissive, after_title, SY0 + 7, "CONTRIBUTIONS", LABEL_DIM, Font::F35, 2);
    for (day, label) in scene.names.iter().enumerate() {
        let cx = TX0 + day as i64 * DAY_W + DAY_W / 2;
        let color = if scene.best == Some(day) {
            AMBER
        } else if values[day] != 0 {
            LABEL
        } else {
            GRID_MAJOR
        };
        let left = py_round(cx as f64 - text_width(label, Font::F57, 2) as f64 / 2.0);
        text_pixels(&mut emissive, left, NAMES_Y, label, color, Font::F57, 2);
    }

    // The heart lights up for a moment each time the spot crosses an R peak.
    let lit = scene.peaks.iter().any(|&(peak, _)| {
        let d = head - (peak - TX0) as f64;
        (0.0..7.0).contains(&d)
    });
    let heart_color = if lit { PHOSPHOR[3] } else { GRID_MAJOR };
    let heart_core = if lit { SPOT } else { PHOSPHOR[0] };
    let hx0 = TX0 + DAYS * DAY_W - 14;
    for (row, line) in HEART.iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            if key != b'.' {
                let c = if key == b'2' { heart_core } else { heart_color };
                for sy in 0..2 {
                    for sx in 0..2 {
                        emissive.insert((hx0 + col as i64 * 2 + sx, SY0 + 4 + row as i64 * 2 + sy), c);
                    }
                }
            }
        }
    }

    // Numbers panel, like the heart-rate readout: the week's total, big.
    let px1 = SX1 - 6;
    text_pixels(&mut emissive, PANEL_X, SY0 + 5, "TOTAL", LABEL_DIM, Font::F57, 2);
    let total_text = scene.total.to_string();
    let big = [4, 3, 2]
        .into_iter()
        .find(|&k| text_width(&total_text, Font::F57, k) <= px1 - PANEL_X + 1)
        .unwrap_or(2);
    let total_y = SY0 + 24 + py_floordiv(28 - 7 * big, 2);
    text_pixels(&mut emissive, PANEL_X, total_y, &total_text, PHOSPHOR[3], Font::F57, big);
    let average = format_average(scene.total as f64 / DAYS as f64);
    text_pixels(&mut emissive, PANEL_X, SY0 + 60, &average, LABEL, Font::F57, 2);
    let per_day_x = PANEL_X + text_width(&average, Font::F57, 2) + 4;
    text_pixels(&mut emissive, per_day_x, SY0 + 64, "/DAY", LABEL_DIM, Font::F35, 2);

    // Best day of the week, on the same row as the day names.
    if let Some(best) = scene.best {
        text_pixels(&mut emissive, PANEL_X, NAMES_Y + 4, "BEST", AMBER_DIM, Font::F35, 2);
        let best_x = PANEL_X + text_width("BEST", Font::F35, 2) + 6;
        text_pixels(&mut emissive, best_x, NAMES_Y, &values[best].to_string(), AMBER, Font::F57, 2);
    }

    glow_and_scan(&mut canvas, &emissive);
    canvas
}

pub fn render(data: &Data) -> String {
    let all_days = &data.days;
    let days = &all_days[all_days.len().saturating_sub(DAYS as usize)..];
    let padding = DAYS as usize - days.len();
    let mut values = vec![0i64; padding];
    values.extend(counts(days));
    let mut names = vec![String::new(); padding];
    names.extend(days.iter().map(|day| crate::dates::weekday_abbr(&day.date).to_string()));

    let (points, peaks) = trace_points(&values);
    let (columns, spot_y) = raster(&points);
    let total: i64 = values.iter().sum();
    // First day with the highest count, like Python's `max(range(DAYS), key=...)`.
    let best = if total != 0 {
        let mut best = 0;
        for (day, &count) in values.iter().enumerate() {
            if count > values[best] {
                best = day;
            }
        }
        Some(best)
    } else {
        None
    };

    let mut background = Canvas::with_background(W, H, CASE);
    draw_casing(&mut background);
    draw_glass(&mut background);

    let scene = Scene {
        background,
        values,
        names,
        columns,
        spot_y,
        peaks,
        best,
        total,
        span: DAYS * DAY_W + 1,
    };
    let frames: Vec<Canvas> = (0..LOOP_FRAMES).map(|frame| draw_frame(&scene, frame)).collect();
    // Still: the spot at the far right, the whole week drawn behind it.
    save(&frames, "ecg", Save { still: Some(LOOP_FRAMES - 2), duration: FRAME_MS, ..Save::default() })
}
