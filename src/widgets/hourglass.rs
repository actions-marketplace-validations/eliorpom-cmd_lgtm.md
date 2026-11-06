//! Port of the Python prototype. Coloured sand hourglass: one grain per commit of the past year
//! (or per N commits when the year is large), poured in chronological order.
//!
//! The simulation is order-sensitive. Python iterates sets of cells in CPython's hash table
//! order, and `pour_upper` draws random numbers inside a `sorted(key=...)` over such a set,
//! so the draws only match if the iteration order matches. `PySet` reproduces that table.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use indexmap::IndexMap;

use crate::data::{Data, LanguageCount, Month};
use crate::export::{save, Save};
use crate::pixel::{mix, panel, rgb, text_width, Canvas, Font, Rgb, BG, BORDER, MUTED, TEXT};
use crate::pymath::{py_ffloordiv, py_imod, py_round};
use crate::pyrand::Random;

type Cell = (i64, i64);

const W: i64 = 400;
const H: i64 = 176;
const DURATION_MS: u32 = 60;
const GRAIN_EVERY: f64 = 1.35; // Frames between two grains leaving the neck.
const GRAVITY: f64 = 0.42; // Art pixels per frame squared.

const CX: i64 = 94; // Hourglass axis.
const TOP: i64 = 15;
const NECK: i64 = 88;
const BOTTOM: i64 = 161;
const CELL: i64 = 3; // A grain is 3 x 3 art pixels.

const WOOD: [Rgb; 6] = [[40, 22, 16], [74, 42, 26], [110, 66, 38], [148, 94, 54], [186, 128, 76], [220, 166, 106]];
const GLASS_EDGE: Rgb = [120, 146, 176];
const GLASS_LIT: Rgb = [196, 220, 244];
const GLASS_TINT: Rgb = [22, 28, 40];
const NUMBER: Rgb = [252, 222, 160];

// Hand-tuned bulb profile: (distance from the neck 0..1, half width 0..1).
const BULB: [(f64, f64); 12] = [
    (0.0, 0.0), (0.05, 0.06), (0.14, 0.22), (0.25, 0.5), (0.35, 0.72), (0.45, 0.88), (0.55, 0.97),
    (0.65, 1.0), (0.75, 0.98), (0.85, 0.93), (0.93, 0.86), (1.0, 0.8),
];
const BULB_HALF: f64 = 30.0;
// Turned cap profile, row by row: (half width, light level). Top cap; the bottom one is mirrored.
const CAP: [(i64, f64); 8] = [(34, 4.6), (38, 3.8), (38, 3.0), (37, 2.2), (39, 3.6), (39, 2.6), (38, 1.6), (33, 1.0)];
const POST_BEADS: [(f64, f64); 9] = [
    (0.0, 3.0), (0.04, 1.8), (0.08, 2.6), (0.13, 1.6), (0.5, 2.2), (0.87, 1.6), (0.92, 2.6), (0.96, 1.8), (1.0, 3.0),
];
// Hand-drawn grains: l = lit edge, b = body, d = shaded edge.
const GRAINS: [[&str; 3]; 4] = [
    ["lbb", "bbd", "bdd"], ["bbb", "lbd", "bdd"], ["blb", "bbb", "dbd"], ["lbb", "bbb", "bdd"],
];
const NICE_STEPS: [i64; 24] = [
    1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 20, 25, 30, 40, 50, 75, 100, 150, 200, 250, 500, 1000, 2000, 5000,
];
const MAX_GRAINS: i64 = 220;
// 5 x 7 "=" missing from the shared font.
const EQUALS: &str = concat!("00000", "00000", "11111", "00000", "11111", "00000", "00000");

// CPython set and tuple hashing, reproduced so that set iteration order matches.
const XXPRIME_1: u64 = 11400714785074694791;
const XXPRIME_2: u64 = 14029467366897019727;
const XXPRIME_5: u64 = 2870177450012600261;
const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;

/// `hash(int)` for small integers: itself, except -1 which hashes to -2.
fn int_hash(value: i64) -> u64 {
    let hash = if value == -1 { -2 } else { value };
    hash as u64
}

/// `hash((a, b))` in CPython 3.8+ (xxHash-style combination of the item hashes).
fn tuple_hash(cell: Cell) -> u64 {
    let mut acc = XXPRIME_5;
    for lane in [int_hash(cell.0), int_hash(cell.1)] {
        acc = acc.wrapping_add(lane.wrapping_mul(XXPRIME_2));
        acc = acc.rotate_left(31);
        acc = acc.wrapping_mul(XXPRIME_1);
    }
    acc = acc.wrapping_add(2u64 ^ (XXPRIME_5 ^ 3527539));
    if acc == u64::MAX { 1546275796 } else { acc }
}

/// First free slot for `hash`, probed like CPython's `set_add_entry` / `set_insert_clean`.
fn free_slot(table: &[Option<(Cell, u64)>], hash: u64) -> usize {
    let mask = table.len() - 1;
    let mut perturb = hash;
    let mut i = (hash as usize) & mask;
    loop {
        let probes = if i + LINEAR_PROBES <= mask { LINEAR_PROBES } else { 0 };
        for j in 0..=probes {
            if table[i + j].is_none() {
                return i + j;
            }
        }
        perturb >>= PERTURB_SHIFT;
        i = ((i as u64).wrapping_mul(5).wrapping_add(1).wrapping_add(perturb) as usize) & mask;
    }
}

/// A Python `set` of cells: membership is exact, and `iter` yields the cells in the order
/// CPython's hash table would (table slot order), which is what `for c in set` follows.
pub struct PySet {
    table: Vec<Option<(Cell, u64)>>,
    members: HashSet<Cell>,
}

impl PySet {
    fn new() -> Self {
        PySet { table: vec![None; 8], members: HashSet::new() }
    }

    fn add(&mut self, cell: Cell) {
        if !self.members.insert(cell) {
            return;
        }
        let hash = tuple_hash(cell);
        let slot = free_slot(&self.table, hash);
        self.table[slot] = Some((cell, hash));
        // CPython grows the table when fill * 5 >= mask * 3.
        let used = self.members.len();
        let mask = self.table.len() - 1;
        if used * 5 >= mask * 3 {
            self.resize(if used > 50000 { used * 2 } else { used * 4 });
        }
    }

    fn resize(&mut self, minused: usize) {
        let mut newsize = 8;
        while newsize <= minused {
            newsize <<= 1;
        }
        let old = std::mem::replace(&mut self.table, vec![None; newsize]);
        for (cell, hash) in old.into_iter().flatten() {
            let slot = free_slot(&self.table, hash);
            self.table[slot] = Some((cell, hash));
        }
    }

    fn contains(&self, cell: &Cell) -> bool {
        self.members.contains(cell)
    }

    fn len(&self) -> usize {
        self.members.len()
    }

    fn iter_order(&self) -> Vec<Cell> {
        self.table.iter().flatten().map(|(cell, _)| *cell).collect()
    }
}

/// `EQUALS` glyph is drawn here so the shared font module stays untouched.
fn text_eq(canvas: &mut Canvas, x: i64, y: i64, text: &str, color: Rgb) {
    let scale = 2;
    for (index, ch) in text.chars().enumerate() {
        let index = index as i64;
        if ch == '=' {
            for (bit, value) in EQUALS.chars().enumerate() {
                let bit = bit as i64;
                if value == '1' {
                    let gx = x + index * 6 * scale + (bit % 5) * scale;
                    let gy = y + (bit / 5) * scale;
                    canvas.rect(gx, gy, gx + scale - 1, gy + scale - 1, color);
                }
            }
        } else {
            canvas.text((x + index * 6 * scale) as f64, y as f64, &ch.to_string(), color, Font::F57, scale, None);
        }
    }
}

/// Flat colour bands with a thin 50% checker at each band edge (zone 0.2).
fn band(colors: &[Rgb], value: f64, x: i64, y: i64) -> Rgb {
    let value = (colors.len() as f64 - 1.0).min(0.0f64.max(value));
    let low = value as usize;
    if low + 1 < colors.len() && value - low as f64 > 1.0 - 0.2 && py_imod(x + y, 2) == 0 {
        return colors[low + 1];
    }
    colors[low]
}

/// Glass half width at row y.
fn half_width(y: i64) -> f64 {
    let span = (NECK - TOP) as f64;
    let d = ((y as f64 + 0.5 - NECK as f64).abs() / span).min(1.0);
    for pair in BULB.windows(2) {
        let ((d0, g0), (d1, g1)) = (pair[0], pair[1]);
        if d0 <= d && d <= d1 {
            return 2.6 + (BULB_HALF - 2.6) * (g0 + (g1 - g0) * (d - d0) / (d1 - d0));
        }
    }
    2.6 + (BULB_HALF - 2.6) * BULB[BULB.len() - 1].1
}

fn post_radius(u: f64) -> f64 {
    for pair in POST_BEADS.windows(2) {
        let ((u0, r0), (u1, r1)) = (pair[0], pair[1]);
        if u0 <= u && u <= u1 {
            let k = (u - u0) / (u1 - u0);
            return r0 + (r1 - r0) * (0.5 - 0.5 * (PI * k).cos());
        }
    }
    POST_BEADS[POST_BEADS.len() - 1].1
}

fn draw_post(canvas: &mut Canvas, x: i64, y0: i64, y1: i64, darker: f64) {
    for y in y0..=y1 {
        let r = post_radius((y - y0) as f64 / (y1 - y0) as f64);
        for xx in (x as f64 - r).floor() as i64..=(x as f64 + r).ceil() as i64 {
            let s = (xx as f64 + 0.5 - x as f64) / r;
            if s.abs() > 1.0 {
                continue;
            }
            let mut level = 3.4 - 2.6 * (s + 0.35).abs() - darker;
            if s.abs() > 0.85 {
                level = 0.5 - darker * 0.3;
            }
            canvas.put(xx, y, band(&WOOD, level, xx, y));
        }
    }
}

fn draw_cap(canvas: &mut Canvas, y0: i64, flip: bool) {
    let rows: Vec<(i64, f64)> = if flip { CAP.iter().rev().copied().collect() } else { CAP.to_vec() };
    for (row, (half, level)) in rows.into_iter().enumerate() {
        let row = row as i64;
        let y = y0 + row;
        for x in CX - half..=CX + half {
            let edge = (x - CX).abs() as f64 / half as f64;
            let side = if (x as f64) < CX as f64 - half as f64 * 0.3 { 0.6 } else { 0.0 };
            let mut lit = level + side - edge.powf(4.0) * 1.6;
            if py_imod(x * 3 + row * 11, 17) < 2 && edge < 0.9 {
                lit -= 0.8; // Wood grain.
            }
            if (x - CX).abs() == half {
                lit = 0.3;
            }
            canvas.put(x, y, band(&WOOD, lit, x, y));
        }
    }
}

/// Shades of a grain, indexed by the grain keys: 0 = lit (`l`), 1 = body (`b`), 2 = shaded (`d`).
fn grain_shades(color: Rgb) -> [Rgb; 3] {
    [mix(color, [255, 255, 255], 0.35), color, mix(color, [0, 0, 0], 0.38)]
}

fn draw_grain(canvas: &mut Canvas, x: i64, y: i64, shades: &[Rgb; 3], variant: usize) {
    for (row, line) in GRAINS[variant].iter().enumerate() {
        for (col, key) in line.bytes().enumerate() {
            let shade = match key {
                b'l' => 0,
                b'b' => 1,
                _ => 2,
            };
            canvas.put(x + col as i64, y + row as i64, shades[shade]);
        }
    }
}

// Cell grids: column 0 is the centre column, row 0 sits against the neck (upper bulb)
// or on the floor (lower bulb).

fn cell_box(col: i64, row: i64, upper: bool) -> Cell {
    let x = CX - 1 + col * CELL;
    let y = if upper { NECK - 2 - (row + 1) * CELL } else { BOTTOM - (row + 1) * CELL };
    (x, y)
}

fn cell_inside(col: i64, row: i64, upper: bool) -> bool {
    let (x, y) = cell_box(col, row, upper);
    if upper && y < TOP + 1 {
        return false;
    }
    if !upper && y < NECK + 3 {
        return false;
    }
    for yy in y..y + CELL {
        let w = half_width(yy) - 1.0;
        if (x as f64 + 0.5 - CX as f64) < -w || (x as f64 + CELL as f64 - 0.5 - CX as f64) > w {
            return false;
        }
    }
    true
}

/// Cells of one bulb, inserted in the same order as the Python loops (row, then column).
fn bulb_cells(upper: bool) -> PySet {
    let mut cells = PySet::new();
    for row in 0..40 {
        for col in -15..=15 {
            if cell_inside(col, row, upper) {
                cells.add((col, row));
            }
        }
    }
    cells
}

fn nice_step(total: i64, capacity: i64) -> i64 {
    let limit = MAX_GRAINS.min(capacity);
    for step in NICE_STEPS {
        if total as f64 / step as f64 <= limit as f64 {
            return step;
        }
    }
    NICE_STEPS[NICE_STEPS.len() - 1]
}

/// Grains per month, split by language with largest remainders, oldest month first.
fn month_grains(months: &[Month], step: i64, rank: &HashMap<String, usize>) -> Vec<(String, Rgb)> {
    let mut sequence: Vec<(String, Rgb)> = Vec::new();
    let mut carry = 0.0f64;
    for month in months {
        let langs: Vec<&LanguageCount> = month.languages.iter().filter(|lang| lang.count > 0).collect();
        let sum_counts: i64 = langs.iter().map(|lang| lang.count).sum();
        let total = sum_counts as f64 + carry;
        let grains = py_ffloordiv(total, step as f64) as i64;
        carry = total - (grains * step) as f64; // Leftover commits roll into the next month.
        if grains <= 0 || langs.is_empty() {
            continue;
        }
        let raw: Vec<(&str, f64)> = langs
            .iter()
            .map(|lang| (lang.name.as_str(), lang.count as f64 / sum_counts as f64 * grains as f64))
            .collect();
        // Dict comprehension: a repeated name keeps its first position and its last value.
        let mut counts: IndexMap<String, i64> = IndexMap::new();
        for (name, share) in &raw {
            counts.insert(name.to_string(), share.trunc() as i64);
        }
        let left = grains - counts.values().sum::<i64>();
        // Stable descending sort on the fractional parts, like sorted(..., reverse=True).
        let mut by_fraction: Vec<usize> = (0..raw.len()).collect();
        by_fraction.sort_by(|&a, &b| {
            let fa = raw[a].1 - raw[a].1.trunc();
            let fb = raw[b].1 - raw[b].1.trunc();
            fb.partial_cmp(&fa).unwrap()
        });
        // Python slice [:left], including its behaviour for a negative bound.
        let take = if left >= 0 { (left as usize).min(raw.len()) } else { raw.len().saturating_sub((-left) as usize) };
        for &index in by_fraction.iter().take(take) {
            *counts.get_mut(raw[index].0).unwrap() += 1;
        }
        // Within a month, each language is poured as its own band, always in the same order.
        let mut ordered = langs.clone();
        ordered.sort_by_key(|lang| rank.get(&lang.name).copied().unwrap_or(99));
        for lang in ordered {
            let n = counts[&lang.name];
            let color = rgb(lang.color.as_deref().expect("language without a colour"));
            for _ in 0..n {
                sequence.push((lang.name.clone(), color));
            }
        }
    }
    sequence
}

/// Let one grain go down from (col, row): straight down while there is room,
/// else down a diagonal (random side first). Returns the cells it passes through.
fn roll(cells: &PySet, filled: &dyn Fn(&Cell) -> bool, col: i64, row: i64, rng: &mut Random, sticky: f64) -> Vec<Cell> {
    let (mut col, mut row) = (col, row);
    let mut path = vec![(col, row)];
    while row > 0 {
        let below = (col, row - 1);
        if cells.contains(&below) && !filled(&below) {
            (col, row) = below;
            path.push((col, row));
            continue;
        }
        if path.len() > 1 && rng.random() < sticky {
            break; // A little friction: some grains stop early, the slopes stay irregular.
        }
        let sides: [i64; 2] = if rng.random() < 0.5 { [-1, 1] } else { [1, -1] };
        let mut moved = false;
        for side in sides {
            let target = (col + side, row - 1);
            if cells.contains(&target) && !filled(&target) {
                (col, row) = target;
                path.push((col, row));
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
    path
}

/// Fill the upper bulb as sand at rest: the lowest cells first, with a little jitter so
/// the surface stays slightly uneven, never a flat ruler line.
fn pour_upper(cells: &PySet, count: usize, rng: &mut Random) -> IndexMap<Cell, usize> {
    let mut bumps: HashMap<i64, f64> = HashMap::new();
    for col in -20..=20 {
        bumps.insert(col, rng.uniform(-1.2, 1.2));
    }
    // Keys are computed in the set's iteration order, which is what draws the random values.
    let mut keyed: Vec<(f64, Cell)> = Vec::new();
    for cell in cells.iter_order() {
        let jitter = rng.random() * 0.8;
        let key = cell.1 as f64 + bumps[&cell.0] + 0.6 * (cell.0 as f64 * 0.7).sin() + jitter;
        keyed.push((key, cell));
    }
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut chosen = PySet::new();
    for (_, cell) in keyed.iter().take(count) {
        chosen.add(*cell);
    }
    // Settle the chosen cells so no grain floats above a hole.
    let mut filled: IndexMap<Cell, usize> = IndexMap::new();
    let mut by_row = chosen.iter_order();
    by_row.sort_by_key(|cell| cell.1);
    for (index, cell) in by_row.into_iter().enumerate() {
        let path = roll(cells, &|c: &Cell| filled.contains_key(c), cell.0, cell.1, rng, 0.0);
        filled.insert(*path.last().unwrap(), index);
    }
    filled
}

/// Empty the upper bulb through the neck, grain by grain, with simple grain physics.
///
/// Each step the grain on the neck leaves, then every grain gets one chance to move down
/// or down a diagonal; a funnel-shaped crater opens above the neck. Returns the ids in the
/// order they leave and the bulb's content after each step.
fn drain_upper(
    cells: &PySet,
    poured: IndexMap<Cell, usize>,
    count: usize,
    rng: &mut Random,
) -> (Vec<usize>, Vec<IndexMap<Cell, usize>>) {
    let mut grid = poured;
    let neck: Cell = (0, 0);
    let mut states = vec![grid.clone()];
    let mut order = Vec::new();

    // Which grain is "deeper" in the sand: low rows and the middle first, with soft,
    // irregular boundaries. The oldest grain always leaves next.
    let phases: Vec<f64> = (0..4).map(|_| rng.uniform(0.0, 6.3)).collect();
    let depth = |cell: Cell, rng: &mut Random| -> f64 {
        let (col, row) = (cell.0 as f64, cell.1 as f64);
        let mut wobble = 1.6 * (col * 0.55 + phases[0]).sin() + 1.1 * (col * 1.3 + row * 0.4 + phases[1]).sin();
        wobble += 0.9 * (row * 0.8 + col * 0.3 + phases[2]).sin() + rng.uniform(-0.8, 0.8);
        row + 0.45 * (cell.0.abs() as f64) + wobble
    };
    let mut deep: HashMap<usize, f64> = HashMap::new();
    for (&cell, &grain) in grid.iter() {
        let value = depth(cell, rng);
        deep.insert(grain, value);
    }

    for _ in 0..count {
        let mut tries = 0;
        while !grid.contains_key(&neck) && tries < 12 {
            if !sweep(cells, &mut grid, rng) {
                break;
            }
            tries += 1;
        }
        let leaving = if grid.contains_key(&neck) {
            neck
        } else {
            // A grain stuck on a flat wall: take the one closest to the hole.
            first_min(grid.keys().copied(), |c| (c.1 + c.0.abs(), c.0.abs()))
        };
        // The grain falling through the neck takes the colour of the oldest grain left,
        // which sits close by, so the sand above keeps its slow, layered order.
        let oldest = first_min(grid.keys().copied(), |c| deep[&grid[&c]]);
        if oldest != leaving {
            let a = grid[&leaving];
            let b = grid[&oldest];
            grid.insert(leaving, b);
            grid.insert(oldest, a);
        }
        order.push(grid.shift_remove(&leaving).unwrap());
        for _ in 0..30 {
            if !sweep(cells, &mut grid, rng) {
                break;
            }
        }
        states.push(grid.clone());
    }
    (order, states)
}

/// `min(iterable, key=...)`: the first minimal item wins, like Python.
fn first_min<K: PartialOrd, I: Iterator<Item = Cell>>(items: I, key: impl Fn(Cell) -> K) -> Cell {
    let mut best: Option<(K, Cell)> = None;
    for cell in items {
        let k = key(cell);
        match &best {
            Some((best_key, _)) if !(k < *best_key) => {}
            _ => best = Some((k, cell)),
        }
    }
    best.expect("min() of an empty sequence").1
}

/// One pass of grain moves. Grains are visited by row (then by a random key), and each
/// tries the cell below, then the two diagonals (the one toward the centre first).
fn sweep(cells: &PySet, grid: &mut IndexMap<Cell, usize>, rng: &mut Random) -> bool {
    // sorted(grid, key=lambda c: (c[1], rng.random())): one draw per grain, in dict order.
    let mut keyed: Vec<(i64, f64, Cell)> = Vec::with_capacity(grid.len());
    for cell in grid.keys().copied().collect::<Vec<Cell>>() {
        keyed.push((cell.1, rng.random(), cell));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap()));
    let mut moved = false;
    for (_, _, cell) in keyed {
        let (col, row) = cell;
        let mut options = vec![(col, row - 1)];
        let mut sides: [i64; 2] = if rng.random() < 0.5 { [-1, 1] } else { [1, -1] };
        // Grains drift toward the hole: the diagonal toward the centre is tried first.
        if col != 0 {
            sides.sort_by_key(|side| (col + side).abs());
        }
        options.extend(sides.iter().map(|side| (col + side, row - 1)));
        // Resting on the glass rather than on sand: slide along the steep wall toward the hole.
        if col != 0 && !cells.contains(&(col, row - 1)) {
            let inward = -col.signum();
            options.push((col + 2 * inward, row - 1));
            options.push((col + inward, row));
        }
        for target in options {
            if cells.contains(&target) && !grid.contains_key(&target) && target.1 >= 0 {
                let value = grid.shift_remove(&cell).unwrap();
                grid.insert(target, value);
                moved = true;
                break;
            }
        }
    }
    moved
}

/// Pour grains one by one onto the centre of the lower bulb: a cone that grows,
/// grains rolling down its slopes until they rest (no friction: a spike is not sand). Returns each grain's path, landing cell first.
fn settle(cells: &PySet, count: usize, rng: &mut Random) -> Vec<Vec<Cell>> {
    let mut filled: HashSet<Cell> = HashSet::new();
    let mut paths = Vec::with_capacity(count);
    let top_row = cells.members.iter().map(|cell| cell.1).max().unwrap();
    for _ in 0..count {
        // The stream wobbles a little, so the cone grows unevenly instead of as a ruled pyramid.
        let col = [-1, 0, 0, 1][rng.randrange_to(4) as usize];
        let land = filled.iter().filter(|cell| cell.0 == col).map(|cell| cell.1).max().unwrap_or(-1) + 1;
        let land = if cells.contains(&(col, land)) { land } else { top_row };
        let path = roll(cells, &|c: &Cell| filled.contains(c), col, land, rng, 0.0);
        filled.insert(*path.last().unwrap());
        paths.push(path);
    }
    paths
}

struct Event {
    emit: i64,
    fall: i64,
}

pub fn render(data: &Data) -> String {
    let months: &[Month] = data.commits_by_month.as_deref().unwrap_or(&[]);
    let total: i64 = months.iter().flat_map(|month| month.languages.iter()).map(|lang| lang.count).sum();
    let upper_cells = bulb_cells(true);
    let lower_cells = bulb_cells(false);
    let capacity = upper_cells.len().min(lower_cells.len()) as i64;
    let step = if total != 0 { nice_step(total, (capacity as f64 * 0.9) as i64) } else { 1 };

    // Yearly totals per language, in first-seen order, then ranked: biggest first, "Other" last.
    let mut yearly: IndexMap<String, i64> = IndexMap::new();
    for month in months {
        for lang in &month.languages {
            *yearly.entry(lang.name.clone()).or_insert(0) += lang.count;
        }
    }
    let mut names: Vec<&String> = yearly.keys().collect();
    names.sort_by_key(|name| (name.as_str() == "Other", -yearly[*name]));
    let rank: HashMap<String, usize> = names.iter().enumerate().map(|(i, name)| ((*name).clone(), i)).collect();

    let mut sequence = month_grains(months, step, &rank);
    // Languages whose GitHub colours look alike (Python and TypeScript, Swift and HTML) get
    // colours that can be told apart, the most used language keeping its own.
    let mut by_rank: Vec<(String, Rgb)> = Vec::new();
    for name in &names {
        if let Some((_, color)) = sequence.iter().find(|(n, _)| n == *name) {
            by_rank.push(((*name).clone(), *color));
        }
    }
    let colors: HashMap<String, Rgb> = crate::pixel::distinct_colors(&by_rank).into_iter().collect();
    for (name, color) in sequence.iter_mut() {
        *color = colors[name];
    }
    let count = sequence.len();
    let mut rng = Random::new(9);
    let variants: Vec<usize> = (0..count).map(|_| rng.randrange_to(GRAINS.len() as i64) as usize).collect();

    // Upper bulb: which slot leaves at which step, then colour slots in chronological order.
    let poured = pour_upper(&upper_cells, count, &mut rng);
    let (order, top_states) = drain_upper(&upper_cells, poured, count, &mut rng);
    let slot_rank: HashMap<usize, usize> = order.iter().enumerate().map(|(rank, &slot)| (slot, rank)).collect();
    let paths = settle(&lower_cells, count, &mut rng);

    // Timeline: emission, fall, roll, then rest.
    let mut events: Vec<Event> = Vec::with_capacity(count);
    for (index, path) in paths.iter().enumerate() {
        let emit = 6 + py_round(index as f64 * GRAIN_EVERY);
        let (_, land_y) = cell_box(path[0].0, path[0].1, false);
        let distance = 1i64.max(land_y - (NECK + 2));
        let fall = 2i64.max(((2 * distance) as f64 / GRAVITY).sqrt().ceil() as i64);
        events.push(Event { emit, fall });
    }
    let pour_end = events
        .iter()
        .zip(paths.iter())
        .map(|(event, path)| event.emit + event.fall + path.len() as i64)
        .max()
        .unwrap_or(0);
    let hold = 48i64.max((pour_end as f64 * 0.43).ceil() as i64);
    let mut frames_total = 96i64.max(pour_end + hold);
    if frames_total % 2 != 0 {
        frames_total += 1;
    }

    let mut shades: HashMap<String, [Rgb; 3]> = HashMap::new();
    for (name, color) in &sequence {
        shades.entry(name.clone()).or_insert_with(|| grain_shades(*color));
    }
    let mut legend_counts: IndexMap<String, i64> = IndexMap::new();
    for month in months {
        for lang in &month.languages {
            if lang.count > 0 {
                *legend_counts.entry(lang.name.clone()).or_insert(0) += lang.count;
            }
        }
    }
    let in_sand: HashSet<&String> = sequence.iter().map(|(name, _)| name).collect();
    let mut legend: Vec<&String> = legend_counts.keys().filter(|name| in_sand.contains(name)).collect();
    legend.sort_by_key(|name| (name.as_str() == "Other", -legend_counts[*name]));

    let draw = |frame: i64| -> Canvas {
        let mut canvas = Canvas::new(W, H);
        panel(&mut canvas, BG, BORDER);
        let emitted = events.iter().filter(|event| event.emit <= frame).count();

        // Shadow, rear post, glass tint.
        for x in CX - 42..CX + 43 {
            if py_imod(x, 2) == 0 || (x - CX).abs() < 34 {
                canvas.put(x, BOTTOM + 11, [11, 14, 20]);
            }
        }
        for y in TOP..=BOTTOM {
            let w = half_width(y);
            for x in (CX as f64 - w).ceil() as i64..=(CX as f64 + w).floor() as i64 {
                if (x as f64 + 0.5 - CX as f64).abs() < w - 0.5 {
                    let rel = (x as f64 + 0.5 - CX as f64).abs() / w;
                    let ratio = if py_imod(x + y, 2) == 0 || rel > 0.8 { 0.6 } else { 0.25 };
                    canvas.put(x, y, mix(canvas.get(x, y), GLASS_TINT, ratio));
                }
            }
        }

        // Upper bulb: the grains still waiting, already in the order they will fall.
        for (cell, slot) in top_states[emitted.min(count)].iter() {
            let rank = slot_rank[slot];
            let (name, _) = &sequence[rank];
            let (x, y) = cell_box(cell.0, cell.1, true);
            draw_grain(&mut canvas, x, y, &shades[name], variants[rank]);
        }

        // Lower bulb: settled grains, rolling grains, falling grains.
        for index in 0..emitted {
            let event = &events[index];
            let (name, _) = &sequence[index];
            let age = frame - event.emit;
            let path = &paths[index];
            if age < event.fall {
                let y = (NECK + 2) as f64 + 0.5 * GRAVITY * age as f64 * age as f64;
                // Drifts from the neck toward its landing column as it falls.
                let (land_x, _) = cell_box(paths[index][0].0, paths[index][0].1, false);
                let drift = (age as f64 / event.fall as f64).powi(2);
                let x = py_round((CX - 1) as f64 + (land_x - (CX - 1)) as f64 * drift);
                draw_grain(&mut canvas, x, py_round(y) - 1, &shades[name], variants[index]);
            } else {
                let step_index = ((path.len() as i64 - 1).min(age - event.fall)) as usize;
                let (col, row) = path[step_index];
                let (x, y) = cell_box(col, row, false);
                draw_grain(&mut canvas, x, y, &shades[name], variants[index]);
            }
        }

        // Glass outline and reflections.
        for y in TOP..=BOTTOM {
            let w = half_width(y);
            let left = py_round(CX as f64 - w);
            let right = py_round(CX as f64 + w);
            canvas.put(left, y, GLASS_EDGE);
            canvas.put(right, y, mix(GLASS_EDGE, BG, 0.25));
            let d = (y - NECK).abs() as f64 / (NECK - TOP) as f64;
            if 0.35 < d && d < 0.8 {
                canvas.put(left + 3, y, mix(canvas.get(left + 3, y), GLASS_LIT, 0.6));
                if 0.45 < d && d < 0.62 {
                    canvas.put(left + 4, y, mix(canvas.get(left + 4, y), GLASS_LIT, 0.35));
                }
            }
            if 0.5 < d && d < 0.62 {
                canvas.put(right - 3, y, mix(canvas.get(right - 3, y), GLASS_LIT, 0.4));
            }
        }
        // A slow glint slides down the glass once the sand rests.
        let sweep = (frame - pour_end - 8) as f64 / (hold as f64 * 0.6).max(1.0);
        if (0.0..=1.0).contains(&sweep) {
            let gy = (TOP + 4) as f64 + sweep * (BOTTOM - TOP - 8) as f64;
            for k in 0..5 {
                let y = py_round(gy) + k;
                let x = py_round(CX as f64 - half_width(y)) + 2;
                canvas.put(x, y, mix(canvas.get(x, y), [255, 255, 255], 0.7 - k as f64 * 0.12));
            }
        }

        draw_cap(&mut canvas, TOP - 9, false);
        draw_cap(&mut canvas, BOTTOM + 1, true);
        for fx in [CX - 31, CX + 27] {
            canvas.rect(fx, BOTTOM + 9, fx + 4, BOTTOM + 10, WOOD[1]);
            canvas.put(fx + 1, BOTTOM + 9, WOOD[3]);
        }
        draw_post(&mut canvas, CX - 34, TOP - 1, BOTTOM + 1, 0.0);
        draw_post(&mut canvas, CX + 34, TOP - 1, BOTTOM + 1, 0.4);

        // Right column: total, legend, grain value.
        let x0: i64 = 150;
        let number = total.to_string();
        canvas.text(x0 as f64, 12.0, &number, NUMBER, Font::F57, 4, Some([70, 44, 20]));
        let nx = x0 + text_width(&number, Font::F57, 4) + 10;
        canvas.text(nx as f64, 12.0, if total == 1 { "COMMIT" } else { "COMMITS" }, TEXT, Font::F57, 2, None);
        canvas.text(nx as f64, 30.0, "LAST 12 MONTHS", MUTED, Font::F35, 2, None);
        if !legend.is_empty() {
            for (index, name) in legend.iter().take(8).enumerate() {
                let index = index as i64;
                let (col, row) = (index % 2, index / 2);
                let (lx, ly) = (x0 + col * 124, 58 + row * 18);
                let swatch = shades[*name];
                for gy in 0..3 {
                    for gx in 0..3 {
                        draw_grain(&mut canvas, lx + gx * 3, ly + gy * 3, &swatch, ((gx + gy * 2) % 4) as usize);
                    }
                }
                let label: String = name.to_uppercase().chars().take(11).collect();
                canvas.text((lx + 14) as f64, ly as f64, &label, TEXT, Font::F35, 2, None);
            }
            let unit = if step == 1 { "COMMIT" } else { "COMMITS" };
            let mut line = format!("1 GRAIN = {step} {unit}");
            if text_width(&line, Font::F57, 2) > W - x0 - 6 {
                line = line.replace(" = ", "="); // Big steps: tighten so the line fits.
            }
            text_eq(&mut canvas, x0, 144, &line, MUTED);
        } else {
            canvas.text(x0 as f64, 66.0, "NO COMMITS YET", MUTED, Font::F57, 2, None);
        }
        canvas
    };

    let frames: Vec<Canvas> = (0..frames_total).map(draw).collect();
    // Plays once: when the sand has run, the pile stays.
    save(&frames, "hourglass", Save { still: Some(frames.len() - 1), duration: DURATION_MS, once: true, ..Save::default() })
}
