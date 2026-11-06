//! Port of the Python prototype. Night field: one tuft of grass per day over the last 90 days,
//! oldest on the left, yesterday on the right. Flowers are the languages, in proportion.
//! Fireflies are the pull requests merged in the last 30 days.
//!
//! Every formula and every random draw keeps the order of the Python source, so the frames
//! match pixel for pixel.

use std::f64::consts::PI;

use crate::data::{Data, Day, Language};
use crate::export::{save, Save};
use crate::pixel::{log_level, mix, rgb, Canvas, Rgb, BAYER, FRAMES};
use crate::pymath::{py_hypot, py_imod, py_round, py_sum};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 176;
const FULL_GRASS_AT: i64 = 100; // Contributions in a day that grow a tuft to full height.
const ROOT: i64 = 173; // Root line of the day tufts.
const TALLEST: f64 = 86.0; // Height of a full tuft, in art pixels.
const MAX_FIREFLIES: i64 = 60;
const MOON_X: i64 = 318;
const MOON_Y: i64 = 34;

const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];

// Palette: deep blues for the sky, teal for the grass, cream moon, lime fireflies.
const SKY: [(i64, Rgb); 6] = [
    (0, [6, 10, 28]), (26, [10, 18, 41]), (50, [15, 28, 60]),
    (72, [22, 41, 79]), (90, [30, 55, 98]), (104, [40, 70, 111]),
];
const MOON: Rgb = [244, 240, 216];
const MOON_SHADE: Rgb = [216, 211, 182];
const MOON_MARE: Rgb = [196, 192, 166];
const MOON_HALO: [Rgb; 2] = [[44, 72, 120], [34, 60, 106]];
const STAR: [Rgb; 3] = [[90, 111, 158], [159, 179, 223], [238, 243, 255]];
const CLOUD: Rgb = [27, 45, 85];
const CLOUD_LIT: Rgb = [58, 85, 133];
const GLOW_MILKY_WAY: Rgb = [22, 36, 74];
const FAR_TREE: Rgb = [20, 36, 74];
const WINDOW: Rgb = [255, 207, 110];
const WINDOW_GLOW: Rgb = [122, 106, 90];
const COTTAGE_DARK: Rgb = [19, 33, 63];
const COTTAGE_WALL: Rgb = [24, 42, 78];
const FLOOR: [(i64, Rgb); 3] = [(136, [14, 32, 52]), (150, [11, 26, 43]), (162, [8, 20, 34])];
const SOIL: Rgb = [4, 10, 18];
const OUTLINE: Rgb = [6, 16, 25];
const PEBBLE: Rgb = [13, 26, 40];
const FIREFLY: [Rgb; 3] = [[251, 255, 196], [225, 245, 106], [147, 184, 58]];
const FLOWER_CORE: Rgb = [255, 244, 194];

// Day tufts are the lightest grass in the scene, so their tops read as a graph.
const NEAR_BODY: Rgb = [29, 74, 87];
const NEAR_SHADE: Rgb = [20, 56, 69];
const NEAR_RIM: Rgb = [60, 124, 132];
const NEAR_TIP: Rgb = [121, 185, 178];
const NEAR_LIT_BODY: Rgb = [44, 102, 112];
const NEAR_LIT_RIM: Rgb = [100, 168, 166];
const NEAR_LIT_TIP: Rgb = [196, 240, 220];
const NEAR_STUBBLE: Rgb = [42, 85, 96];
const NEAR_ROOT: Rgb = [13, 42, 54];

// Hand-drawn sprites. Keys map to colors at draw time; "." is transparent.
const FLOWER: [&str; 5] = [".p.p.", "ppLpp", ".pcp.", "pdpdp", ".d.d."];
const FLOWER_SMALL: [&str; 3] = [".p.", "pcp", ".d."];
const COTTAGE: [&str; 8] = [
    "....r....", "...rrr...", "..rrrrr.c", ".rrrrrrrc", "rrrrrrrrr", ".wwwwwww.", ".wwwyww..", ".wwwyww..",
];
const PINE: [&str; 6] = ["..t..", ".ttt.", "..t..", ".ttt.", "ttttt", "..t.."];
const ROUND_TREE: [&str; 5] = [".ttt.", "ttttt", "ttttt", ".ttt.", "..t.."];

const HIDDEN_LANGUAGES: [&str; 7] = ["HTML", "CSS", "SCSS", "MDX", "Jupyter Notebook", "Dockerfile", "Makefile"];

/// `(cycles, wavelength, phase, strength)` of the gust bands.
type Phases = [(i64, i64, f64, f64); 3];

struct Hill {
    base: i64,
    amp: f64,
    freq: f64,
    seed: i64,
    body: Rgb,
    edge: Rgb,
}

const HILLS: [Hill; 3] = [
    Hill { base: 100, amp: 7.0, freq: 0.018, seed: 3, body: [26, 44, 85], edge: [43, 68, 116] },
    Hill { base: 111, amp: 5.0, freq: 0.03, seed: 4, body: [19, 35, 74], edge: [32, 58, 102] },
    Hill { base: 121, amp: 3.0, freq: 0.05, seed: 7, body: [15, 29, 60], edge: [26, 49, 86] },
];

/// Short, hazy meadow grass: it gives the wind somewhere to show without hiding the graph.
struct MidSpec {
    top: i64,
    tufts: usize,
    h: (f64, f64),
    w: i64,
    sway: f64,
    body: [Rgb; 2],
    shade: Rgb,
    high: Rgb,
    tip: Rgb,
    fill: Option<Rgb>,
}

const MID_LAYERS: [MidSpec; 4] = [
    MidSpec {
        top: 124, tufts: 190, h: (2.0, 5.0), w: 1, sway: 0.3,
        body: [[26, 53, 82], [28, 57, 86]], shade: [22, 48, 74], high: [46, 88, 119], tip: [63, 111, 138],
        fill: Some([19, 42, 68]),
    },
    MidSpec {
        top: 132, tufts: 150, h: (3.0, 8.0), w: 2, sway: 0.4,
        body: [[17, 38, 61], [19, 42, 66]], shade: [14, 33, 53], high: [36, 80, 106], tip: [58, 113, 137],
        fill: Some([14, 32, 52]),
    },
    MidSpec {
        top: 146, tufts: 120, h: (2.0, 4.0), w: 1, sway: 0.3,
        body: [[15, 34, 54], [17, 37, 57]], shade: [12, 29, 47], high: [29, 66, 88], tip: [26, 58, 80],
        fill: None,
    },
    MidSpec {
        top: 158, tufts: 110, h: (2.0, 4.0), w: 1, sway: 0.3,
        body: [[12, 28, 46], [14, 32, 50]], shade: [10, 24, 40], high: [26, 60, 80], tip: [23, 52, 72],
        fill: None,
    },
];

struct MidBlade {
    x: f64,
    root: i64,
    height: f64,
    lean: f64,
    body: Rgb,
    phase: f64,
}

struct DayBlade {
    x: f64,
    height: f64,
    lean: f64,
    curve: f64,
    width: i64,
    phase: f64,
    main: bool,
    stubble: bool,
    flower: Option<Rgb>,
    small: bool,
}

struct DayTuft {
    x: f64,
    level: f64,
    blades: Vec<DayBlade>,
}

struct Twinkle {
    x: i64,
    y: i64,
    cycles: i64,
    phase: f64,
    big: bool,
}

struct Firefly {
    x: f64,
    y: f64,
    ax: f64,
    ay: f64,
    cx: i64,
    cy: i64,
    px: f64,
    py: f64,
    blink: i64,
    pb: f64,
    front: bool,
}

// Helpers mirroring the Python `Layer` class ----------------------------------------------

/// `Layer.get`: `None` outside the image, the pixel otherwise.
fn get(c: &Canvas, x: f64, y: f64) -> Option<Rgb> {
    let (x, y) = (py_round(x), py_round(y));
    if 0 <= x && x < W && 0 <= y && y < H {
        Some(c.get(x, y))
    } else {
        None
    }
}

/// `Layer.blend`: mixes the color over what is already there.
fn blend(c: &mut Canvas, x: f64, y: f64, color: Rgb, ratio: f64) {
    if let Some(under) = get(c, x, y) {
        c.put(x, y, mix(under, color, ratio));
    }
}

/// `Layer.sprite`: every non-transparent key is drawn with its color.
fn sprite(c: &mut Canvas, rows: &[&str], x: i64, y: i64, colors: impl Fn(char) -> Rgb) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, key) in row.chars().enumerate() {
            if key != '.' {
                c.put(x + dx as i64, y + dy as i64, colors(key));
            }
        }
    }
}

fn lighter(color: Rgb, amount: f64) -> Rgb {
    mix(color, WHITE, amount)
}

fn darker(color: Rgb, amount: f64) -> Rgb {
    mix(color, BLACK, amount)
}

fn channel_sum(color: Rgb) -> i64 {
    color.iter().map(|&value| value as i64).sum()
}

// Static backdrop -------------------------------------------------------------------------

fn draw_sky(c: &mut Canvas) {
    let blend_height = 8.0;
    for y in 0..H {
        for x in 0..W {
            let mut color = SKY[0].1;
            for index in 1..SKY.len() {
                let (boundary, next_color) = SKY[index];
                let boundary = boundary as f64;
                let yf = y as f64;
                if yf >= boundary + blend_height / 2.0 {
                    color = next_color;
                } else if yf >= boundary - blend_height / 2.0 {
                    if (yf - (boundary - blend_height / 2.0)) / blend_height * 16.0
                        > BAYER[(y % 4) as usize][(x % 4) as usize] as f64
                    {
                        color = next_color;
                    }
                    break;
                } else {
                    break;
                }
            }
            c.put(x, y, color);
        }
    }
}

fn draw_milky_way(c: &mut Canvas) {
    for x in 0..W {
        let center = 8.0 + x as f64 * 0.16;
        for y in 0..92 {
            let dist = ((y as f64) - center - 6.0 * (x as f64 * 0.02).sin()).abs();
            let ratio = 0.0f64.max(1.0 - dist / 16.0) * 0.55;
            if ratio * 16.0 > BAYER[(y % 4) as usize][(x % 4) as usize] as f64 {
                let under = c.get(x, y);
                if channel_sum(under) < channel_sum(GLOW_MILKY_WAY) {
                    c.put(x, y, mix(under, GLOW_MILKY_WAY, 0.6));
                }
            }
        }
    }
}

fn draw_moon(c: &mut Canvas) {
    let (mx, my) = (MOON_X, MOON_Y);
    let r = 11.0;
    // Two dithered halo rings, brighter close to the disc.
    for y in my - 34..my + 35 {
        for x in mx - 34..mx + 35 {
            let dist = py_hypot((x - mx) as f64, (y - my) as f64);
            if r < dist && dist <= 18.0 {
                let ratio = 1.0 - (dist - r) / (18.0 - r);
                if ratio * 16.0 > (BAYER[(y % 4) as usize][(x % 4) as usize] - 3) as f64 {
                    c.put(x, y, MOON_HALO[0]);
                }
            } else if 18.0 < dist && dist <= 34.0 {
                let ratio = 1.0 - (dist - 18.0) / 16.0;
                if ratio * 9.0 > BAYER[(y % 4) as usize][(x % 4) as usize] as f64 {
                    if let Some(under) = get(c, x as f64, y as f64) {
                        c.put(x, y, mix(under, MOON_HALO[1], 0.7));
                    }
                }
            }
        }
    }
    // The disc, darker toward the lower left limb, with a few seas.
    let seas = [(-3.0, -3.0, 3.6, 2.6), (3.0, 1.0, 3.0, 2.2), (-1.0, 5.0, 2.0, 1.4)];
    for y in my - 11..my + 12 {
        for x in mx - 11..mx + 12 {
            let (dx, dy) = ((x - mx) as f64, (y - my) as f64);
            let dist = py_hypot(dx, dy);
            if dist > r + 0.3 {
                continue;
            }
            let mut color = MOON;
            let limb = (-dx + dy) / r;
            if dist > r - 2.2 && limb > 0.15 {
                color = MOON_SHADE;
            }
            for (sx, sy, ax, ay) in seas {
                if ((dx - sx) / ax).powf(2.0) + ((dy - sy) / ay).powf(2.0) <= 1.0 {
                    color = if color == MOON { MOON_MARE } else { darker(MOON_MARE, 0.05) };
                    break;
                }
            }
            c.put(x, y, color);
        }
    }
    // A bright crater with its rays, and a highlight on the upper right limb.
    c.put(mx + 4, my + 7, [255, 253, 240]);
    for (dx, dy) in [(2, -8), (3, -8), (5, -7), (6, -6), (7, -5), (8, -4)] {
        c.put(mx + dx, my + dy, [255, 253, 240]);
    }
}

fn draw_clouds(c: &mut Canvas) {
    let (mx, my) = (MOON_X, MOON_Y);
    // x, y of the flat underside, length.
    let wisps: [(i64, i64, i64); 6] = [(256, 50, 84), (296, 57, 52), (336, 20, 44), (30, 36, 70), (132, 62, 58), (190, 28, 30)];
    for (x0, y0, length) in wisps {
        let mut rng = Random::new(x0 * 7 + y0);
        // Each streak: start x, underside y, length, thickness.
        let mut streaks: Vec<(f64, i64, f64, f64)> = vec![(x0 as f64, y0, length as f64, 1.0)];
        for _ in 0..2 {
            let sub = length as f64 * rng.uniform(0.35, 0.6);
            let sx = x0 as f64 + rng.uniform(0.1, 0.5) * length as f64;
            let sy = y0 - rng.choice(&[1i64, 2]);
            streaks.push((sx, sy, sub, 1.6));
        }
        for (sx, sy, slen, thick) in streaks {
            for x in py_round(sx)..py_round(sx + slen) {
                let u = (x as f64 - sx) / slen;
                let half = thick * (PI * u).sin().max(0.0).powf(0.8);
                let top = py_round(sy as f64 - half);
                for y in top..sy + 1 {
                    let near = 0.0f64.max(1.0 - py_hypot((x - mx) as f64, (y - my) as f64) / 42.0);
                    let lift = near * 1.6 + if y == top { 0.35 } else { 0.0 };
                    let color = mix(CLOUD, CLOUD_LIT, 1.0f64.min(lift));
                    // Ragged ends: the outer fifth thins out in a checker pattern.
                    if (u < 0.15 || u > 0.85) && py_imod(x + y, 2) != 0 {
                        continue;
                    }
                    c.put(x, y, color);
                }
            }
        }
    }
}

/// Returns the twinkling stars; faint ones are baked into the backdrop.
fn draw_stars(c: &mut Canvas, rng: &mut Random) -> Vec<Twinkle> {
    let (mx, my) = (MOON_X, MOON_Y);
    let mut twinkling = Vec::new();
    for _ in 0..170 {
        let x = rng.randrange_to(W);
        let y = rng.randrange(0, 96);
        if py_hypot((x - mx) as f64, (y - my) as f64) < 24.0 {
            continue;
        }
        let level = rng.random();
        if level < 0.7 {
            c.put(x, y, STAR[0]);
        } else {
            let cycles = rng.choice(&[1i64, 2, 3]);
            let phase = rng.uniform(0.0, 2.0 * PI);
            twinkling.push(Twinkle { x, y, cycles, phase, big: level > 0.93 });
        }
    }
    twinkling
}

/// Trees and a lit cottage on the far ridge, so the hills read as a place.
fn draw_far_life(c: &mut Canvas) {
    let hill = &HILLS[0];
    let mut rng = Random::new(21);
    let xs: Vec<i64> = (18..70)
        .step_by(6)
        .chain((150..196).step_by(5))
        .chain((226..250).step_by(6))
        .collect();
    for x in xs {
        if rng.random() < 0.75 {
            let rows: &[&str] = if rng.random() < 0.6 { &PINE[..] } else { &ROUND_TREE[..] };
            let top = hill_top(hill, x + 2);
            let dx = rng.randint(-1, 1);
            sprite(c, rows, x + dx, top - rows.len() as i64 + 1, |_| FAR_TREE);
        }
    }
    let cx = 104;
    let top = hill_top(hill, cx + 4);
    sprite(c, &COTTAGE, cx, top - COTTAGE.len() as i64 + 2, |key| match key {
        'r' | 'c' => COTTAGE_DARK,
        'w' => COTTAGE_WALL,
        _ => WINDOW,
    });
    // Warm light spilling from the window.
    let (wx, wy) = (cx + 4, top - COTTAGE.len() as i64 + 9);
    for (dx, dy) in [(-1, 0), (1, 0), (-1, -1), (1, -1)] {
        blend(c, (wx + dx) as f64, (wy + dy) as f64, WINDOW_GLOW, 0.35);
    }
}

/// `noise_line(x, freq, seed)`: smooth hill profile from a few seeded sines.
fn noise_line(x: i64, freq: f64, seed: i64) -> f64 {
    let mut rng = Random::new(seed);
    let mut total = 0.0;
    for octave in 0..3i64 {
        let phase = rng.uniform(0.0, 2.0 * PI);
        total += (x as f64 * freq * (octave + 1) as f64 * 1.7 + phase).sin() / (octave + 1) as f64;
    }
    total / 1.8
}

fn hill_top(hill: &Hill, x: i64) -> i64 {
    py_round(hill.base as f64 + hill.amp * noise_line(x, hill.freq, hill.seed))
}

/// Bands of wind rolling across the field from left to right, whole cycles per loop.
fn gust(x: f64, t: f64, phases: &Phases) -> f64 {
    let mut wave = 0.0;
    for &(cycles, wavelength, phase, strength) in phases {
        let value = (2.0 * PI * cycles as f64 * t - 2.0 * PI * x / wavelength as f64 + phase).sin();
        wave += strength * 0.0f64.max(value).powf(3.0);
    }
    wave
}

fn draw_hills(c: &mut Canvas) {
    for (index, hill) in HILLS.iter().enumerate() {
        for x in 0..W {
            let top = hill_top(hill, x);
            for y in top..H {
                c.put(x, y, hill.body);
            }
            // Moonlight catches the ridge where it faces the moon (slope rising to the right).
            let rising = hill_top(hill, x + 2) - top;
            if rising <= 0 || x % 3 == 0 {
                c.put(x, top, hill.edge);
            }
            if rising < 0 {
                let color = if x % 2 == 1 { mix(hill.body, hill.edge, 0.5) } else { hill.body };
                c.put(x, top + 1, color);
            }
        }
        if index == 0 {
            draw_far_life(c);
        }
    }
}

fn build_backdrop() -> (Canvas, Vec<Twinkle>) {
    let mut canvas = Canvas::with_background(W, H, BLACK);
    draw_sky(&mut canvas);
    let mut rng = Random::new(8);
    draw_milky_way(&mut canvas);
    let twinkling = draw_stars(&mut canvas, &mut rng);
    draw_moon(&mut canvas);
    draw_clouds(&mut canvas);
    draw_hills(&mut canvas);
    (canvas, twinkling)
}

// Grass -----------------------------------------------------------------------------------

fn make_mid_tufts(rng: &mut Random) -> Vec<Vec<MidBlade>> {
    let mut layers = Vec::new();
    for (depth, spec) in MID_LAYERS.iter().enumerate() {
        let mut blades = Vec::new();
        for _ in 0..spec.tufts {
            let cx = rng.uniform(-10.0, W as f64 + 10.0);
            let root = spec.top + rng.randint(0, 3 + depth as i64 * 2);
            let count = rng.randint(3, 6);
            for index in 0..count {
                let fan = (index as f64 - (count - 1) as f64 / 2.0) / (count - 1).max(1) as f64;
                let x = cx + fan * 3.0 + rng.uniform(-1.0, 1.0);
                let blade_root = root + rng.randint(0, 1);
                let height = rng.uniform(spec.h.0, spec.h.1);
                let lean = fan * 0.35 + rng.uniform(-0.1, 0.15);
                let body = spec.body[rng.choice_index(spec.body.len())];
                let phase = rng.uniform(0.0, 2.0 * PI);
                blades.push(MidBlade { x, root: blade_root, height, lean, body, phase });
            }
        }
        blades.sort_by_key(|blade| blade.root);
        layers.push(blades);
    }
    layers
}

/// Flat fill under the hazy grass, then dithered steps darker toward the viewer.
fn floor_color(x: i64, y: i64, fill: Rgb) -> Rgb {
    let mut color = fill;
    for (boundary, step_color) in FLOOR {
        if y >= boundary + 2 {
            color = step_color;
        } else if y >= boundary - 2 {
            if (y - boundary + 2) as f64 / 4.0 * 16.0 > BAYER[(y % 4) as usize][(x % 4) as usize] as f64 {
                color = step_color;
            }
            break;
        } else {
            break;
        }
    }
    color
}

fn draw_mid_layer(c: &mut Canvas, spec: &MidSpec, blades: &[MidBlade], t: f64, phases: &Phases) {
    if let Some(fill) = spec.fill {
        for y in spec.top + 2..H {
            for x in 0..W {
                c.put(x, y, floor_color(x, y, fill));
            }
        }
    }
    for blade in blades {
        let g = gust(blade.x, t, phases);
        let wind = 0.35 + g + 0.12 * (2.0 * PI * 4.0 * t + blade.phase).sin();
        let bend = (blade.lean + spec.sway * wind) * blade.height;
        let lit = g > 0.5;
        let body = if lit { spec.high } else { blade.body };
        let side = if lit { blade.body } else { spec.shade };
        let steps = py_round(blade.height).max(1);
        for step in 0..=steps {
            let ratio = step as f64 / steps as f64;
            let x = blade.x + bend * ratio.powf(1.8);
            let y = blade.root - step;
            let width = py_round(spec.w as f64 * (1.0 - ratio) + 0.4).max(1);
            let left = py_round(x - width as f64 / 2.0);
            for offset in 0..width {
                let mut color = if width > 1 && offset == 0 { side } else { body };
                if ratio > 0.8 {
                    color = spec.tip;
                }
                c.put(left + offset, y, color);
            }
        }
    }
}

// Day tufts ------------------------------------------------------------------------------

/// One tuft per day; its tallest blade is exactly the day's height, so tops read as a graph.
fn make_day_tufts(days: &[Day], rng: &mut Random) -> Vec<DayTuft> {
    let count = days.len() as i64;
    let mut tufts = Vec::new();
    for (index, day) in days.iter().enumerate() {
        let value = day.count;
        let x = 4.0 + index as f64 * (W - 8) as f64 / (count - 1).max(1) as f64;
        let level = log_level(value, FULL_GRASS_AT);
        let mut blades = Vec::new();
        if value == 0 {
            // Bare stubble: two short, dark blades, never lit, so a pause reads as a gap.
            for blade_index in 0..3i64 {
                let height = rng.uniform(2.0, 4.5);
                let lean = (blade_index - 1) as f64 * 0.6;
                let phase = rng.uniform(0.0, 6.3);
                blades.push(DayBlade {
                    x: x + (blade_index - 1) as f64 * 1.6,
                    height,
                    lean,
                    curve: 0.0,
                    width: 1,
                    phase,
                    main: false,
                    stubble: true,
                    flower: None,
                    small: false,
                });
            }
        } else {
            let height = 10.0 + (TALLEST - 10.0) * level;
            let blade_count = 3 + py_round(3.0 * level);
            let main = blade_count / 2;
            for blade_index in 0..blade_count {
                let fan = (blade_index as f64 - (blade_count - 1) as f64 / 2.0) / (blade_count - 1).max(1) as f64;
                let is_main = blade_index == main;
                let blade_x = x + fan * (2.4 + 1.6 * level) + if is_main { 0.0 } else { rng.uniform(-0.4, 0.4) };
                let blade_height = if is_main { height } else { height * rng.uniform(0.5, 0.86) };
                let lean = fan * (if is_main { 0.0 } else { 0.95 }) + rng.uniform(-0.12, 0.16);
                let curve = rng.uniform(1.5, 2.5);
                let width = if level > 0.45 { 3 } else { 2 };
                let phase = rng.uniform(0.0, 2.0 * PI);
                blades.push(DayBlade {
                    x: blade_x,
                    height: blade_height,
                    lean,
                    curve,
                    width,
                    phase,
                    main: is_main,
                    stubble: false,
                    flower: None,
                    small: false,
                });
            }
            // Draw the main blade last, so the graph top is never hidden.
            blades.sort_by_key(|blade| blade.main);
        }
        tufts.push(DayTuft { x, level, blades });
    }
    tufts
}

/// Python `top_languages(data)`: the five largest shown languages, shares normalised over
/// all shown languages (the markup ones are left out of the total).
fn top_languages(data: &Data) -> Vec<(Option<String>, f64)> {
    let shown: Vec<&Language> = data
        .languages
        .iter()
        .filter(|language| !HIDDEN_LANGUAGES.contains(&language.name.as_str()))
        .collect();
    let mut total = py_sum(shown.iter().map(|language| language.share));
    if total == 0.0 {
        total = 1.0;
    }
    let top: Vec<&&Language> = shown.iter().take(5).collect();
    // Languages whose GitHub colours look alike get colours that can be told apart,
    // the most used language keeping its own. Languages without a colour stay without one.
    let ranked: Vec<(String, Rgb)> = top
        .iter()
        .filter_map(|language| language.color.as_deref().map(|hex| (language.name.clone(), rgb(hex))))
        .collect();
    let mut distinct = crate::pixel::distinct_colors(&ranked).into_iter();
    top.iter()
        .map(|language| {
            let color = language.color.as_ref().and_then(|_| distinct.next()).map(|(_, c)| crate::pixel::hex(c));
            (color, language.share / total)
        })
        .collect()
}

/// Flowers in proportion to each language, on the taller tufts, spread along the field.
fn place_flowers(tufts: &mut [DayTuft], languages: &[(Option<String>, f64)], rng: &mut Random) {
    let candidates: Vec<usize> = (0..tufts.len()).filter(|&index| tufts[index].level >= 0.3).collect();
    if candidates.is_empty() || languages.is_empty() {
        return;
    }
    let total = 28i64.min((languages.len() as i64).max(py_round(candidates.len() as f64 * 0.45)));
    let shares: Vec<f64> = languages.iter().map(|(_, share)| share * total as f64).collect();
    let mut counts: Vec<i64> = shares.iter().map(|&share| py_round(share).max(1)).collect();
    while counts.iter().sum::<i64>() > total && *counts.iter().max().unwrap() > 1 {
        let largest = counts.iter().max().copied().unwrap();
        let index = counts.iter().position(|&count| count == largest).unwrap();
        counts[index] -= 1;
    }
    // Evenly spaced picks across the candidates, then languages shuffled over them.
    let pick_count = counts.iter().sum::<i64>();
    let step = candidates.len() as f64 / pick_count as f64;
    let picks: Vec<usize> = (0..pick_count)
        .map(|i| {
            let slot = ((i as f64 * step + step / 2.0) as i64).min(candidates.len() as i64 - 1);
            candidates[slot as usize]
        })
        .collect();
    let mut colors: Vec<Option<String>> = Vec::new();
    for ((color, _), &n) in languages.iter().zip(&counts) {
        for _ in 0..n {
            colors.push(color.clone());
        }
    }
    rng.shuffle(&mut colors);
    for (tuft_index, color) in picks.into_iter().zip(colors) {
        let tuft_level = tufts[tuft_index].level;
        let tuft = &mut tufts[tuft_index];
        let secondary: Vec<usize> = (0..tuft.blades.len()).filter(|&i| !tuft.blades[i].main).collect();
        let blade_index = secondary[rng.choice_index(secondary.len())];
        let last_height = tuft.blades.last().unwrap().height;
        let hex = color.expect("language without a color cannot get a flower");
        let blade = &mut tuft.blades[blade_index];
        blade.flower = Some(rgb(&hex));
        blade.height = last_height * rng.uniform(0.58, 0.92);
        blade.small = tuft_level < 0.4;
    }
}

/// Flower sprite colors: petals, lighter and darker shades, core.
fn flower_colors(color: Rgb) -> impl Fn(char) -> Rgb {
    let light = lighter(color, 0.45);
    let dark = darker(color, 0.4);
    move |key| match key {
        'p' => color,
        'L' => light,
        'd' => dark,
        _ => FLOWER_CORE,
    }
}

fn draw_day_tufts(c: &mut Canvas, tufts: &[DayTuft], t: f64, phases: &Phases, flowers: &mut Vec<(i64, i64, Rgb, bool)>) {
    for tuft in tufts {
        let g = gust(tuft.x, t, phases);
        let lit = g > 0.5;
        for blade in &tuft.blades {
            let height = blade.height;
            let wobble = 0.14 * (2.0 * PI * 4.0 * t + blade.phase).sin();
            let wind = 0.3 + g + wobble;
            // Tip displacement: a lean of its own plus the wind, capped so tufts stay above their day.
            let disp = blade.lean * height.min(30.0) * 0.35 + wind * (2.0 + 9.0 * 1.0f64.min(height / TALLEST));
            if blade.stubble {
                for step in 0..=py_round(height) {
                    let x = blade.x + disp * 0.2 * step as f64 / 1.0f64.max(height);
                    c.put(x, ROOT - step, NEAR_STUBBLE);
                }
                continue;
            }
            let steps = py_round(height - 0.05 * disp.abs()).max(1);
            let mut tip = (0i64, 0i64);
            for step in 0..=steps {
                let ratio = step as f64 / steps as f64;
                let x = blade.x + disp * ratio.powf(blade.curve);
                let y = ROOT - step;
                let width = py_round(blade.width as f64 * (1.0 - ratio) + 0.45).max(1);
                let left = py_round(x - width as f64 / 2.0);
                for offset in 0..width {
                    let mut color = if ratio > 0.84 {
                        if lit { NEAR_LIT_TIP } else { NEAR_TIP }
                    } else if width > 1 && offset == width - 1 {
                        if lit { NEAR_LIT_RIM } else { NEAR_RIM }
                    } else if width > 2 && offset == 0 {
                        NEAR_SHADE
                    } else if lit {
                        NEAR_LIT_BODY
                    } else {
                        NEAR_BODY
                    };
                    let depth = if !blade.main { (0.42 - ratio) / 0.42 } else { (0.25 - ratio) / 0.25 };
                    if depth > 0.0 && depth * 16.0 > BAYER[py_imod(y, 4) as usize][py_imod(left + offset, 4) as usize] as f64 {
                        color = if depth > 0.55 { NEAR_ROOT } else { NEAR_SHADE };
                    }
                    c.put(left + offset, y, color);
                }
                tip = (py_round(x), y);
            }
            if let Some(flower) = blade.flower {
                flowers.push((tip.0, tip.1, flower, blade.small));
            }
        }
    }
}

fn sprite_filled(rows: &[&str], dx: i64, dy: i64) -> bool {
    if dx < 0 || dy < 0 {
        return false;
    }
    rows.get(dy as usize)
        .and_then(|row| row.as_bytes().get(dx as usize))
        .map_or(false, |&key| key != b'.')
}

fn draw_flowers(c: &mut Canvas, flowers: &[(i64, i64, Rgb, bool)]) {
    for &(x, y, color, small) in flowers {
        let rows: &[&str] = if small { &FLOWER_SMALL[..] } else { &FLOWER[..] };
        let half = rows[0].len() as i64 / 2;
        let (x0, y0) = (x - half, y - half);
        // A dark outline, so blue languages still stand out against the teal grass.
        for (dy, row) in rows.iter().enumerate() {
            for (dx, key) in row.chars().enumerate() {
                if key == '.' {
                    continue;
                }
                let (dx, dy) = (dx as i64, dy as i64);
                for (nx, ny) in [(dx + 1, dy), (dx - 1, dy), (dx, dy + 1), (dx, dy - 1)] {
                    if !sprite_filled(rows, nx, ny) {
                        c.put(x0 + nx, y0 + ny, OUTLINE);
                    }
                }
            }
        }
        sprite(c, rows, x0, y0, flower_colors(color));
    }
}

fn draw_soil(c: &mut Canvas, seed: i64) {
    let mut rng = Random::new(seed);
    for y in ROOT + 1..H {
        for x in 0..W {
            if y > ROOT + 1 || py_imod(x * 7 + y, 3) != 0 {
                c.put(x, y, SOIL);
            }
        }
    }
    for _ in 0..40 {
        let x = rng.randrange_to(W);
        let y = rng.randint(ROOT + 2, H - 1);
        c.put(x, y, PEBBLE);
        if rng.random() < 0.5 {
            c.put(x + 1, y, PEBBLE);
        }
    }
}

// Fireflies -------------------------------------------------------------------------------

fn make_fireflies(count: i64, rng: &mut Random) -> Vec<Firefly> {
    let mut flies = Vec::new();
    for index in 0..count {
        let x = rng.uniform(6.0, (W - 6) as f64);
        let y = rng.uniform(78.0, 162.0);
        let ax = rng.uniform(3.0, 9.0);
        let ay = rng.uniform(2.0, 5.0);
        let cx = rng.choice(&[1i64, 2]);
        let cy = rng.choice(&[1i64, 2, 3]);
        let px = rng.uniform(0.0, 6.3);
        let py = rng.uniform(0.0, 6.3);
        let blink = rng.choice(&[1i64, 2, 3]);
        let pb = rng.uniform(0.0, 6.3);
        flies.push(Firefly { x, y, ax, ay, cx, cy, px, py, blink, pb, front: index % 2 == 0 });
    }
    flies
}

fn draw_fireflies(c: &mut Canvas, flies: &[Firefly], t: f64, front: bool) {
    for fly in flies {
        if fly.front != front {
            continue;
        }
        let x = py_round(fly.x + fly.ax * (2.0 * PI * fly.cx as f64 * t + fly.px).sin());
        let y = py_round(fly.y + fly.ay * (2.0 * PI * fly.cy as f64 * t + fly.py).sin());
        let glow = 0.5 + 0.5 * (2.0 * PI * fly.blink as f64 * t + fly.pb).sin();
        if glow < 0.18 {
            c.put(x, y, FIREFLY[2]);
            continue;
        }
        // Soft light around the body, blended with what is behind it.
        let reach: i64 = if glow > 0.6 { 3 } else { 2 };
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let dist = py_hypot(dx as f64, dy as f64);
                if 0.0 < dist && dist <= reach as f64 {
                    let strength = glow * (1.0 - dist / (reach as f64 + 0.6)) * 0.55;
                    if strength * 16.0 > BAYER[py_imod(y + dy, 4) as usize][py_imod(x + dx, 4) as usize] as f64 * 0.6 {
                        blend(c, (x + dx) as f64, (y + dy) as f64, FIREFLY[2], 0.8f64.min(strength + 0.25));
                    }
                }
            }
        }
        c.put(x, y, if glow > 0.55 { FIREFLY[0] } else { FIREFLY[1] });
        if glow > 0.55 {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                c.put(x + dx, y + dy, FIREFLY[1]);
            }
        }
    }
}

fn draw_twinkles(c: &mut Canvas, stars: &[Twinkle], t: f64) {
    for star in stars {
        let value = (2.0 * PI * star.cycles as f64 * t + star.phase).sin();
        if value > 0.7 && star.big {
            c.put(star.x, star.y, STAR[2]);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                c.put(star.x + dx, star.y + dy, STAR[1]);
            }
        } else if value > 0.2 {
            c.put(star.x, star.y, if star.big { STAR[2] } else { STAR[1] });
        } else if value > -0.5 {
            c.put(star.x, star.y, if star.big { STAR[1] } else { STAR[0] });
        }
    }
}

// Widget ----------------------------------------------------------------------------------

pub fn render(data: &Data) -> String {
    let mut rng = Random::new("meadow"); // Seed word: changing it redraws the scene.
    let phases: Phases = [
        (1, 260, rng.uniform(0.0, 6.3), 0.55),
        (2, 170, rng.uniform(0.0, 6.3), 0.35),
        (3, 110, rng.uniform(0.0, 6.3), 0.2),
    ];
    let (backdrop, stars) = build_backdrop();
    let mid = make_mid_tufts(&mut rng);
    let mut tufts = make_day_tufts(&data.days, &mut rng);
    place_flowers(&mut tufts, &top_languages(data), &mut rng);
    let flies = make_fireflies(data.merged_prs_30d.min(MAX_FIREFLIES), &mut rng);

    let frames: Vec<Canvas> = (0..FRAMES)
        .map(|index| {
            let t = index as f64 / FRAMES as f64;
            let mut canvas = backdrop.clone();
            draw_twinkles(&mut canvas, &stars, t);
            for (spec, blades) in MID_LAYERS.iter().zip(&mid) {
                draw_mid_layer(&mut canvas, spec, blades, t, &phases);
            }
            draw_fireflies(&mut canvas, &flies, t, false);
            draw_soil(&mut canvas, 5);
            let mut flowers = Vec::new();
            draw_day_tufts(&mut canvas, &tufts, t, &phases, &mut flowers);
            draw_flowers(&mut canvas, &flowers);
            draw_fireflies(&mut canvas, &flies, t, true);
            canvas
        })
        .collect();
    save(&frames, "grass", Save { still: Some(FRAMES / 4), ..Save::default() })
}
