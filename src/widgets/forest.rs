//! Port of the Python prototype. Forest of the year: one tree per week over 12 months, its height
//! follows that week's contributions. An empty week leaves a stump, a long pause becomes a
//! clearing where a deer grazes. Dusk sky, far mountains, drifting mist, light wind.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::data::Data;
use crate::export::{save, Save};
use crate::pixel::{dither_on, mix, text_width, weekly_totals, Canvas, Font, Rgb, FRAMES};
use crate::pymath::{py_hypot, py_imod, py_round};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 176;
const GROUND: i64 = 140; // First row of soil; trees stand on GROUND - 1.
const LEFT: i64 = 10;
const RIGHT: i64 = 390; // Horizontal span of the forest.
const WEEK_FULL: i64 = 500; // Contributions in a week for a full-height tree.
// A busier week makes a bigger tree: taller a little, much wider and fuller.
const TREE_MIN: i64 = 7;
const TREE_MAX: i64 = 58;
const HALF_MIN: f64 = 2.0;
const HALF_MAX: f64 = 9.5;

// Palette ---------------------------------------------------------------------

const SKY: [Rgb; 7] = [[10, 13, 32], [18, 22, 50], [34, 34, 74], [66, 48, 96], [122, 70, 104], [190, 104, 102], [232, 150, 112]];
const STAR: Rgb = [200, 206, 240];
const MOON: [Rgb; 3] = [[238, 232, 200], [208, 200, 170], [160, 152, 140]];
const FAR_MOUNT: Rgb = [88, 66, 112];
const FAR_SNOW: Rgb = [226, 184, 186];
const FAR_SNOW_DARK: Rgb = [150, 130, 172];
const FAR_LIT: Rgb = [132, 86, 122];
const NEAR_MOUNT: Rgb = [54, 46, 86];
const NEAR_LIT: Rgb = [88, 62, 104];
const MIST: Rgb = [150, 116, 150];
const SOIL: Rgb = [20, 26, 26];
const FLOOR: Rgb = [13, 17, 20];
const GRASS: [Rgb; 3] = [[26, 52, 36], [40, 74, 44], [66, 102, 56]];
const LABEL: Rgb = [150, 158, 150];
const LABEL_HI: Rgb = [214, 222, 206];
const TEXT: Rgb = [236, 236, 244];
const MUTED: Rgb = [176, 170, 200];
const SHADOW: Rgb = [8, 9, 22];
const OUTLINE: Rgb = [10, 20, 22];
const WARM_RIM: Rgb = [236, 170, 120];
const BARK: [Rgb; 3] = [[52, 36, 30], [84, 58, 42], [118, 86, 60]];
const TICK: Rgb = [44, 52, 50];

// Tints as (dark, mid, light). Ids index PALETTES: 0-2 firs, 3 larch, 4-6 broadleaves.
const PALETTES: [[Rgb; 3]; 7] = [
    [[16, 48, 44], [26, 76, 64], [44, 108, 84]],    // teal fir
    [[18, 52, 32], [30, 84, 44], [56, 118, 58]],    // deep green spruce
    [[24, 46, 62], [38, 76, 92], [66, 112, 120]],   // blue spruce
    [[92, 64, 24], [150, 112, 40], [206, 164, 72]], // larch
    [[30, 64, 28], [56, 104, 40], [100, 146, 58]],  // oak
    [[50, 62, 24], [88, 104, 40], [138, 146, 66]],  // olive
    [[22, 66, 46], [40, 104, 64], [84, 148, 86]],   // linden
];
const LARCH_ID: usize = 3;
const FIR_IDS: [usize; 3] = [0, 1, 2];
const LEAF_IDS: [usize; 3] = [4, 5, 6];

const FIREFLY: [Rgb; 2] = [[150, 170, 70], [236, 250, 150]];
const FLOWERS: [Rgb; 3] = [[220, 210, 120], [210, 140, 170], [180, 190, 230]];

// Hand-drawn sprites ------------------------------------------------------------

// A stag, facing left. o outline, b body, d darker back, l belly, w tail, e eye, a antler.
const DEER_UP: [&str; 14] = [
    "a.a.a.........",
    ".aa.a.........",
    "..ao..........",
    "..bbo.........",
    ".ebbb.........",
    "bbbb..........",
    "..bbd.........",
    "...bdddddddw..",
    "...bbbbbbbbbw.",
    "...bbbbbbbbb..",
    "....lllllbbb..",
    "....b.b..b.b..",
    "....b.b..b.b..",
    "....o.o..o.o..",
];
const DEER_DOWN: [&str; 14] = [
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "..............",
    "...ddddddddw..",
    "..bbbbbbbbbbw.",
    ".bbbbbbbbbbb..",
    "abb.llllbbbb..",
    "ab..b.b..b.b..",
    "eb..b.b..b.b..",
    "bo..o.o..o.o..",
];
const DEER_COLORS: [(u8, Rgb); 7] = [
    (b'o', [40, 26, 20]),
    (b'b', [138, 92, 58]),
    (b'd', [104, 66, 42]),
    (b'l', [190, 150, 106]),
    (b'w', [232, 226, 210]),
    (b'e', [20, 14, 12]),
    (b'a', [176, 150, 112]),
];
const ANTLER_TIP: Rgb = [176, 150, 112];

const STUMP: [&str; 4] = [".ttt.", "rrrrr", "rbbbr", "obbbo"];
const STUMP_SMALL: [&str; 3] = [".tt.", "rbbr", "obbo"];
const STUMP_COLORS: [(u8, Rgb); 4] = [(b't', [150, 118, 80]), (b'r', [96, 66, 44]), (b'b', [70, 48, 34]), (b'o', [44, 30, 24])];

const ROCK: [&str; 4] = ["..hh..", ".hmmd.", "hmmmdd", "mmmddd"];
const ROCK_COLORS: [(u8, Rgb); 3] = [(b'h', [120, 120, 132]), (b'm', [86, 86, 100]), (b'd', [56, 56, 70])];

const BIRD: [[&str; 3]; 3] = [
    ["o...o", ".o.o.", "..o.."],
    [".....", "ooooo", "o...o"],
    [".....", ".ooo.", "o...o"],
];

// Helpers ---------------------------------------------------------------------

fn sprite(canvas: &mut Canvas, rows: &[&str], x: i64, y: i64, colors: &[(u8, Rgb)]) {
    for (dy, line) in rows.iter().enumerate() {
        for (dx, key) in line.bytes().enumerate() {
            if key != b'.' {
                let color = colors.iter().find(|(k, _)| *k == key).expect("sprite colour").1;
                canvas.put(x + dx as i64, y + dy as i64, color);
            }
        }
    }
}

fn week_x(index: i64, count: i64) -> i64 {
    let step = (RIGHT - LEFT) as f64 / count as f64;
    py_round(LEFT as f64 + step * (index as f64 + 0.5))
}

/// (height, half-width) on a fixed square-root scale; (0, 0) for an empty week. A log scale
/// made weeks of 30 and 100 contributions look alike; the square root keeps quiet weeks
/// visible and still tells busy weeks apart.
fn tree_size(total: i64) -> (i64, f64) {
    if total <= 0 {
        return (0, 0.0);
    }
    let level = 1.0f64.min((total as f64 / WEEK_FULL as f64).sqrt());
    (py_round(TREE_MIN as f64 + (TREE_MAX - TREE_MIN) as f64 * level), HALF_MIN + (HALF_MAX - HALF_MIN) * level)
}

/// Three-tone shading with Bayer steps between tones; light runs from -1 (shadow) to 1 (lit).
fn shade(palette: [Rgb; 3], light: f64, x: i64, y: i64) -> Rgb {
    let (dark, mid, high) = (palette[0], palette[1], palette[2]);
    if light < -0.35 {
        return if dither_on(x, y, (light + 0.6) * 2.0) && light > -0.6 { mid } else { dark };
    }
    if light < 0.35 {
        return mid;
    }
    if light > 0.6 || dither_on(x, y, (light - 0.35) * 4.0) { high } else { mid }
}

// Tree builders: rows map dy (0 at the ground, growing upward) to {dx: colour}.
type Rows = BTreeMap<i64, BTreeMap<i64, Rgb>>;

fn add(rows: &mut Rows, dx: i64, dy: i64, color: Rgb) {
    rows.entry(dy).or_default().insert(dx, color);
}

fn build_fir(h: i64, half_max: f64, palette: [Rgb; 3]) -> Rows {
    let mut rows = Rows::new();
    let trunk = py_round(h as f64 * 0.08).max(1);
    let tiers = py_round(h as f64 / 9.0).min(6).max(2);
    let foliage = h - trunk;
    let seg = foliage as f64 / tiers as f64;
    for dy in 0..=trunk {
        add(&mut rows, 0, dy, BARK[1]);
        if half_max > 6.0 {
            add(&mut rows, -1, dy, BARK[0]);
        }
    }
    // Each tier hangs from the trunk; only its lower part shows under the tier above,
    // so the silhouette steps in at every tier.
    for i in 0..tiers {
        let bottom = trunk as f64 + seg * (tiers - 1 - i) as f64;
        let top = if i == 0 { h as f64 } else { bottom + seg * 1.55 };
        let span = top - bottom;
        let width = half_max * (0.5 + 0.5 * (i + 1) as f64 / tiers as f64);
        for dy in py_round(bottom)..=py_round(top) {
            let f = 0.0f64.max((top - dy as f64) / span);
            let half = width * f.powf(0.9) + 0.3;
            let reach = half.floor() as i64;
            let lowest = dy == py_round(bottom);
            for dx in -reach..=reach {
                let lit_bonus = if f < 0.35 { 0.25 } else { 0.0 };
                let low_penalty = if lowest { 0.5 } else { 0.0 };
                let mut light = dx as f64 / 1.0f64.max(half) * 1.1 + lit_bonus - low_penalty;
                // Shadow cast by the tier above, under its skirt.
                let gap = (bottom + seg) - dy as f64;
                if i != 0 && (0.0..2.0).contains(&gap) && (dx.abs() as f64) < half - 1.0 {
                    light -= 0.7;
                }
                add(&mut rows, dx, dy, shade(palette, light, dx, dy));
            }
        }
        // Drooping tips at both ends of the skirt.
        if width > 2.2 {
            let reach = (width + 0.3).floor() as i64;
            add(&mut rows, -reach, py_round(bottom) - 1, palette[0]);
            add(&mut rows, reach, py_round(bottom) - 1, palette[1]);
        }
    }
    rows
}

/// Leafy crown drawn as overlapping round clumps, each lit from the upper right.
fn clumps(rows: &mut Rows, mut blobs: Vec<(f64, f64, f64)>, palette: [Rgb; 3]) {
    blobs.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("finite y"));
    for (cx, cy, r) in blobs {
        for dy in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
            for dx in (cx - r).floor() as i64..=(cx + r).ceil() as i64 {
                let nx = (dx as f64 - cx) / r;
                let ny = (dy as f64 - cy) / r;
                let d = nx * nx + ny * ny;
                if d > 1.05 {
                    continue;
                }
                let light = nx * 0.6 + ny * 0.8;
                let color = if d > 0.6 && light < -0.1 {
                    palette[0]
                } else if light > 0.45 && d < 0.75 {
                    palette[2]
                } else if light > 0.25 && d < 0.8 {
                    if dither_on(dx, dy, 0.5) { palette[2] } else { palette[1] }
                } else {
                    palette[1]
                };
                add(rows, dx, dy, color);
            }
        }
    }
}

/// Broad leafy tree: short trunk, a wide crown of clumps that fills out with the week's size.
fn build_round(h: i64, rh: f64, palette: [Rgb; 3], rng: &mut Random) -> Rows {
    let mut rows = Rows::new();
    let trunk_top = py_round(h as f64 * 0.26).max(2);
    for dy in 0..trunk_top + 3 {
        add(&mut rows, 0, dy, if dy % 5 != 0 { BARK[1] } else { BARK[2] });
        if rh > 5.0 {
            add(&mut rows, -1, dy, BARK[0]);
        }
    }
    if rh > 5.0 {
        add(&mut rows, 1, trunk_top - 1, BARK[1]);
        add(&mut rows, 2, trunk_top, BARK[1]);
        add(&mut rows, -2, trunk_top, BARK[0]);
    }
    let crown = h - trunk_top;
    let r = 1.8f64.max(4.6f64.min(rh * 0.5));
    let mut blobs: Vec<(f64, f64, f64)> = Vec::new();
    let levels = py_round(crown as f64 / (r * 1.3)).max(1);
    for level in 0..levels {
        let f = (level as f64 + 0.5) / levels as f64;
        let cy = trunk_top as f64 + r * 0.8 + (crown as f64 - r * 1.7) * f;
        let bulge = (PI * 1.0f64.min(0.15 + f * 0.85)).sin().powf(0.4);
        let spread = (rh - r * 0.8) * bulge;
        if spread > 1.0 {
            for side in [-1.0, 1.0] {
                let bx = side * spread + rng.uniform(-0.8, 0.8);
                let by = cy + rng.uniform(-1.5, 1.5);
                let br = r * rng.uniform(0.85, 1.12);
                blobs.push((bx, by, br));
            }
            if spread > 4.0 {
                for side in [-1.0, 1.0] {
                    let bx = side * spread * 0.5 + rng.uniform(-0.6, 0.6);
                    blobs.push((bx, cy + r * 0.5, r * 1.05));
                }
            }
        }
        let bx = rng.uniform(-1.0, 1.0);
        let br = r * rng.uniform(0.95, 1.15);
        blobs.push((bx, cy + r * 0.4, br));
    }
    let bx = rng.uniform(-0.8, 0.8);
    blobs.push((bx, h as f64 - r, r * 0.95));
    clumps(&mut rows, blobs, palette);
    rows
}

/// Dark edge on the shadow side and a warm sunset rim on the lit side, so neighbours stay apart.
fn outline(rows: &Rows) -> Rows {
    let mut result = rows.clone();
    for (&dy, cols) in rows {
        for (&dx, &color) in cols {
            if !cols.contains_key(&(dx - 1)) {
                result.entry(dy).or_default().insert(dx - 1, OUTLINE);
            }
            if !cols.contains_key(&(dx + 1)) && dy > 2 && !BARK.contains(&color) {
                result.entry(dy).or_default().insert(dx, mix(color, WARM_RIM, 0.35));
            }
        }
        if !rows.contains_key(&(dy + 1)) {
            for &dx in cols.keys() {
                result.entry(dy + 1).or_default().entry(dx).or_insert(OUTLINE);
            }
        }
    }
    result
}

struct Tree {
    x: i64,
    h: i64,
    rows: Rows,
    phase: f64,
}

/// Species, tint and pixels for every week, plus the clearings where nothing grew.
fn plan_forest(weeks: &[i64]) -> (Vec<Tree>, Vec<i64>) {
    let mut rng = Random::new(11i64);
    let mut trees = Vec::new();
    let mut empties = Vec::new();
    let mut last_tint: Option<usize> = None;
    for (index, &total) in weeks.iter().enumerate() {
        let x = week_x(index as i64, weeks.len() as i64);
        let (h, half) = tree_size(total);
        if h == 0 {
            empties.push(index as i64);
            continue;
        }
        let fir = rng.random() < 0.5;
        let options: Vec<usize> = if fir {
            let mut ids = FIR_IDS.to_vec();
            if h > 30 {
                ids.push(LARCH_ID);
            }
            ids
        } else {
            LEAF_IDS.to_vec()
        };
        let remaining: Vec<usize> = options.iter().copied().filter(|&p| Some(p) != last_tint).collect();
        let choices = if remaining.is_empty() { options.clone() } else { remaining };
        let weights: Vec<f64> = choices.iter().map(|&p| if p == LARCH_ID { 0.08 } else { 1.0 }).collect();
        let tint = choices[rng.choices_weighted_index(&weights)];
        last_tint = Some(tint);
        let palette = PALETTES[tint];
        let rows = if fir { build_fir(h, half, palette) } else { build_round(h, half, palette, &mut rng) };
        let rows = outline(&rows);
        trees.push(Tree { x, h, rows, phase: rng.uniform(-0.3, 0.3) });
    }
    // Big first, so a smaller tree is drawn in front of a big neighbour and stays visible.
    trees.sort_by_key(|tree| -tree.h);
    (trees, empties)
}

fn runs(indices: &[i64]) -> Vec<(i64, i64)> {
    let mut result = Vec::new();
    let mut start: Option<i64> = None;
    for (i, &value) in indices.iter().enumerate() {
        if start.is_none() {
            start = Some(value);
        }
        if i + 1 == indices.len() || indices[i + 1] != value + 1 {
            result.push((start.expect("run start"), value));
            start = None;
        }
    }
    result
}

// Background -------------------------------------------------------------------

// (x of the summit, summit y, slope): each peak is a cone, the ridge is the lowest of them.
const FAR_PEAKS: [(i64, i64, f64); 6] = [(20, 84, 0.55), (92, 76, 0.62), (150, 90, 0.5), (226, 70, 0.6), (300, 86, 0.52), (372, 78, 0.58)];
const NEAR_PEAKS: [(i64, i64, f64); 7] = [(-10, 108, 0.4), (60, 104, 0.45), (136, 112, 0.38), (196, 100, 0.42), (268, 110, 0.4), (338, 102, 0.44), (410, 108, 0.4)];

/// Peaks drawn back to front (tallest first), so lower ones overlap them along a slope.
fn paint_range(canvas: &mut Canvas, peaks: &[(i64, i64, f64)], faces: [Rgb; 2], snow_colors: Option<[Rgb; 2]>, snow: i64, seed: i64) {
    let mut rng = Random::new(seed);
    let jag: Vec<i64> = (0..W).map(|_| rng.choice(&[0i64, 0, 1, 1, 2])).collect();
    let mut sorted = peaks.to_vec();
    sorted.sort_by_key(|peak| peak.1);
    for &(px, top, slope) in &sorted {
        let reach = py_round((GROUND - top) as f64 / slope);
        for x in (px - reach).max(0)..(px + reach + 1).min(W) {
            let ridge_y = py_round(top as f64 + (x - px).abs() as f64 * slope) + jag[x as usize] / 2;
            for y in ridge_y..GROUND {
                // The crease between the two faces wanders a little on its way down.
                let crease = px + py_round(1.5 * (y as f64 * 0.45 + px as f64).sin() + (y - top) as f64 * 0.12);
                let lit = x > crease;
                let mut color = if lit { faces[1] } else { faces[0] };
                if y == ridge_y && !lit {
                    color = mix(faces[0], faces[1], 0.5);
                }
                if let Some(snow_colors) = snow_colors
                    && y < top + snow + jag[((x * 3) % W) as usize] * 2 - py_imod(y - top, 3) {
                        color = if lit { snow_colors[1] } else { snow_colors[0] };
                    }
                canvas.put(x, y, color);
            }
        }
    }
}

fn build_background(data: &Data, weeks: &[i64], empties: &[i64]) -> Canvas {
    let mut canvas = Canvas::new(W, H);
    // Dusk sky, Bayer steps between bands.
    for y in 0..GROUND {
        let level = (7.0f64 - 1.001).min((y as f64 / (GROUND - 18) as f64).powf(1.35) * 6.0);
        let low = level as i64;
        let frac = level - low as f64;
        for x in 0..W {
            let color = if dither_on(x, y, frac) { SKY[(low + 1) as usize] } else { SKY[low as usize] };
            canvas.put(x, y, color);
        }
    }
    // Sunset glow on the right, where the recent weeks stand.
    for y in 70..GROUND {
        for x in 200..W {
            let glow = 0.0f64.max(1.0 - py_hypot((x - 330) as f64 / 120.0, (y - 124) as f64 / 40.0));
            if glow > 0.0 && dither_on(x, y, glow * 0.8) {
                let c = mix(canvas.get(x, y), [248, 184, 124], 0.35);
                canvas.put(x, y, c);
            }
        }
    }
    // Moon with its halo.
    let (mx, my) = (352i64, 24i64);
    for y in my - 14..my + 15 {
        for x in mx - 14..mx + 15 {
            let d = py_hypot((x - mx) as f64, (y - my) as f64);
            if 7.0 < d && d < 13.0 && dither_on(x, y, (13.0 - d) / 6.0 * 0.45) {
                let c = mix(canvas.get(x, y), MOON[0], 0.18);
                canvas.put(x, y, c);
            }
        }
    }
    for y in my - 7..my + 8 {
        for x in mx - 7..mx + 8 {
            let d = py_hypot((x - mx) as f64, (y - my) as f64);
            if d <= 6.6 && py_hypot((x - mx) as f64 + 3.5, (y - my) as f64 + 2.0) > 5.6 {
                let inner = py_hypot((x - mx) as f64, (y - my) as f64);
                canvas.put(x, y, if inner < 5.4 { MOON[0] } else { MOON[1] });
            }
        }
    }
    for (x, y) in [(mx + 3, my + 2), (mx + 1, my + 4), (mx + 4, my - 1)] {
        canvas.put(x, y, MOON[2]);
    }
    // Far snowy peaks, then a lower range; faces turned to the sunset (right) are lit.
    paint_range(&mut canvas, &FAR_PEAKS, [FAR_MOUNT, FAR_LIT], Some([FAR_SNOW_DARK, FAR_SNOW]), 7, 1);
    paint_range(&mut canvas, &NEAR_PEAKS, [NEAR_MOUNT, NEAR_LIT], None, 0, 2);
    // Ground: grass rim, soil, dark floor for the month labels.
    let mut rng = Random::new(3i64);
    canvas.rect(0, GROUND, W - 1, H - 1, FLOOR);
    canvas.rect(0, GROUND, W - 1, GROUND + 3, SOIL);
    for x in 0..W {
        canvas.put(x, GROUND, if x % 3 != 0 { GRASS[1] } else { GRASS[0] });
        canvas.put(x, GROUND + 1, if dither_on(x, 1, 0.5) { GRASS[0] } else { SOIL });
        if rng.random() < 0.45 {
            canvas.put(x, GROUND - 1, GRASS[(rng.randrange_to(2) + 1) as usize]);
        }
    }
    // Clearings: stumps, grass, rocks and a few flowers in the empty weeks.
    let week_count = weeks.len() as i64;
    for (start, end) in runs(empties) {
        let long_gap = end - start + 1 >= 4;
        for index in start..=end {
            let x = week_x(index, week_count);
            if !long_gap || (index - start) % 3 == 1 {
                if (index * 7) % 3 != 0 {
                    sprite(&mut canvas, &STUMP, x - 5 / 2, GROUND - STUMP.len() as i64, &STUMP_COLORS);
                } else {
                    sprite(&mut canvas, &STUMP_SMALL, x - 4 / 2, GROUND - STUMP_SMALL.len() as i64, &STUMP_COLORS);
                }
            } else if (index - start) % 6 == 3 {
                sprite(&mut canvas, &ROCK, x - 3, GROUND - 4, &ROCK_COLORS);
            } else {
                for blade in -2i64..3 {
                    let tall = 2 + py_imod(index + blade * 5, 3);
                    for dy in 0..tall {
                        let shift = if dy == tall - 1 && blade > 0 { 1 } else { 0 };
                        let color = if dy == tall - 1 { GRASS[2] } else { GRASS[1] };
                        canvas.put(x + blade + shift, GROUND - 1 - dy, color);
                    }
                }
                if index % 2 != 0 {
                    canvas.put(x + 2, GROUND - 4, FLOWERS[py_imod(index, 3) as usize]);
                }
            }
        }
    }
    // Month labels: a tick at the week holding the 1st, the name under that month.
    let year_days = data.year_days();
    let (ly, lm, ld) = crate::dates::parse_iso(&year_days[year_days.len() - 1].date);
    let last_num = crate::dates::days_from_civil(ly, lm, ld);
    let first_num = last_num - (week_count * 7 - 1);
    let (fy, fm, _) = crate::dates::civil_from_days(first_num);
    const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let step = (RIGHT - LEFT) as f64 / week_count as f64;
    let (mut y, mut m) = (fy, fm);
    loop {
        let day_num = crate::dates::days_from_civil(y, m, 1);
        if day_num > last_num {
            break;
        }
        if day_num >= first_num {
            let x = py_round(LEFT as f64 + ((day_num - first_num) as f64 / 7.0) * step);
            let label = MONTHS[(m - 1) as usize];
            canvas.put(x, GROUND + 3, TICK);
            if x + text_width(label, Font::F35, 2) + 1 < W - 4 {
                let color = if m == 1 { LABEL_HI } else { LABEL };
                canvas.text((x + 1) as f64, (GROUND + 7) as f64, label, color, Font::F35, 2, None);
            }
        }
        (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    }
    // Title.
    let total = data.total_contributions;
    let word = data.unit(total);
    canvas.text(9.0, 8.0, &format!("{total} {word}"), TEXT, Font::F57, 2, Some(SHADOW));
    canvas.text(9.0, 27.0, "12 MONTHS - ONE TREE PER WEEK", MUTED, Font::F35, 2, Some(SHADOW));
    canvas
}

/// Two noise rows: a slow one fixed across the width, a fast one that wraps every 100 px.
fn mist_noise() -> (Vec<f64>, Vec<f64>) {
    let fixed = (0..W)
        .map(|x| {
            let x = x as f64;
            0.5 + 0.3 * (x * 2.0 * PI / 400.0 * 3.0 + 1.0).sin() + 0.2 * (x * 2.0 * PI / 400.0 * 7.0).sin()
        })
        .collect();
    let moving = (0..100)
        .map(|x| {
            let x = x as f64;
            0.5 + 0.35 * (x * 2.0 * PI / 100.0).sin() + 0.15 * (x * 2.0 * PI / 50.0 + 2.0).sin()
        })
        .collect();
    (fixed, moving)
}

pub fn render(data: &Data) -> String {
    let year_days = data.year_days();
    let weeks = weekly_totals(year_days);
    let (trees, empties) = plan_forest(&weeks);
    let background = build_background(data, &weeks, &empties);
    let mut rng = Random::new(21i64);
    let mut stars: Vec<(i64, i64, i64)> = Vec::new();
    for _ in 0..46 {
        let x = rng.randrange_to(W);
        let y = rng.randrange_to(60);
        let speed = rng.choice(&[1i64, 2, 4]);
        stars.push((x, y, speed));
    }
    stars.retain(|&(x, y, _)| !(x < 230 && y < 40) && py_hypot((x - 352) as f64, (y - 24) as f64) > 16.0);
    let (fixed, moving) = mist_noise();
    let gaps = runs(&empties);
    // First of the longest clearings, like Python's max(key=...).
    let mut longest: Option<(i64, i64)> = None;
    for &run in &gaps {
        match longest {
            None => longest = Some(run),
            Some(best) if run.1 - run.0 > best.1 - best.0 => longest = Some(run),
            _ => {}
        }
    }
    let mut deer_x: Option<i64> = None;
    if let Some((start, end)) = longest
        && end - start + 1 >= 4 {
            deer_x = Some(week_x((start + end) / 2, weeks.len() as i64) - 7);
        }

    // A few fireflies over every clearing of three weeks or more.
    let mut flies: Vec<(f64, f64, i64)> = Vec::new();
    for &(start, end) in &gaps {
        if end - start + 1 >= 3 {
            for _ in 0..6i64.min((end - start + 1) / 2) {
                let fx = rng.uniform(week_x(start, weeks.len() as i64) as f64, week_x(end, weeks.len() as i64) as f64);
                let fy = rng.uniform((GROUND - 22) as f64, (GROUND - 6) as f64);
                let speed = rng.choice(&[1i64, 1, 2]);
                flies.push((fx, fy, speed));
            }
        }
    }

    let mut frames: Vec<Canvas> = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES {
        let t = frame as f64 / FRAMES as f64;
        let mut canvas = background.clone();
        // Twinkling stars, whole cycles per loop.
        for &(x, y, speed) in &stars {
            let on = (2.0 * PI * (t * speed as f64) + x as f64).sin() > -0.3;
            if on && canvas.get(x, y)[2] < 110 {
                let index = (y * canvas.width + x) as usize;
                canvas.pixels[index] = if (x + y) % 5 != 0 { STAR } else { [255, 240, 200] };
            }
        }
        // Mist drifting along the foot of the mountains.
        let shift = py_round(t * 100.0);
        for y in 108..GROUND {
            let band = (PI * (y - 108) as f64 / (GROUND - 108) as f64).sin() * 0.62;
            for x in 0..W {
                let density = band * (0.55 * fixed[x as usize] + 0.45 * moving[py_imod(x + shift, 100) as usize]);
                if dither_on(x, y, density) {
                    let c = mix(canvas.get(x, y), MIST, 0.55);
                    canvas.put(x, y, c);
                }
            }
        }
        // Birds crossing once per loop, right to left, wings beating.
        for (k, &(ox, oy)) in [(0i64, 0i64), (8, -4), (8, 4), (16, -7), (17, 8)].iter().enumerate() {
            let bx = py_round((W + 20) as f64 - t * (W + 60) as f64 + ox as f64);
            let by = py_round(68.0 + 4.0 * (2.0 * PI * t).sin() + oy as f64);
            let rows = &BIRD[(frame / 2 + k) % 3];
            sprite(&mut canvas, rows, bx, by, &[(b'o', [40, 24, 46])]);
        }
        // Trees in the wind: the top moves the most, a wave runs from left to right.
        for tree in &trees {
            let amp = 0.4 + 1.1 * tree.h as f64 / TREE_MAX as f64;
            let wave = (2.0 * PI * 2.0 * t - tree.x as f64 * 0.018 + tree.phase).sin();
            for (&dy, cols) in &tree.rows {
                let f = dy.max(0) as f64 / tree.h as f64;
                let off = py_round(amp * f * f * wave);
                let y = GROUND - 1 - dy;
                for (&dx, &color) in cols {
                    canvas.put(tree.x + dx + off, y, color);
                }
            }
        }
        // The deer grazes in the longest clearing, raising its head twice per loop.
        if let Some(dx0) = deer_x {
            let phase = frame % 32;
            let up = (10..22).contains(&phase);
            let rows: &[&str] = if up { &DEER_UP } else { &DEER_DOWN };
            sprite(&mut canvas, rows, dx0, GROUND - 14, &DEER_COLORS);
            if up && (phase == 14 || phase == 15) {
                canvas.put(dx0 + 2, GROUND - 13, ANTLER_TIP);
            }
        }
        for (k, &(fx, fy, speed)) in flies.iter().enumerate() {
            let kf = k as f64;
            let x = fx + 3.0 * (2.0 * PI * (t * speed as f64 + kf * 0.37)).sin();
            let y = fy + 2.0 * (2.0 * PI * (t * 2.0 + kf * 0.21)).cos();
            let glow = (2.0 * PI * (t * 2.0 + kf * 0.29)).sin();
            if glow > 0.1 {
                canvas.put(x, y, if glow > 0.6 { FIREFLY[1] } else { FIREFLY[0] });
                if glow > 0.75 {
                    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let px = py_imod(py_round(x + dx as f64), W);
                        let py = py_imod(py_round(y + dy as f64), H);
                        let c = mix(canvas.get(px, py), FIREFLY[0], 0.5);
                        canvas.put(x + dx as f64, y + dy as f64, c);
                    }
                }
            }
        }
        frames.push(canvas);
    }
    save(&frames, "forest", Save { still: Some(12), duration: 100, ..Save::default() })
}
