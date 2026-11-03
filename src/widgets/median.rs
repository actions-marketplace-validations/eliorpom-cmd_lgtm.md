//! Port of the Python prototype. You vs the median: two builders raise brick towers, one for your
//! contributions over the last 12 months, one for the median of the accounts you follow.
//! Each builder works from a scaffold beside the tower: he crouches to take a brick, lifts
//! it overhead, walks to its slot, lowers it and walks back. When a course is complete the
//! platform hoists him up one course. The shorter tower is finished first and its builder
//! turns to watch. Then a dashed line carries the median height across to your tower.

use crate::data::{Data, Followed};
use crate::export::{save, Save};
use crate::pixel::{dither_on, Canvas, Font, Rgb, TEXT};
use crate::pymath::{py_floordiv, py_imod, py_median, py_mod, py_round};

const W: i64 = 280;
const H: i64 = 160;
const GROUND: i64 = 98; // First row of ground under the towers.
const COURSE: i64 = 8; // One brick: 7 px of clay + 1 px mortar line.
const BRICK_W: i64 = 18;
const MAX_BRICKS: i64 = 9; // The taller pillar is 9 bricks high.
const CENTERS: [i64; 2] = [72, 208];
const FRAME_MS: u32 = 70;
const WALK: i64 = 2; // Pixels per frame while walking.
const PLATFORM_W: i64 = 28;
const HOLD_SHARE: f64 = 0.32; // Share of the loop spent on the finished scene.

const SKY: [Rgb; 6] = [[14, 17, 34], [20, 24, 46], [30, 32, 62], [46, 42, 80], [70, 54, 96], [100, 68, 104]];
const HILL_FAR: Rgb = [36, 36, 70];
const HILL_NEAR: Rgb = [28, 30, 56];
const STAR: [Rgb; 3] = [[120, 130, 170], [200, 210, 240], [255, 255, 255]];
const GRASS: [Rgb; 3] = [[36, 84, 52], [58, 122, 66], [96, 160, 84]];
const SOIL: [Rgb; 4] = [[20, 16, 20], [28, 22, 26], [40, 30, 32], [56, 40, 38]];
const OUTLINE: Rgb = [18, 14, 26];
const TIMBER: [Rgb; 4] = [[58, 38, 26], [92, 62, 40], [128, 90, 56], [164, 122, 78]];
const WOOD: [Rgb; 4] = [[70, 46, 30], [104, 70, 44], [132, 92, 58], [166, 120, 78]];
const DASH: Rgb = [236, 244, 255];
const SPARKLE: Rgb = [255, 236, 150];

/// Colours of one tower: its bricks and mortar, the label, and the builder's own sprite keys
/// (`h`, `H`, `c`, `C`, `v`). The shared keys (`s`, `S`, `e`, `p`, `P`, `k`) are the same for both.
#[derive(Clone, Copy)]
struct Side {
    brick: [Rgb; 4],
    mortar: Rgb,
    label: Rgb,
    h: Rgb,
    hh: Rgb,
    c: Rgb,
    cc: Rgb,
    v: Rgb,
}

const YOU: Side = Side {
    brick: [[96, 36, 32], [150, 62, 44], [194, 92, 60], [228, 136, 92]],
    mortar: [78, 48, 50],
    label: [255, 168, 96],
    h: [250, 204, 64],
    hh: [198, 146, 36],
    c: [232, 112, 48],
    cc: [178, 74, 36],
    v: [255, 236, 150],
};

const THEM: Side = Side {
    brick: [[44, 56, 88], [74, 92, 132], [112, 136, 180], [164, 186, 222]],
    mortar: [38, 42, 64],
    label: [132, 180, 255],
    h: [226, 232, 242],
    hh: [160, 170, 190],
    c: [70, 124, 206],
    cc: [44, 84, 150],
    v: [190, 226, 255],
};

fn sprite_color(side: &Side, key: u8) -> Rgb {
    match key {
        b'h' => side.h,
        b'H' => side.hh,
        b'c' => side.c,
        b'C' => side.cc,
        b'v' => side.v,
        b's' => [242, 198, 152],
        b'S' => [204, 150, 110],
        b'e' => [30, 22, 34],
        b'p' => [52, 62, 104],
        b'P' => [36, 42, 76],
        b'k' => [70, 44, 34],
        other => panic!("unknown sprite key {}", other as char),
    }
}

// Hand-drawn builder, 8 px wide, drawn facing right; feet on the last row.
const HEAD: [&str; 7] = ["..hhhh..", ".hhhhhh.", "HHHHHHHH", ".ssssss.", ".sssese.", ".ssssSs.", "..sSSs.."];
const HEAD_FRONT: [&str; 7] = ["..hhhh..", ".hhhhhh.", "HHHHHHHH", ".ssssss.", ".sesses.", ".ssssSs.", "..sSSs.."];
const BODY: [&str; 4] = ["cccvvccC", "c.cvvc.C", "c.cccC.C", "s.cccC.S"];
const BODY_UP: [&str; 4] = [".ccvvcC.", ".ccvvcC.", ".cccccC.", "..cccC.."];
const ARMS_UP: [&str; 3] = ["s......S", "c.hhhh.C", "chhhhhhC"];
const LEGS_STAND: [&str; 4] = ["..pppP..", "..pp.P..", "..pp.P..", ".kkk.kk."];
const LEGS_WALK_A: [&str; 4] = ["..pppP..", ".pp..P..", ".p...PP.", "kk....kk"];
const LEGS_WALK_B: [&str; 4] = ["..pppP..", "..ppP...", "...pP...", "..kkkk.."];
// Crouching with both hands reaching forward and down, to take or lay a brick.
const CROUCH: [&str; 12] = [
    "..hhhh..", ".hhhhhh.", "HHHHHHHH", ".ssssss.", ".sssese.", ".ssssSs.", "cccvvccC", ".ccvvcCcs", ".cccccC.s",
    "pppPPpp..", "pp...pP..", "kk...kk..",
];
const QUESTION: [&str; 7] = [".111.", "1...1", "....1", "...1.", "..1..", ".....", "..1.."];

#[derive(Clone, Copy)]
enum Gait {
    Stand,
    A,
    B,
}

#[derive(Clone, Copy)]
enum Set {
    Up,
    Empty,
}

fn legs(gait: Gait) -> &'static [&'static str] {
    match gait {
        Gait::Stand => &LEGS_STAND,
        Gait::A => &LEGS_WALK_A,
        Gait::B => &LEGS_WALK_B,
    }
}

fn join(parts: &[&[&str]]) -> Vec<String> {
    parts.iter().flat_map(|part| part.iter().map(|row| row.to_string())).collect()
}

fn stand() -> Vec<String> {
    join(&[&HEAD[..], &BODY[..], &LEGS_STAND[..]])
}

fn front() -> Vec<String> {
    join(&[&HEAD_FRONT[..], &BODY[..], &LEGS_STAND[..]])
}

fn front_up() -> Vec<String> {
    join(&[&ARMS_UP[..], &HEAD_FRONT[2..], &BODY_UP[..], &LEGS_STAND[..]])
}

/// Arms raised to hold a brick overhead (the brick is drawn on top separately).
fn up(gait: Gait) -> Vec<String> {
    join(&[&ARMS_UP[..], &HEAD[2..], &BODY_UP[..], legs(gait)])
}

fn walk_empty(gait: Gait) -> Vec<String> {
    join(&[&HEAD[..], &BODY[..], legs(gait)])
}

fn crouch() -> Vec<String> {
    join(&[&CROUCH[..]])
}

impl Set {
    fn rows(self, gait: Gait) -> Vec<String> {
        match self {
            Set::Up => up(gait),
            Set::Empty => walk_empty(gait),
        }
    }
}

/// Mirror a sprite: pad each row on the right to the widest row, then reverse it.
fn mirror(rows: &[String]) -> Vec<String> {
    let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    rows.iter()
        .map(|row| {
            let mut padded = row.clone();
            while padded.len() < width {
                padded.push('.');
            }
            padded.chars().rev().collect()
        })
        .collect()
}

/// Deterministic pseudo-random value in [0, 1) from integer coordinates, like `hash01` in Python.
fn hash01(x: i64, y: i64, seed: i64) -> f64 {
    let mut n = (x * 374761393 + y * 668265263 + seed * 2147483647) & 0xFFFF_FFFF;
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFF_FFFF;
    ((n ^ (n >> 16)) & 0xFFFF) as f64 / 65536.0
}

/// Colour ramp: `value` picks between neighbouring colours, dithered with the Bayer matrix.
fn ramp(colors: &[Rgb], value: f64, x: i64, y: i64) -> Rgb {
    let last = (colors.len() - 1) as f64;
    let value = value.min(last).max(0.0);
    let mut low = value as usize;
    if low < colors.len() - 1 && dither_on(x, y, value - low as f64) {
        low += 1;
    }
    colors[low]
}

/// Draw a sprite with its feet on `bottom`, plus a 1 px dark outline around the silhouette.
fn sprite(canvas: &mut Canvas, rows: &[String], x: i64, bottom: i64, side: &Side) {
    let top = bottom - rows.len() as i64 + 1;
    let is_filled = |cx: i64, cy: i64| -> bool {
        if cy < 0 || cy >= rows.len() as i64 || cx < 0 {
            return false;
        }
        match rows[cy as usize].as_bytes().get(cx as usize) {
            Some(&key) => key != b'.',
            None => false,
        }
    };
    for (cy, line) in rows.iter().enumerate() {
        for cx in 0..line.len() as i64 {
            let (cy, cx) = (cy as i64, cx);
            if !is_filled(cx, cy) {
                continue;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if !is_filled(cx + dx, cy + dy) {
                    canvas.put(x + cx + dx, top + cy + dy, OUTLINE);
                }
            }
        }
    }
    for (cy, line) in rows.iter().enumerate() {
        for (cx, key) in line.bytes().enumerate() {
            if key != b'.' {
                canvas.put(x + cx as i64, top + cy as i64, sprite_color(side, key));
            }
        }
    }
}

/// One brick, 7 px high, lit from the upper left, with a few pores in the clay.
fn brick(canvas: &mut Canvas, x0: i64, y0: i64, width: i64, colors: &[Rgb; 4], seed: i64, outline: bool) {
    if outline {
        canvas.rect(x0 - 1, y0 - 1, x0 + width, y0 + 7, OUTLINE);
    }
    let tone = 1.7 + (hash01(seed, 0, 21) - 0.5) * 0.8;
    for y in y0..y0 + 7 {
        for x in x0..x0 + width {
            let mut level = tone;
            if y == y0 {
                level += 1.4;
            } else if y == y0 + 6 || x == x0 + width - 1 {
                level -= 1.0;
            } else if x == x0 {
                level += 0.5;
            }
            if hash01(x - x0 + seed * 17, y - y0, 22) > 0.9 {
                level -= 0.7;
            }
            canvas.put(x, y, ramp(colors, level, x, y));
        }
    }
}

/// The pillar: bricks stacked with a mortar line under each.
fn tower(canvas: &mut Canvas, cx: i64, laid: i64, side: &Side) {
    let x0 = cx - BRICK_W / 2;
    for index in 0..laid {
        let y0 = GROUND - (index + 1) * COURSE;
        canvas.rect(x0, y0 + COURSE - 1, x0 + BRICK_W - 1, y0 + COURSE - 1, side.mortar);
        brick(canvas, x0, y0, BRICK_W, &side.brick, index, false);
    }
}

/// Two timber poles, zigzag braces, the hoist platform and the stack of bricks waiting on it.
fn scaffold(canvas: &mut Canvas, cx: i64, out: i64, platform: i64, stack: i64, side: &Side) {
    let (_, left, right, stack_x, _, _) = layout(cx, out);
    for pole in [left + 1, right - 1] {
        canvas.rect(pole, platform, pole + 1, GROUND - 1, TIMBER[1]);
        canvas.rect(pole + 1, platform, pole + 1, GROUND - 1, TIMBER[0]);
    }
    if GROUND - platform > 6 {
        // Zigzag braces between the poles, one per two bricks of height.
        let span = right - left - 4;
        let mut y = GROUND - 2;
        let mut flip = false;
        while y - 14 > platform {
            for k in 0..span {
                let yy = y - py_round((k * 14) as f64 / span as f64);
                let xx = left + 3 + if !flip { k } else { span - 1 - k };
                canvas.put(xx, yy, TIMBER[0]);
            }
            y -= 14;
            flip = !flip;
        }
    }
    canvas.rect(left, platform, right, platform + 2, TIMBER[2]);
    canvas.rect(left, platform, right, platform, TIMBER[3]);
    canvas.rect(left, platform + 2, right, platform + 2, TIMBER[0]);
    let mut nail = left + 3;
    while nail < right {
        canvas.put(nail, platform + 1, TIMBER[1]);
        nail += 6;
    }
    // Bricks waiting on the platform, stacked crosswise with a little irregularity.
    for k in 0..stack {
        let y = platform - 4 - k * 4;
        let sx = stack_x - 1 + (k % 2);
        canvas.rect(sx - 1, y - 1, sx + 8, y + 3, OUTLINE);
        canvas.rect(sx, y, sx + 7, y + 2, side.brick[1]);
        canvas.rect(sx, y, sx + 6, y, side.brick[3]);
        canvas.rect(sx + 7, y, sx + 7, y + 2, side.brick[0]);
        canvas.put(sx + 3, y + 1, side.brick[2]);
    }
}

/// Wooden sign with a painted question mark, for an account that follows nobody.
fn empty_lot_sign(canvas: &mut Canvas, cx: i64) {
    for post in [cx - 10, cx + 8] {
        canvas.rect(post, GROUND - 12, post + 1, GROUND - 1, WOOD[1]);
        canvas.rect(post + 1, GROUND - 12, post + 1, GROUND - 1, WOOD[0]);
    }
    let (x0, y0, x1, y1) = (cx - 15, GROUND - 32, cx + 14, GROUND - 12);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let plank = py_floordiv(y - y0, 7);
            let mut level = 2.1 + (hash01(py_floordiv(x, 6), y, 60 + plank) - 0.5) * 0.7;
            if py_imod(y - y0, 7) == 6 {
                level = 0.6; // Gap between planks.
            }
            if y == y0 || x == x0 {
                level += 0.9;
            }
            if y == y1 || x == x1 {
                level = 0.0;
            }
            canvas.put(x, y, ramp(&WOOD, level, x, y));
        }
    }
    for (nx, ny) in [(x0 + 2, y0 + 2), (x1 - 2, y0 + 2), (x0 + 2, y1 - 3), (x1 - 2, y1 - 3)] {
        canvas.put(nx, ny, [60, 60, 70]);
    }
    for (row, line) in QUESTION.iter().enumerate() {
        for (col, bit) in line.bytes().enumerate() {
            if bit == b'1' {
                let px = cx - 5 + col as i64 * 2;
                let py = y0 + 3 + row as i64 * 2;
                canvas.rect(px + 1, py + 1, px + 2, py + 2, WOOD[0]);
                canvas.rect(px, py, px + 1, py + 1, [246, 232, 196]);
            }
        }
    }
}

fn background() -> Canvas {
    let mut canvas = Canvas::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let xf = x as f64;
            let color = if y < GROUND {
                let level = (y as f64 / GROUND as f64).powf(1.4) * (SKY.len() - 1) as f64;
                let mut color = ramp(&SKY, level, x, y);
                // Two ranges of rolling hills, sums of slow sines so no two humps match.
                let far = 76.0 + 5.0 * (xf / 23.0 + 1.3).sin() + 3.0 * (xf / 9.7).sin() + 2.0 * (xf / 41.0 + 4.0).sin();
                let near = 89.0 + 3.0 * (xf / 17.0 + 0.4).sin() + 2.0 * (xf / 7.3 + 2.0).sin() + 1.5 * (xf / 37.0).sin();
                if y as f64 > near {
                    color = HILL_NEAR;
                } else if y as f64 > far {
                    color = HILL_FAR;
                }
                color
            } else if y < GROUND + 3 {
                // Grass lip with a ragged top edge.
                let mut color = if y == GROUND && hash01(x, 0, 41) > 0.35 {
                    GRASS[2]
                } else if y == GROUND + 1 {
                    GRASS[1]
                } else {
                    GRASS[0]
                };
                if y == GROUND && hash01(x, 0, 41) <= 0.35 {
                    color = GRASS[1];
                }
                color
            } else {
                let depth = (y - GROUND - 3) as f64 / 8.0;
                let level = 2.6 - (depth * 1.8).min(2.6) + (hash01(x, y, 42) - 0.5) * 0.6;
                ramp(&SOIL, level, x, y)
            };
            canvas.put(x, y, color);
        }
    }
    // A crescent moon, lit from the left.
    let (mx, my) = (262i64, 18i64);
    for y in my - 7..=my + 7 {
        for x in mx - 7..=mx + 7 {
            let (dx, dy) = (x - mx, y - my);
            let inside = dx * dx + dy * dy <= 42;
            let bite = (dx - 3) * (dx - 3) + (dy + 2) * (dy + 2) <= 34;
            if inside && !bite {
                let rim = dx * dx + dy * dy > 30;
                canvas.put(x, y, if !rim { [236, 228, 200] } else { [196, 186, 160] });
            } else if dx * dx + dy * dy <= 72 && !bite && dither_on(x, y, 0.25) {
                canvas.put(x, y, SKY[2]);
            }
        }
    }
    // Tufts poking above the grass line.
    for x in 2..W - 2 {
        if hash01(x, 1, 43) > 0.86 {
            canvas.put(x, GROUND - 1, GRASS[1]);
            if hash01(x, 2, 43) > 0.5 {
                canvas.put(x, GROUND - 2, GRASS[0]);
            }
        }
    }
    canvas
}

/// (x, y, twinkle cycles, phase) for each star.
fn stars() -> Vec<(i64, i64, i64, f64)> {
    (0..26)
        .map(|i| {
            let x = (hash01(i, 1, 51) * (W - 8) as f64) as i64 + 4;
            let y = (hash01(i, 2, 51) * 60.0) as i64 + 3;
            let cycles = (hash01(i, 3, 51) * 3.0) as i64 + 1;
            (x, y, cycles, hash01(i, 4, 51))
        })
        .collect()
}

/// Frame-by-frame state of one builder laying bricks on his pillar.
#[derive(Clone)]
struct State {
    x: i64,
    feet: i64,
    rows: Vec<String>,
    laid: i64,
    held: Option<(i64, i64)>,
    platform: i64,
    stack: i64,
}

struct Plan {
    out: i64,
    count: i64,
    laid: i64,
    states: Vec<State>,
}

impl Plan {
    fn push(&mut self, x: i64, rows: Vec<String>, toward_pillar: bool, held: Option<(i64, i64)>, lift: i64, stack: Option<i64>) {
        // Sprites are drawn facing right; the pillar is on the right when the scaffold is on its left.
        let facing_right = (self.out < 0) == toward_pillar;
        let platform = GROUND - self.laid * COURSE - lift;
        let rows = if facing_right { rows } else { mirror(&rows) };
        let stack = stack.unwrap_or_else(|| (self.count - self.laid).min(3));
        self.states.push(State { x, feet: platform - 1, rows, laid: self.laid, held, platform, stack });
    }

    /// Walk from `x` to `target` one step at a time. `carry` holds a brick overhead.
    fn walk(&mut self, mut x: i64, target: i64, set: Set, toward: bool, carry: bool, stack: Option<i64>) -> i64 {
        let mut step = 0i64;
        while x != target {
            x += (target - x).min(WALK).max(-WALK);
            step += 1;
            let gait = if py_floordiv(step, 2) % 2 == 0 { Gait::A } else { Gait::B };
            let held = if carry { Some(over(x, self.laid)) } else { None };
            self.push(x, set.rows(gait), toward, held, 0, stack);
        }
        x
    }
}

/// Position of the brick held overhead for a builder at `x` with `laid` bricks on the tower.
fn over(x: i64, laid: i64) -> (i64, i64) {
    (x + 4 - BRICK_W / 2, GROUND - laid * COURSE - 1 - 15 - 7)
}

/// (pillar left edge, platform left, platform right, stack x, pick x, lay x).
fn layout(cx: i64, out: i64) -> (i64, i64, i64, i64, i64, i64) {
    let x0 = cx - BRICK_W / 2;
    if out > 0 {
        let left = x0 + BRICK_W + 1;
        let right = left + PLATFORM_W - 1;
        let stack_x = right - 7;
        (x0, left, right, stack_x, stack_x - 9, left)
    } else {
        let right = x0 - 2;
        let left = right - PLATFORM_W + 1;
        let stack_x = left + 1;
        (x0, left, right, stack_x, stack_x + 8, right - 7)
    }
}

fn simulate(cx: i64, out: i64, count: i64) -> Vec<State> {
    let (x0, _, _, _, pick, lay) = layout(cx, out);
    let mut plan = Plan { out, count, laid: 0, states: Vec::new() };
    for _ in 0..4 {
        plan.push(pick, front(), true, None, 0, None);
    }
    for _ in 0..count {
        let left_after = (count - plan.laid - 1).min(3);
        // 1. Crouch at the stack and take a brick.
        for _ in 0..3 {
            plan.push(pick, crouch(), false, None, 0, None);
        }
        // 2. Stand up with the brick overhead.
        for _ in 0..3 {
            let held = over(pick, plan.laid);
            plan.push(pick, up(Gait::Stand), true, Some(held), 0, Some(left_after));
        }
        // 3. Carry it to the pillar.
        let x = plan.walk(pick, lay, Set::Up, true, true, Some(left_after));
        // 4. Lower it onto the top of the pillar.
        let slot_y = GROUND - (plan.laid + 1) * COURSE;
        plan.push(x, up(Gait::Stand), true, Some((x0, slot_y - 8)), 0, Some(left_after));
        for rise in [5, 3, 1] {
            plan.push(x, crouch(), true, Some((x0, slot_y - rise)), 0, Some(left_after));
        }
        plan.laid += 1;
        plan.push(x, crouch(), true, None, -COURSE, Some(left_after));
        plan.push(x, stand(), true, None, -COURSE, Some(left_after));
        // 5. The platform hoists him up one brick, then he walks back to the stack.
        for lift in [-6, -4, -2] {
            plan.push(x, front(), true, None, lift, Some(left_after));
        }
        let x = plan.walk(x, pick, Set::Empty, false, false, Some(left_after));
        plan.push(x, stand(), false, None, 0, Some(left_after));
    }
    plan.states
}

/// (values, median) of the followed accounts: `median_of` in Python.
fn median_of(following: Option<&[Followed]>) -> (Option<i64>, usize) {
    let totals: Vec<i64> = following.unwrap_or(&[]).iter().map(|entry| entry.total).collect();
    let median = py_median(&totals).map(py_round);
    (median, totals.len())
}

struct Sim {
    states: Vec<State>,
    n: i64,
    value: i64,
}

pub fn render(data: &Data) -> String {
    let you = data.total_contributions;
    let (median, count) = median_of(data.following.as_deref());
    let peak = {
        let peak = you.max(median.unwrap_or(0));
        if peak == 0 { 1 } else { peak }
    };
    let bricks_for = |value: i64| -> i64 {
        if value != 0 {
            1.max(py_round((MAX_BRICKS * value) as f64 / peak as f64))
        } else {
            0
        }
    };

    // Scaffolds stand on the outer sides, leaving the middle free for the comparison line.
    let values = [Some(you), median];
    let outs = [-1i64, 1];
    let sims: Vec<Option<Sim>> = (0..2)
        .map(|index| {
            values[index].map(|value| {
                let n = bricks_for(value);
                Sim { states: simulate(CENTERS[index], outs[index], n), n, value }
            })
        })
        .collect();
    let build = sims.iter().flatten().map(|sim| sim.states.len() as i64).max().unwrap_or(0);
    let hold = 56i64.max((build as f64 * HOLD_SHARE / (1.0 - HOLD_SHARE) / 8.0).ceil() as i64 * 8);
    let total = build + hold;

    let base = background();
    let sky_stars = stars();
    let footnote = if count > 0 {
        format!("MEDIAN OF {} ACCOUNT{} I FOLLOW", count, if count != 1 { "S" } else { "" })
    } else {
        "I FOLLOW NO ACCOUNTS YET".to_string()
    };
    let winner: Option<usize> = match median {
        Some(m) if you != m => Some(if you > m { 0 } else { 1 }),
        _ => None,
    };

    let mut frames: Vec<Canvas> = Vec::new();
    let mut still = total - 1;
    for f in 0..total {
        let t = f as f64 / total as f64;
        let mut canvas = base.clone();
        for &(x, y, cycles, phase) in &sky_stars {
            // Whole number of twinkles per loop, so the loop has no seam.
            let on = py_mod(t * (cycles + 3) as f64 + phase, 1.0);
            canvas.put(x, y, if on < 0.1 { STAR[2] } else if on < 0.45 { STAR[1] } else { STAR[0] });
        }
        let line_age = f - build;

        for index in 0..2 {
            let cx = CENTERS[index];
            let (side, out) = if index == 0 { (YOU, -1) } else { (THEM, 1) };
            let label = if index == 0 { "YOU" } else { "MEDIAN" };
            let shown: Option<i64> = match &sims[index] {
                None => {
                    empty_lot_sign(&mut canvas, cx);
                    None
                }
                Some(sim) => {
                    let last = sim.states.len() as i64 - 1;
                    let state = &sim.states[f.min(last) as usize];
                    let finished = f >= last;
                    let laid = state.laid;
                    tower(&mut canvas, cx, laid, &side);
                    scaffold(&mut canvas, cx, out, state.platform, state.stack, &side);
                    let x = state.x;
                    let mut feet = state.feet;
                    let mut rows = state.rows.clone();
                    if finished {
                        rows = if out < 0 { stand() } else { mirror(&stand()) };
                        if line_age >= 6 && winner == Some(index) {
                            // The taller builder celebrates with little jumps.
                            let beat = py_imod(line_age - 6, 8);
                            let jump = [0, 2, 4, 5, 5, 4, 2, 0][beat as usize];
                            rows = if (1..=6).contains(&beat) { front_up() } else { front() };
                            feet -= jump;
                            if (3..=5).contains(&beat) {
                                let spread = beat - 1;
                                for (dx, dy) in [(-3 - spread, -15), (10 + spread, -16), (-2 - spread, -7), (9 + spread, -8)] {
                                    canvas.put(x + dx, feet + dy, if beat != 5 { SPARKLE } else { side.label });
                                }
                            }
                        } else if line_age >= 6 && winner.is_none() {
                            rows = if py_floordiv(line_age - 6, 8) % 2 != 0 { front_up() } else { front() };
                        }
                    }
                    sprite(&mut canvas, &rows, x, feet, &side);
                    if let Some((hx, hy)) = state.held {
                        brick(&mut canvas, hx, hy, BRICK_W, &side.brick, laid, true);
                    }
                    Some(if sim.n != 0 { py_round((sim.value * laid) as f64 / sim.n as f64) } else { 0 })
                }
            };
            let number = match shown {
                None => "-".to_string(),
                Some(value) => value.to_string(),
            };
            canvas.text_center(cx as f64, (GROUND + 7) as f64, &number, TEXT, Font::F57, 3, Some(SOIL[0]));
            canvas.text_center(cx as f64, (GROUND + 31) as f64, label, side.label, Font::F57, 2, Some(SOIL[0]));
        }

        // Once both towers stand, a dashed line carries the median height across to yours.
        if line_age >= 0 {
            if let Some(them) = &sims[1] {
                let level_y = GROUND - them.n * COURSE - 1;
                let start = CENTERS[1] - BRICK_W / 2 - 1;
                let end = CENTERS[0] - BRICK_W / 2;
                let reach = start as f64 - (1.0f64.min(line_age as f64 / 10.0)) * ((start - end) as f64);
                for x in (reach as i64)..=start {
                    let inside = end <= x && x < end + BRICK_W;
                    if py_floordiv(x, 3) % 2 == 0 {
                        canvas.put(x, level_y, if !inside { THEM.label } else { DASH });
                        canvas.put(x, level_y + 1, OUTLINE);
                    }
                }
                if line_age >= 10 {
                    for k in [-1, 0, 1] {
                        canvas.put(end - 1, level_y + k, DASH);
                    }
                }
            }
        }

        canvas.text_center(W as f64 / 2.0, 6.0, "LAST 12 MONTHS", [150, 150, 190], Font::F35, 2, None);
        canvas.text_center(W as f64 / 2.0, (H - 12) as f64, &footnote, [150, 140, 150], Font::F35, 2, None);
        frames.push(canvas);
        if line_age >= 6 && py_imod(line_age - 6, 8) == 0 {
            still = f; // Latest hold frame with the winner on his feet.
        }
    }

    save(&frames, "median", Save { still: Some(still as usize), duration: FRAME_MS, ..Save::default() })
}
