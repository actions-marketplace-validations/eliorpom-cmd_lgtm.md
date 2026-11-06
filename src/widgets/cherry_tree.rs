//! Port of the Python prototype. Cherry tree in bloom at dusk: ninety blossom clusters, one per
//! day (oldest on the left), and one falling petal per merged pull request, landing on the
//! grass. Same draws in the same order and same float formulas as the Python, so frames
//! match pixel for pixel.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use crate::data::Data;
use crate::export::{save, Save};
use crate::pixel::{log_level, mix, text_width, Canvas, Font, Rgb, BAYER};
use crate::pymath::{py_ffloordiv, py_hypot, py_imod, py_round};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 176;
const FULL_BLOOM_AT: i64 = 100;
const MAX_PETALS: i64 = 60;
const TRUNK_X: i64 = 198;
const TRUNK_Y: i64 = 156;
const SUN_X: i64 = 312;
const SUN_Y: i64 = 122;
const SUN_R: i64 = 14;
const LOOP: usize = 128;
const FRAME_MS: u32 = 90;

// Palette, generated from the Python module.
const SKY: [(i64, Rgb); 8] = [
    (0, [36, 27, 71]), (22, [58, 35, 88]), (44, [92, 44, 104]), (66, [138, 58, 110]),
    (86, [187, 79, 108]), (104, [226, 112, 95]), (118, [243, 154, 92]), (128, [252, 197, 111]),
];
const SUN: Rgb = [255, 243, 196];
const SUN_RIM: Rgb = [255, 224, 142];
const SUN_HALO: [Rgb; 2] = [[255, 210, 124], [247, 172, 100]];
const CLOUD_TOP: Rgb = [106, 52, 104];
const CLOUD_BODY: Rgb = [142, 66, 112];
const CLOUD_LIT: Rgb = [242, 149, 95];
const CLOUD_EDGE: Rgb = [255, 207, 122];
const STAR: [Rgb; 2] = [[140, 122, 184], [233, 222, 252]];

/// Mountain range: base row, amplitude, frequency, noise seed, body and sunlit edge.
struct Range {
    base: i64,
    amp: f64,
    freq: f64,
    seed: i64,
    body: Rgb,
    edge: Rgb,
}

const RANGES: [Range; 3] = [
    Range { base: 124, amp: 9.0, freq: 0.022, seed: 31, body: [154, 74, 116], edge: [210, 108, 108] },
    Range { base: 134, amp: 6.0, freq: 0.035, seed: 32, body: [110, 56, 102], edge: [184, 90, 104] },
    Range { base: 143, amp: 4.0, freq: 0.05, seed: 35, body: [76, 40, 86], edge: [148, 80, 106] },
];
const GROUND: Rgb = [44, 26, 60];
const GROUND_DARK: Rgb = [34, 20, 48];
const GROUND_RIM: Rgb = [200, 106, 94];
const GRASS: [Rgb; 4] = [[58, 36, 72], [74, 44, 80], [122, 62, 88], [224, 138, 98]];
const SHADOW: Rgb = [26, 15, 38];
const BARK_DARK: Rgb = [42, 20, 38];
const BARK_MID: Rgb = [69, 34, 58];
const BARK_LIGHT: Rgb = [100, 50, 74];
const BARK_RIM: Rgb = [180, 90, 80];
const BARK_HOT: Rgb = [240, 144, 96];
const LENTICEL: Rgb = [122, 74, 90];
const CRACK: Rgb = [30, 14, 28];
const PINK_L: Rgb = [255, 241, 242];
const PINK_P: Rgb = [255, 196, 212];
const PINK_S: Rgb = [239, 143, 176];
const PINK_D: Rgb = [189, 90, 138];
const PINK_C: Rgb = [210, 60, 108];
const PINK_Y: Rgb = [255, 227, 154];
const PINK_WARM: Rgb = [255, 217, 196];
const BUD: Rgb = [194, 80, 122];
const BUD_DARK: Rgb = [106, 42, 72];
const COUNTER_TEXT: Rgb = [255, 230, 234];
const COUNTER_SHADOW: Rgb = [23, 12, 34];

// Hand-drawn blossoms. L light, P petal, S shaded petal, c heart.
const BLOSSOM: [&str; 5] = [".L.P.", "LLPPP", ".PcP.", "SPPPS", ".S.S."];
const BLOSSOM_SMALL: [&str; 3] = [".L.", "LcP", ".S."];
// Petal turning in the air: four views, then the resting one.
const PETAL_VIEWS: [&[&str]; 4] = [&["LP", ".S"], &[".L", "PS"], &["LPS"], &["L", "S"]];
const PETAL_RESTS: [&[&str]; 5] = [&["LP", ".S"], &["PL", "S."], &["LPS"], &["L", "S"], &[".L", "PS"]];
const PETAL_ICON: [&str; 5] = ["..LL.", ".LPPS", "LPPS.", "PPS..", ".S..."];
const FALL_END: f64 = 0.62; // Every petal has landed by this point of the loop; the rest is a pause.
const FALL_TIME: f64 = 0.16; // Share of the loop one petal spends in the air.
const WIND_DRIFT: (f64, f64) = (14.0, 46.0); // The wind blows to the right.
const COUNTER_BOX: (i64, i64) = (W - 186, H - 21); // Keep the counter clear of petals (left x, top y).

/// Colours of a sprite, indexed by key: L, P, S, c.
type SpriteColors = [Rgb; 4];

fn key_index(key: u8) -> usize {
    match key {
        b'L' => 0,
        b'P' => 1,
        b'S' => 2,
        b'c' => 3,
        _ => panic!("unknown sprite key {}", key as char),
    }
}

/// Deterministic per-line noise: three octaves of sines with seeded phases.
fn noise_line(x: f64, freq: f64, seed: i64) -> f64 {
    let mut rng = Random::new(seed);
    let mut total = 0.0;
    for octave in 0..3 {
        let phase = rng.uniform(0.0, 2.0 * PI);
        let k = (octave + 1) as f64;
        total += (x * freq * k * 1.7 + phase).sin() / k;
    }
    total / 1.8
}

fn range_top(spec: &Range, x: i64) -> i64 {
    py_round(spec.base as f64 + spec.amp * noise_line(x as f64, spec.freq, spec.seed))
}

/// A gentle knoll under the tree.
fn ground_top(x: f64) -> i64 {
    let knoll = 6.0 * (-((x - TRUNK_X as f64) / 120.0).powf(2.0)).exp();
    py_round(154.0 - knoll + 1.5 * noise_line(x, 0.04, 40))
}

fn hash2(a: i64, b: i64, seed: i64) -> f64 {
    let value = (a * 374_761_393 + b * 668_265_263 + seed * 2_147_483_647) & 0xFFFF_FFFF;
    let mixed = (((value ^ (value >> 13)) as u64).wrapping_mul(1_274_126_177) & 0xFFFF_FFFF) as i64;
    let value = mixed ^ (mixed >> 16);
    value as f64 / 4_294_967_295.0
}

/// Python's `round(x, 1)`, reduced to the integer tenth it names (only used as a key).
fn round1(x: f64) -> i64 {
    let t = x * 10.0;
    let e = x.mul_add(10.0, -t); // Exact error of the product: decides exact ties.
    let floor = t.floor();
    if t - floor == 0.5 {
        if e > 0.0 {
            return floor as i64 + 1;
        }
        if e < 0.0 {
            return floor as i64;
        }
    }
    py_round(t)
}

/// `draw` helpers: the Python `Layer` class rounds coordinates like `Canvas.put`.
fn blend(canvas: &mut Canvas, x: f64, y: f64, color: Rgb, ratio: f64) {
    let (x, y) = (py_round(x), py_round(y));
    if (0..W).contains(&x) && (0..H).contains(&y) {
        let under = canvas.get(x, y);
        canvas.put(x, y, mix(under, color, ratio));
    }
}

fn draw_sprite(canvas: &mut Canvas, rows: &[&str], x: i64, y: i64, colors: &SpriteColors) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            if key != b'.' {
                canvas.put(x + dx as i64, y + dy as i64, colors[key_index(key)]);
            }
        }
    }
}

// Sky and land ----------------------------------------------------------------------

fn draw_sky(canvas: &mut Canvas) {
    let blend_height = 8.0;
    for y in 0..H {
        for x in 0..W {
            let mut color = SKY[0].1;
            for index in 1..SKY.len() {
                let (boundary, next_color) = (SKY[index].0 as f64, SKY[index].1);
                let yf = y as f64;
                if yf >= boundary + blend_height / 2.0 {
                    color = next_color;
                } else if yf >= boundary - blend_height / 2.0 {
                    if (yf - (boundary - blend_height / 2.0)) / blend_height * 16.0
                        > BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64
                    {
                        color = next_color;
                    }
                    break;
                } else {
                    break;
                }
            }
            canvas.put(x, y, color);
        }
    }
}

fn draw_sun(canvas: &mut Canvas) {
    let sun_r = SUN_R as f64;
    for y in SUN_Y - 40..SUN_Y + 20 {
        for x in SUN_X - 48..SUN_X + 49 {
            let dist = py_hypot((x - SUN_X) as f64, (y - SUN_Y) as f64 * 1.25);
            let bayer = BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64;
            if sun_r < dist && dist <= sun_r + 8.0 {
                if (1.0 - (dist - sun_r) / 8.0) * 16.0 > bayer - 4.0 {
                    canvas.put(x, y, SUN_HALO[0]);
                }
            } else if sun_r + 8.0 < dist && dist <= sun_r + 30.0
                && (1.0 - (dist - sun_r - 8.0) / 22.0) * 10.0 > bayer {
                    blend(canvas, x as f64, y as f64, SUN_HALO[1], 0.6);
                }
        }
    }
    for y in SUN_Y - SUN_R..=SUN_Y + SUN_R {
        for x in SUN_X - SUN_R..=SUN_X + SUN_R {
            let dist = py_hypot((x - SUN_X) as f64, (y - SUN_Y) as f64);
            if dist <= sun_r + 0.3 {
                canvas.put(x, y, if dist > sun_r - 1.2 { SUN_RIM } else { SUN });
            }
        }
    }
    // Heat bands across the lower half of the disc, the classic sunset stripes.
    for band_y in [SUN_Y + 5, SUN_Y + 9] {
        for x in SUN_X - SUN_R..=SUN_X + SUN_R {
            if py_hypot((x - SUN_X) as f64, (band_y - SUN_Y) as f64) <= sun_r {
                canvas.put(x, band_y, SUN_HALO[0]);
            }
        }
    }
}

/// Long flat streaks: dark violet tops, undersides lit by the sun below.
fn draw_clouds(canvas: &mut Canvas) {
    let streaks: [(i64, i64, i64, i64); 8] = [
        (18, 58, 120, 3), (90, 72, 90, 2), (230, 48, 140, 3), (262, 86, 110, 3),
        (40, 96, 70, 2), (330, 66, 70, 2), (150, 36, 60, 2), (196, 100, 50, 2),
    ];
    for (x0, y0, length, thick) in streaks {
        let mut rng = Random::new(x0 * 13 + y0);
        // Parts: (start x, start y, length, thickness).
        let mut parts: Vec<(f64, i64, f64, f64)> = vec![(x0 as f64, y0, length as f64, thick as f64)];
        let first_draw = rng.uniform(0.15, 0.45);
        let start_x = x0 as f64 + length as f64 * first_draw;
        let start_y = y0 - thick + 1;
        let second_draw = rng.uniform(0.3, 0.5);
        let part_length = length as f64 * second_draw;
        let part_thick = if thick - 1 == 0 { 1 } else { thick - 1 }; // Python: `thick - 1 or 1`.
        parts.push((start_x, start_y, part_length, part_thick as f64));
        for (sx, sy, slen, sthick) in parts {
            for x in py_round(sx)..py_round(sx + slen) {
                let u = (x as f64 - sx) / slen;
                let half = sthick * (PI * u).sin().max(0.0).powf(0.6);
                let top = py_round(sy as f64 - half);
                if top > sy {
                    continue;
                }
                let near_sun = (1.0 - (x - SUN_X).abs() as f64 / 170.0).max(0.0);
                for y in top..=sy {
                    if (u < 0.12 || u > 0.9) && py_imod(x + y, 2) != 0 {
                        continue;
                    }
                    let color = if y == sy {
                        mix(CLOUD_LIT, CLOUD_EDGE, near_sun)
                    } else if y == sy - 1 {
                        mix(CLOUD_BODY, CLOUD_LIT, 0.35 + 0.5 * near_sun)
                    } else if y == top {
                        CLOUD_TOP
                    } else {
                        CLOUD_BODY
                    };
                    canvas.put(x, y, color);
                }
            }
        }
    }
}

struct Star {
    x: i64,
    y: i64,
    cycles: i64,
    phase: f64,
    big: bool,
}

fn make_stars(rng: &mut Random) -> Vec<Star> {
    let mut stars = Vec::new();
    while stars.len() < 26 {
        let x = rng.randrange_to(W);
        let y = rng.randrange(0, 40);
        // The first stars only show in the dark top of the sky, away from the sun.
        if (y as f64) < 14.0 + x as f64 * 0.08 {
            let cycles = rng.choice(&[1i64, 2, 3]);
            let phase = rng.uniform(0.0, 6.3);
            let big = rng.random() < 0.2;
            stars.push(Star { x, y, cycles, phase, big });
        }
    }
    stars
}

fn draw_ranges(canvas: &mut Canvas) {
    for spec in &RANGES {
        for x in 0..W {
            let top = range_top(spec, x);
            for y in top..H {
                canvas.put(x, y, spec.body);
            }
            // The sun grazes the crests: lit where the ridge faces it, more so near it.
            let facing = range_top(spec, x + 2) > top || (x - SUN_X).abs() < 90;
            if facing {
                canvas.put(x, top, spec.edge);
            }
            // Haze at the foot of each range, dithered.
            for y in top + 1..top + 8 {
                let bayer = BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64;
                if (1.0 - (y - top) as f64 / 8.0) * 6.0 > bayer + 2.0 {
                    canvas.put(x, y, mix(spec.body, spec.edge, 0.35));
                }
            }
        }
    }
}

fn draw_ground(canvas: &mut Canvas) {
    for x in 0..W {
        let top = ground_top(x as f64);
        for y in top..H {
            let depth = (y - top) as f64 / std::cmp::max(1, H - top) as f64;
            let bayer = BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64;
            let color = if depth * 16.0 > bayer + 4.0 { GROUND_DARK } else { GROUND };
            canvas.put(x, y, color);
        }
        canvas.put(x, top, if x > 120 { GROUND_RIM } else { GRASS[2] });
    }
    // The tree throws a long shadow to the left, away from the low sun.
    for y in 140..H {
        for x in 0..TRUNK_X + 10 {
            let top = ground_top(x as f64);
            if y <= top {
                continue;
            }
            let u = (TRUNK_X + 6 - x) as f64 / 200.0;
            let v = (y as f64 - ((TRUNK_Y + 2) as f64 + u * 10.0)) / (4.0 + 10.0 * u);
            if (0.0..=1.0).contains(&u) && v.abs() <= 1.0 {
                let strength = (1.0 - v.abs()) * (1.0 - u * 0.7);
                let bayer = BAYER[py_imod(y, 4) as usize][py_imod(x, 4) as usize] as f64;
                if strength * 16.0 > bayer {
                    canvas.put(x, y, SHADOW);
                }
            }
        }
    }
}

// Tree --------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    r: f64,
    dx: f64,
    dy: f64,
    s: f64,
    depth: i64,
}

/// Branches as chains of circles: each circle knows its radius, direction and arc length.
struct Tree {
    rng: Random,
    points: Vec<Point>,
    tips: Vec<(f64, f64, f64)>,
}

/// A point on a cubic Bezier curve, with the same operation order as the Python.
fn cubic(controls: &[(f64, f64); 4], u: f64) -> (f64, f64) {
    let (a, b, c, d) = (
        (1.0 - u).powf(3.0),
        3.0 * u * (1.0 - u).powf(2.0),
        3.0 * u * u * (1.0 - u),
        u.powf(3.0),
    );
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = *controls;
    (a * x0 + b * x1 + c * x2 + d * x3, a * y0 + b * y1 + c * y2 + d * y3)
}

impl Tree {
    #[allow(clippy::too_many_arguments)]
    fn grow(
        &mut self,
        mut x: f64,
        mut y: f64,
        angle: f64,
        length: f64,
        r0: f64,
        r1: f64,
        depth: i64,
        mut s: f64,
        bend: f64,
        max_depth: i64,
    ) {
        let steps = py_round(length / 1.5).max(2);
        let mut heading = angle;
        for step in 0..=steps {
            let u = step as f64 / steps as f64;
            let r = r0 + (r1 - r0) * u;
            let (dx, dy) = (heading.cos(), -heading.sin());
            self.points.push(Point { x, y, r, dx, dy, s, depth });
            // Organic line: a slow random walk plus a bend toward a preferred direction.
            let jitter = self.rng.uniform(-0.1, 0.1);
            heading += jitter + bend / steps as f64;
            if y < 16.0 {
                // Keep the crown inside the frame: near the top, branches level out.
                let flat = if heading.cos() >= 0.0 { 0.0 } else { PI };
                heading += (flat - heading) * 0.25;
            }
            x += dx * 1.5;
            y += dy * 1.5;
            s += 1.5;
        }
        if r1 < 0.75 || depth >= max_depth {
            self.tips.push((x, y, heading));
            return;
        }
        let children: i64 = if self.rng.random() < 0.75 { 2 } else { 3 };
        let spread = self.rng.uniform(0.35, 0.55);
        for index in 0..children {
            let offset = (index as f64 - (children - 1) as f64 / 2.0) * spread * 2.0
                / std::cmp::max(1, children - 1) as f64;
            let mut child_angle = heading + offset + self.rng.uniform(-0.12, 0.12);
            // Gentle pull toward the sky: cherry branches reach up and out.
            let cosine = child_angle.cos();
            let sign = if cosine > 0.0 { -1.0 } else { 1.0 };
            child_angle += -(0.15 * cosine * sign);
            let child_length = length * self.rng.uniform(0.6, 0.8);
            let child_bend = self.rng.uniform(-0.3, 0.3);
            self.grow(
                x,
                y,
                child_angle,
                child_length,
                r1 * 0.8,
                r1 * 0.8 * 0.6,
                depth + 1,
                s,
                child_bend,
                max_depth,
            );
        }
    }

    /// A main limb along a cubic curve, sprouting side branches as it goes.
    #[allow(unused_assignments)] // The initial radius is only a placeholder before the loop.
    fn limb(&mut self, controls: [(f64, f64); 4], r0: f64, r1: f64, s0: f64) {
        let (x0, y0) = controls[0];
        let (x1, y1) = controls[1];
        let samples = 160;
        let mut s = s0;
        let mut prev: Option<(f64, f64)> = None;
        let mut next_branch = self.rng.uniform(8.0, 14.0);
        let mut side = if self.rng.choice(&[-1i64, 1]) == -1 { -1.0 } else { 1.0 };
        let (mut x, mut y, mut dx, mut dy) = (0.0, 0.0, 0.0, 0.0);
        let mut r = r0;
        for k in 0..=samples {
            let u = k as f64 / samples as f64;
            (x, y) = cubic(&controls, u);
            if let Some((px, py)) = prev {
                let step = py_hypot(x - px, y - py);
                if step < 1.1 && k < samples {
                    continue;
                }
                s += step;
                dx = (x - px) / step;
                dy = (y - py) / step;
            } else {
                dx = x1 - x0;
                dy = y1 - y0;
                let norm = py_hypot(dx, dy);
                dx /= norm;
                dy /= norm;
            }
            r = r0 + (r1 - r0) * u;
            self.points.push(Point { x, y, r, dx, dy, s, depth: 1 });
            prev = Some((x, y));
            if s - s0 >= next_branch && u < 0.94 {
                let heading = (-dy).atan2(dx);
                // Side branches alternate and tilt upward, shorter toward the end of the limb.
                let mut angle = heading + side * self.rng.uniform(0.55, 0.95);
                if angle.sin() < -0.2 {
                    angle = heading - side * self.rng.uniform(0.55, 0.95);
                }
                let length = self.rng.uniform(12.0, 24.0) * (1.0 - 0.5 * u);
                let bend = self.rng.uniform(-0.3, 0.3);
                self.grow(x, y, angle, length, (r * 0.6).max(1.0), (r * 0.36).max(0.8), 2, s, bend, 4);
                side = -side;
                next_branch = (s - s0) + self.rng.uniform(7.0, 13.0);
            }
        }
        let heading = (-dy).atan2(dx);
        for offset in [-0.4, 0.35] {
            let length = self.rng.uniform(10.0, 16.0);
            self.grow(x, y, heading + offset, length, (r1 * 0.8).max(0.9), 0.6, 3, s, 0.0, 4);
        }
    }
}

fn build_tree() -> Tree {
    let mut tree = Tree { rng: Random::new(7), points: Vec::new(), tips: Vec::new() };
    let fork = (194.0, 116.0);
    // Trunk: a curve from the roots to the fork, thick and slightly twisted.
    let trunk = [
        (TRUNK_X as f64, (TRUNK_Y + 2) as f64),
        ((TRUNK_X + 4) as f64, 142.0),
        (188.0, 128.0),
        fork,
    ];
    let samples = 60;
    let mut s = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for k in 0..=samples {
        let u = k as f64 / samples as f64;
        let (x, y) = cubic(&trunk, u);
        let (dx, dy);
        if let Some((px, py)) = prev {
            let mut step = py_hypot(x - px, y - py);
            if step == 0.0 {
                step = 1.0; // Python: `hypot(...) or 1`.
            }
            s += step;
            dx = (x - px) / step;
            dy = (y - py) / step;
        } else {
            dx = 0.0;
            dy = -1.0;
        }
        // Wider at the foot, where the trunk flares into its roots.
        let r = 7.0 + 3.5 * (1.0 - u * 4.0).max(0.0).powf(2.0) - 1.4 * u;
        tree.points.push(Point { x, y, r, dx, dy, s, depth: 0 });
        prev = Some((x, y));
    }

    // Two leaders leave the fork; the upper limbs split off them further out, like a real crown.
    let far_left = [fork, (150.0, 90.0), (74.0, 108.0), (10.0, 76.0)];
    let far_right = [(fork.0 + 2.0, fork.1 + 1.0), (246.0, 96.0), (316.0, 110.0), (390.0, 72.0)];
    let left_start = cubic(&far_left, 0.18);
    let right_start = cubic(&far_right, 0.16);
    let limbs: [([(f64, f64); 4], f64, f64); 5] = [
        (far_left, 5.2, 1.1),
        ([left_start, (150.0, 78.0), (126.0, 62.0), (108.0, 30.0)], 3.4, 1.0),
        ([(fork.0 + 1.0, fork.1 - 1.0), (210.0, 92.0), (184.0, 52.0), (204.0, 18.0)], 3.6, 0.9),
        ([right_start, (250.0, 84.0), (272.0, 70.0), (288.0, 34.0)], 3.4, 1.0),
        (far_right, 5.2, 1.1),
    ];
    for (controls, r0, r1) in limbs {
        tree.limb(controls, r0, r1, s);
    }

    // Roots flaring into the ground.
    let mut roots = Vec::new();
    for (angle, length) in [(196.0, 12), (226.0, 7), (344.0, 13), (316.0, 7)] {
        let rad = angle * (PI / 180.0);
        let (mut x, mut y) = (TRUNK_X as f64, (TRUNK_Y - 1) as f64);
        for step in 0..length {
            let u = step as f64 / length as f64;
            let r = 4.2 * (1.0 - u) + 0.8;
            roots.push(Point { x, y, r, dx: rad.cos(), dy: -rad.sin(), s: step as f64, depth: 0 });
            x += rad.cos() * 1.2;
            y -= rad.sin() * 1.2;
        }
    }
    roots.append(&mut tree.points);
    tree.points = roots;
    tree
}

/// Cylinder lit from the low sun on the right, with lenticels and cracks on thick wood.
fn bark_color(nx: f64, ny: f64, s: f64, across: f64, r: f64) -> Rgb {
    let light = nx * 0.92 - ny * 0.38;
    let mut color = if light > 0.72 && r > 1.4 {
        if r > 3.0 && light > 0.86 { BARK_HOT } else { BARK_RIM }
    } else if light > 0.25 {
        BARK_LIGHT
    } else if light > -0.45 {
        BARK_MID
    } else {
        BARK_DARK
    };
    if r >= 2.6 && (color == BARK_LIGHT || color == BARK_MID) {
        // Cherry bark: short horizontal lenticels across the wood, and long dark cracks along it.
        let band = py_ffloordiv(s, 3.0) as i64;
        if hash2(band, (across * 1.5 + 9.0) as i64, 3) < 0.16 && across.abs() < r * 0.8 {
            color = LENTICEL;
        } else if hash2((across + 20.0) as i64, py_ffloordiv(s, 7.0) as i64, 5) < 0.12 {
            color = CRACK;
        }
    }
    if r < 1.1 {
        color = if light > -0.2 { BARK_MID } else { BARK_DARK };
    }
    color
}

fn draw_tree(canvas: &mut Canvas, tree: &Tree, mask: &mut HashSet<(i64, i64)>) {
    let mut best: HashMap<(i64, i64), (f64, Rgb)> = HashMap::new();
    for p in &tree.points {
        let reach = p.r.max(0.5);
        for py in (p.y - reach).floor() as i64..=(p.y + reach).ceil() as i64 {
            for px in (p.x - reach).floor() as i64..=(p.x + reach).ceil() as i64 {
                let (vx, vy) = (px as f64 - p.x, py as f64 - p.y);
                let dist = py_hypot(vx, vy);
                if dist > reach + 0.15 {
                    continue;
                }
                let ratio = dist / reach;
                if let Some((best_ratio, _)) = best.get(&(px, py))
                    && *best_ratio <= ratio {
                        continue;
                    }
                let (nx, ny) = if dist != 0.0 { (vx / reach, vy / reach) } else { (0.0, 0.0) };
                let along = p.s + vx * p.dx + vy * p.dy;
                let across = vx * -p.dy + vy * p.dx;
                best.insert((px, py), (ratio, bark_color(nx, ny, along, across, p.r)));
            }
        }
    }
    for ((px, py), (_, color)) in best {
        if (0..W).contains(&px) && (0..H).contains(&py) {
            canvas.put(px, py, color);
            mask.insert((px, py));
        }
    }
}

/// Every place a blossom can grow: all twig tips, plus spots along the thin branches.
/// Returned sorted, with near-duplicates dropped.
fn blossom_sites(tree: &Tree) -> Vec<(f64, f64)> {
    let mut sites: Vec<(f64, f64)> = tree.tips.iter().map(|&(x, y, _)| (x, y)).collect();
    let mut last: HashMap<(i64, i64, i64), f64> = HashMap::new();
    for p in &tree.points {
        if p.depth < 1 || !(0.7 <= p.r && p.r <= 2.0) {
            continue;
        }
        let key = (p.depth, round1(p.dx), round1(p.dy));
        let previous = *last.get(&key).unwrap_or(&-99.0);
        if (p.s - previous).abs() >= 5.0 {
            sites.push((p.x, p.y));
            last.insert(key, p.s);
        }
    }
    sites.retain(|&(x, y)| 8.0 < x && x < (W - 8) as f64 && 10.0 < y && y < 118.0);
    sites.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Drop near-duplicates, so clusters spread over the crown instead of piling up.
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for (x, y) in sites {
        let start = kept.len().saturating_sub(40);
        if kept[start..].iter().all(|&(kx, ky)| (x - kx).abs() + (y - ky).abs() > 3.5) {
            kept.push((x, y));
        }
    }
    kept
}

struct Cluster {
    x: f64,
    y: f64,
    level: f64,
    size: f64,
    flowers: Vec<(f64, f64, bool)>,
    puffs: Vec<(f64, f64, f64)>,
    spread: f64,
    phase: f64,
}

/// The crown is cut into as many vertical slices as there are days, oldest on the left.
/// Each slice holds the same number of blossom sites, and every site in a slice blooms
/// with that day's contributions.
fn make_clusters(tree: &Tree, values: &[i64], rng: &mut Random) -> Vec<Cluster> {
    let sites = blossom_sites(tree);
    let count = values.len();
    let mut clusters = Vec::with_capacity(sites.len());
    for (index, &(x, y)) in sites.iter().enumerate() {
        let day = if count > 0 { (count - 1).min(index * count / sites.len()) } else { 0 };
        let level = if count > 0 { log_level(values[day], FULL_BLOOM_AT) } else { 0.0 };
        let size = level * rng.uniform(0.8, 1.1);
        let mut flowers = Vec::new();
        let mut puffs = Vec::new();
        let spread = 1.0 + 4.6 * size;
        if level > 0.0 {
            let n = 1 + py_round(size * 3.5);
            for k in 0..n {
                let a = rng.uniform(0.0, 2.0 * PI);
                let d = if k > 0 { spread * rng.random().sqrt() } else { 0.0 };
                let fx = a.cos() * d;
                let fy = a.sin() * d * 0.75 - size;
                flowers.push((fx, fy, size > 0.3 && (k as f64) < 1.0 + size * 4.0));
            }
            flowers.sort_by(|a, b| (a.1 - a.0 * 0.3).partial_cmp(&(b.1 - b.0 * 0.3)).unwrap());
            if size > 0.22 {
                let m = 2 + py_round(size * 3.0);
                for k in 0..m {
                    let a = rng.uniform(0.0, 2.0 * PI);
                    let d = if k > 0 { spread * 0.5 * rng.random().sqrt() } else { 0.0 };
                    let radius = spread * rng.uniform(0.6, 0.85);
                    puffs.push((a.cos() * d, a.sin() * d * 0.7 - size, radius));
                }
            }
        }
        let phase = rng.uniform(0.0, 6.3);
        clusters.push(Cluster { x, y, level, size, flowers, puffs, spread, phase });
    }
    clusters
}

/// Colours of a blossom by shade: 0 lit, 1 middle, 2 shadow.
fn blossom_colors(shade: i64) -> SpriteColors {
    match shade {
        0 => [PINK_L, PINK_WARM, PINK_P, PINK_C],
        1 => [PINK_L, PINK_P, PINK_S, PINK_C],
        _ => [PINK_P, PINK_S, PINK_D, PINK_D],
    }
}

fn cluster_offset(cluster: &Cluster, t: f64) -> (i64, i64) {
    // A slow wave of wind crossing the crown from left to right, one cycle per loop.
    let wave = (2.0 * PI * t - cluster.x / 70.0 + cluster.phase * 0.3).sin();
    (py_round(0.9 * wave), 0)
}

fn draw_cluster(canvas: &mut Canvas, cluster: &Cluster, ox: i64, oy: i64) {
    let (cx, cy) = (cluster.x + ox as f64, cluster.y + oy as f64);
    if cluster.level == 0.0 {
        // Bare twig: a closed bud, nothing more.
        canvas.put(cx, cy, BUD_DARK);
        canvas.put(cx, cy - 1.0, BUD);
        return;
    }
    let spread = cluster.spread;
    if !cluster.puffs.is_empty() {
        // A rounded mass of petals behind the blossoms, shaded like a ball lit from the right.
        let mut cells: HashMap<(i64, i64), f64> = HashMap::new();
        for &(px_off, py_off, radius) in &cluster.puffs {
            for py in (cy + py_off - radius).floor() as i64..=(cy + py_off + radius).ceil() as i64 {
                for px in (cx + px_off - radius).floor() as i64..=(cx + px_off + radius).ceil() as i64 {
                    let (ox_rel, oy_rel) = ((px as f64 - cx) - px_off, (py as f64 - cy) - py_off);
                    let d = py_hypot(ox_rel, oy_rel * 1.15) / radius;
                    if d <= 1.0 {
                        let nx = ox_rel / radius;
                        let ny = oy_rel / radius;
                        let light = nx * 0.75 - ny * 0.55;
                        let previous = cells.get(&(px, py)).copied().unwrap_or(-9.0);
                        cells.insert((px, py), if light > previous { light } else { previous });
                    }
                }
            }
        }
        for ((px, py), light) in cells {
            let color = if light > 0.35 {
                PINK_P
            } else if light > -0.15 {
                PINK_S
            } else {
                PINK_D
            };
            canvas.put(px, py, color);
        }
    }
    for &(fx, fy, big) in &cluster.flowers {
        let light = fx * 0.8 - fy * 0.6;
        let shade = if light > spread * 0.35 {
            0
        } else if light < -spread * 0.35 {
            2
        } else {
            1
        };
        let sprite: &[&str] = if big { &BLOSSOM } else { &BLOSSOM_SMALL };
        let half = sprite.len() as i64 / 2;
        draw_sprite(
            canvas,
            sprite,
            py_round(cx + fx) - half,
            py_round(cy + fy) - half,
            &blossom_colors(shade),
        );
    }
    // A few golden stamens catching the light.
    if cluster.level > 0.5 {
        canvas.put(cx + 1.0, cy - 1.0, PINK_Y);
    }
}

// Petals: one per merged pull request -----------------------------------------------------

struct Petal {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    land: f64,
    rest: &'static [&'static str],
    flutter: i64,
    amp: f64,
    spin: i64,
    phase: f64,
}

/// A point on the grass under the crown, denser near the trunk, never under the counter.
fn landing_spot(rng: &mut Random) -> (f64, f64) {
    loop {
        let x = TRUNK_X as f64 + rng.gauss(0.0, 85.0);
        if !(8.0 < x && x < (W - 8) as f64) {
            continue;
        }
        let top = ground_top(py_round(x) as f64) + 2;
        // Depth into the meadow: mostly near the foot of the knoll, some closer to us.
        let y = top as f64 + ((H - 3 - top) as f64) * rng.random().powf(1.6);
        if x >= (COUNTER_BOX.0 - 4) as f64 && y >= (COUNTER_BOX.1 - 3) as f64 {
            continue;
        }
        return (x, y);
    }
}

fn make_petals(count: i64, clusters: &[Cluster], tree: &Tree, rng: &mut Random) -> Vec<Petal> {
    let live: Vec<(f64, f64)> = clusters.iter().filter(|c| c.level > 0.0).map(|c| (c.x, c.y)).collect();
    let sources: Vec<(f64, f64)> = if live.is_empty() {
        tree.tips.iter().map(|&(x, y, _)| (x, y)).collect()
    } else {
        live
    };
    let mut petals = Vec::with_capacity(count as usize);
    for index in 0..count {
        let (land_x, land_y) = landing_spot(rng);
        let drift = rng.uniform(WIND_DRIFT.0, WIND_DRIFT.1);
        // Python's `min(key=...)` draws one random number per candidate, in order, and keeps
        // the first minimum.
        let mut chosen: Option<((f64, f64), f64)> = None;
        for &(sx, sy) in &sources {
            let key = (sx - (land_x - drift)).abs() + rng.uniform(0.0, 30.0);
            let better = match chosen {
                None => true,
                Some((_, best)) => key < best,
            };
            if better {
                chosen = Some(((sx, sy), key));
            }
        }
        let ((src_x, src_y), _) = chosen.expect("petals need at least one source");
        // Landing times spread evenly over the falling phase, one petal after the other.
        let land = FALL_TIME + (FALL_END - FALL_TIME) * (index + 1) as f64 / std::cmp::max(1, count) as f64;
        let x0 = src_x + rng.uniform(-2.0, 2.0);
        let rest = rng.choice(&PETAL_RESTS);
        let flutter = rng.choice(&[1i64, 2]);
        let amp = rng.uniform(3.0, 7.0);
        let spin = rng.choice(&[3i64, 4, 5]);
        let phase = rng.uniform(0.0, 6.3);
        petals.push(Petal { x0, y0: src_y, x1: land_x, y1: land_y, land, rest, flutter, amp, spin, phase });
    }
    petals
}

/// Draws the carpet and the petals in the air; returns how many have landed.
fn draw_petals(canvas: &mut Canvas, petals: &[Petal], t: f64) -> i64 {
    let mut landed = 0;
    let rest: SpriteColors = [PINK_L, PINK_P, PINK_D, PINK_D];
    for petal in petals {
        if t >= petal.land {
            landed += 1;
            draw_sprite(canvas, petal.rest, py_round(petal.x1), py_round(petal.y1), &rest);
        }
    }
    let flying: SpriteColors = [PINK_L, PINK_P, PINK_S, PINK_S];
    for petal in petals {
        let start = petal.land - FALL_TIME;
        if start <= t && t < petal.land {
            let f = (t - start) / FALL_TIME;
            // Pushed by the wind: slow sideways at first, then carried as it drops.
            let mut x = petal.x0 + (petal.x1 - petal.x0) * (f * f * 0.5 + f * 0.5);
            x += petal.amp
                * (2.0 * PI * petal.flutter as f64 * f + petal.phase).sin()
                * (PI * f).sin();
            let y = petal.y0 + (petal.y1 - petal.y0) * (f * f * 0.35 + f * 0.65);
            let view = PETAL_VIEWS[py_imod((f * petal.spin as f64 * 4.0) as i64, 4) as usize];
            draw_sprite(canvas, view, py_round(x), py_round(y), &flying);
        }
    }
    landed
}

/// A petal icon and the number of merged pull requests, bottom right on the dark grass.
fn draw_counter(canvas: &mut Canvas, shown: i64, total: i64, pop: bool) {
    let label = format!("{shown} PRS MERGED");
    let x = (W - 10 - 14) - text_width(&label, Font::F57, 2);
    let y = H - 19;
    let icon_y = y + 1 - if pop { 1 } else { 0 };
    for (dy, row) in PETAL_ICON.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            if key != b'.' {
                canvas.put(x + dx as i64 * 2 + 1, icon_y + dy as i64 * 2 + 1, COUNTER_SHADOW);
            }
        }
    }
    for (dy, row) in PETAL_ICON.iter().enumerate() {
        for (dx, key) in row.bytes().enumerate() {
            if key != b'.' {
                let color = match key {
                    b'L' => PINK_L,
                    b'P' => PINK_P,
                    _ => PINK_S,
                };
                for sy in 0..2 {
                    for sx in 0..2 {
                        canvas.put(x + dx as i64 * 2 + sx, icon_y + dy as i64 * 2 + sy, color);
                    }
                }
            }
        }
    }
    let color = if total != 0 { COUNTER_TEXT } else { mix(COUNTER_TEXT, COUNTER_SHADOW, 0.35) };
    canvas.text((x + 14) as f64, y as f64, &label, color, Font::F57, 2, Some(COUNTER_SHADOW));
}

// Foreground grass ---------------------------------------------------------------------------

struct Blade {
    x: f64,
    root: i64,
    height: f64,
    phase: f64,
    lean: f64,
}

fn make_grass(rng: &mut Random) -> Vec<Blade> {
    let mut blades = Vec::with_capacity(190);
    for _ in 0..190 {
        let x = rng.uniform(0.0, W as f64);
        let top = ground_top(py_round(x) as f64);
        let root = rng.randint(top + 1, H + 2);
        let height = rng.uniform(2.0, 5.0) * (0.6 + 0.6 * (root - top) as f64 / 24.0);
        let phase = rng.uniform(0.0, 6.3);
        let lean = rng.uniform(-0.4, 0.4);
        blades.push(Blade { x, root, height, phase, lean });
    }
    blades.sort_by_key(|blade| blade.root); // Stable, like Python's sort.
    blades
}

fn draw_grass(canvas: &mut Canvas, blades: &[Blade], t: f64) {
    for blade in blades {
        let sway = blade.lean + 0.35 * (2.0 * PI * t - blade.x / 70.0 + blade.phase * 0.2).sin();
        let steps = std::cmp::max(1, py_round(blade.height));
        // Only the blades on the crest of the knoll catch the sun.
        let sunlit = blade.x > 110.0 && blade.root - ground_top(py_round(blade.x) as f64) < 4;
        for step in 0..=steps {
            let u = step as f64 / steps as f64;
            let px = blade.x + sway * blade.height * u * u;
            let color = if u < 0.5 {
                GRASS[0]
            } else if sunlit && u > 0.7 {
                GRASS[3]
            } else {
                GRASS[1]
            };
            canvas.put(px, blade.root - step, color);
        }
    }
}

// Widget ----------------------------------------------------------------------------------

pub fn render(data: &Data) -> String {
    let mut rng = Random::new("blossom"); // Seed word: changing it redraws the scene.
    let values: Vec<i64> = data.days.iter().map(|day| day.count).collect();
    let total_prs = data.merged_prs_30d;

    let mut base = Canvas::with_background(W, H, [0, 0, 0]);
    draw_sky(&mut base);
    let stars = make_stars(&mut rng);
    draw_sun(&mut base);
    draw_clouds(&mut base);
    draw_ranges(&mut base);
    draw_ground(&mut base);
    let tree = build_tree();
    let mut mask = HashSet::new();
    draw_tree(&mut base, &tree, &mut mask);
    let clusters = make_clusters(&tree, &values, &mut rng);
    let shown_petals = MAX_PETALS.min(total_prs);
    let petals = make_petals(shown_petals, &clusters, &tree, &mut rng);
    let grass = make_grass(&mut rng);
    let mut ordered: Vec<&Cluster> = clusters.iter().collect();
    ordered.sort_by(|a, b| a.size.partial_cmp(&b.size).unwrap()); // Stable, like Python's sort.

    let mut frames: Vec<Canvas> = Vec::with_capacity(LOOP);
    for index in 0..LOOP {
        let t = index as f64 / LOOP as f64;
        let mut canvas = base.clone();
        for star in &stars {
            if mask.contains(&(star.x, star.y)) {
                continue;
            }
            let value = (2.0 * PI * star.cycles as f64 * t + star.phase).sin();
            if value > -0.3 {
                let color = if star.big && value > 0.6 { STAR[1] } else { STAR[0] };
                canvas.put(star.x, star.y, color);
            }
        }
        for cluster in &ordered {
            let (ox, oy) = cluster_offset(cluster, t);
            draw_cluster(&mut canvas, cluster, ox, oy);
        }
        draw_grass(&mut canvas, &grass, t);
        let landed = draw_petals(&mut canvas, &petals, t);
        // The counter follows the landed petals; above 60 it scales up to the real number.
        let shown = if landed == shown_petals {
            total_prs
        } else {
            py_round((landed * total_prs) as f64 / std::cmp::max(1, shown_petals) as f64)
        };
        let just_landed = petals.iter().any(|petal| (t - petal.land).abs() < 1.5 / LOOP as f64);
        draw_counter(&mut canvas, shown, total_prs, just_landed);
        frames.push(canvas);
    }
    save(&frames, "cherry-tree", Save { still: Some(LOOP - 1), duration: FRAME_MS, ..Save::default() })
}
