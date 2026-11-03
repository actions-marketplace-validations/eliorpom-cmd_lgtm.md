//! Port of the Python prototype. Split-flap board: a station departure board that cycles
//! through 4 GitHub stats. Each stat holds a quarter of the loop. When it changes, every
//! flap turns over on its hinge, with a per-flap start offset so the board ripples.

use std::collections::HashMap;
use std::f64::consts::PI;

use crate::data::{Data, Language};
use crate::export::{save, Save};
use crate::pixel::{mix, Canvas, Font, Rgb, SCALE};
use crate::pymath::{py_floordiv, py_imod, py_round, py_sum};
use crate::pyrand::Random;

const W: i64 = 400;
const H: i64 = 96;
const PAGE: Rgb = [13, 17, 23];

// Board: matte black painted steel in an anodised frame.
const BOARD: Rgb = [17, 17, 19];
const BOARD_SPECK: Rgb = [23, 23, 26];
const FRAME_LIGHT: Rgb = [74, 74, 80];
const FRAME_MID: Rgb = [46, 46, 51];
const FRAME_DARK: Rgb = [28, 28, 31];
const RIVET: Rgb = [96, 96, 104];

// Flaps.
const FACE_TOP: Rgb = [44, 44, 49];
const FACE_TOP_HI: Rgb = [54, 54, 60];
const FACE_BOTTOM: Rgb = [36, 36, 41];
const FACE_EDGE: Rgb = [62, 62, 68];
const HINGE: Rgb = [8, 8, 9];
const PIN: Rgb = [120, 120, 128];
const SHADOW: Rgb = [9, 9, 11];
const AMBER: Rgb = [255, 176, 46];
const WHITE: Rgb = [236, 231, 216];
const CAPTION: Rgb = [176, 170, 156];
const LAMP_OFF: Rgb = [52, 46, 36];

// Geometry: FONT57 at scale 3 inside 18 x 40 flaps.
const GLYPH_SCALE: i64 = 3;
const TILE_W: i64 = 18;
const TILE_H: i64 = 40;
const PITCH: i64 = 20;
const HALF: i64 = 20; // Rows 0..19 are the top flap, 20..39 the bottom one.
const TILE_Y: i64 = 30;
const VALUE_TILES: i64 = 5;
const LABEL_TILES: i64 = 13;
const VALUE_X: i64 = 15;
const LABEL_X: i64 = VALUE_X + VALUE_TILES * PITCH + 10;

// Slow board: each stat is 45 frames at 90 ms (about 4 s).
const PER_STAT: usize = 45;
const LOOP_FRAMES: usize = 4 * PER_STAT;
const FRAME_MS: u32 = 90;
const SWITCH_AT: i64 = 4;
const FLIP_FRAMES: i64 = 3; // One character change: top flap falls, edge-on, bottom flap lands.
const DRUM: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789%";

const HIDDEN_LANGUAGES: [&str; 7] = ["HTML", "CSS", "SCSS", "MDX", "Jupyter Notebook", "Dockerfile", "Makefile"];

/// One flap face: 40 rows of 18 colours, `None` meaning "painted face".
type Rows = Vec<Vec<Option<Rgb>>>;
/// The characters of one stat row: 5 value cells then 13 label cells.
type Cells = Vec<char>;
type Cache = HashMap<(char, bool), Rows>;

#[derive(Clone, Copy, PartialEq)]
enum Half {
    Top,
    Bottom,
}

fn compact(number: i64) -> String {
    if number >= 100000 {
        return format!("{}K", py_floordiv(number, 1000));
    }
    number.to_string()
}

/// `top_languages(data, 1)`: first visible language with its share of the visible total.
fn top_language(data: &Data) -> Option<(String, f64)> {
    let shown: Vec<&Language> = data
        .languages
        .iter()
        .filter(|lang| !HIDDEN_LANGUAGES.contains(&lang.name.as_str()))
        .collect();
    let total = py_sum(shown.iter().map(|lang| lang.share));
    // Python: `sum(...) or 1`
    let total = if total == 0.0 { 1.0 } else { total };
    shown.first().map(|lang| (lang.name.clone(), lang.share / total))
}

fn stats(data: &Data) -> Vec<(String, Cells)> {
    let language = top_language(data);
    let (total, prs, streak) = (data.total_contributions, data.merged_prs_30d, data.streak);
    let mut rows: Vec<(String, String, String)> = vec![
        (
            "LAST 12 MONTHS".to_string(),
            compact(total),
            if total == 1 { "CONTRIBUTION" } else { "CONTRIBUTIONS" }.to_string(),
        ),
        (
            "LAST 30 DAYS".to_string(),
            compact(prs),
            if prs == 1 { "PR MERGED" } else { "PRS MERGED" }.to_string(),
        ),
        (
            "CURRENT STREAK".to_string(),
            compact(streak),
            if streak == 1 { "DAY IN A ROW" } else { "DAYS IN A ROW" }.to_string(),
        ),
    ];
    match language {
        Some((name, share)) => {
            let share = py_round(share * 100.0).max(1);
            let mut name = name.to_uppercase();
            if name.chars().count() as i64 > LABEL_TILES {
                // Cut on a word when the name is too long for the board ("JUPYTER NOTEBOOK").
                name = name
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(LABEL_TILES as usize)
                    .collect();
            }
            rows.push(("TOP LANGUAGE".to_string(), format!("{share}%"), name));
        }
        None => rows.push(("TOP LANGUAGE".to_string(), "--".to_string(), "NONE YET".to_string())),
    }
    rows.into_iter()
        .map(|(caption, value, label)| {
            // `value[-5:].rjust(5)` and `label[:13].ljust(13)`.
            let chars: Vec<char> = value.chars().collect();
            let tail: String = chars[chars.len().saturating_sub(VALUE_TILES as usize)..].iter().collect();
            let label: String = label.chars().take(LABEL_TILES as usize).collect();
            let cells: Cells = format!("{:>5}{:<13}", tail, label).chars().collect();
            (caption, cells)
        })
        .collect()
}

/// The flap face for one character, as 40 rows of 18 colours.
fn glyph_rows(ch: char, color: Rgb) -> Rows {
    let glyph = Font::F57.glyph(ch);
    let (gw, gh) = Font::F57.size();
    let mut rows: Rows = vec![vec![None; TILE_W as usize]; TILE_H as usize];
    let x0 = py_floordiv(TILE_W - gw * GLYPH_SCALE, 2) + 1;
    let y0 = py_floordiv(TILE_H - gh * GLYPH_SCALE, 2);
    for (index, bit) in glyph.bytes().enumerate() {
        if bit != b'1' {
            continue;
        }
        let gx = index as i64 % gw;
        let gy = index as i64 / gw;
        for sy in 0..GLYPH_SCALE {
            for sx in 0..GLYPH_SCALE {
                let y = y0 + gy * GLYPH_SCALE + sy;
                // Print slightly worn: a touch darker towards the bottom of each half.
                let shade = if y < HALF { 0.0 } else { 0.12 };
                rows[y as usize][(x0 + gx * GLYPH_SCALE + sx) as usize] = Some(mix(color, [0, 0, 0], shade));
            }
        }
    }
    rows
}

fn rows_for(cache: &mut Cache, ch: char, column: usize) -> Rows {
    let is_value = (column as i64) < VALUE_TILES;
    let color = if is_value { AMBER } else { WHITE };
    cache.entry((ch, is_value)).or_insert_with(|| glyph_rows(ch, color)).clone()
}

/// Painted flap surface: lit top half with a sheen, darker bottom half.
fn face_color(_x: i64, y: i64) -> Rgb {
    if y < HALF {
        if y == 0 {
            return FACE_EDGE;
        }
        if y <= 2 {
            return FACE_TOP_HI;
        }
        return FACE_TOP;
    }
    if y >= TILE_H - 2 {
        return [29, 29, 33];
    }
    FACE_BOTTOM
}

fn corner(x: i64, y: i64) -> bool {
    (x == 0 || x == TILE_W - 1) && (y == 0 || y == TILE_H - 1)
}

/// Draw one half of a flap, squashed toward the hinge by `scale_y` (turning flap).
fn put_half(canvas: &mut Canvas, tx: i64, ty: i64, rows: &Rows, half: Half, scale_y: f64, darken: f64) {
    let height = py_round(HALF as f64 * scale_y).max(0);
    if height == 0 {
        return;
    }
    for i in 0..height {
        // Sample the source row that maps to this screen row.
        let (src, y) = match half {
            Half::Top => (HALF - 1 - ((i as f64 + 0.5) / height as f64 * HALF as f64) as i64, HALF - 1 - i),
            Half::Bottom => (HALF + ((i as f64 + 0.5) / height as f64 * HALF as f64) as i64, HALF + i),
        };
        for x in 0..TILE_W {
            let check_y = if scale_y == 1.0 { y } else { src };
            if corner(x, check_y) {
                continue;
            }
            let mut color = rows[src as usize][x as usize].unwrap_or_else(|| face_color(x, src));
            if darken != 0.0 {
                color = mix(color, [0, 0, 0], darken);
            }
            canvas.put(tx + x, ty + y, color);
        }
    }
}

/// phase None = at rest showing `new`; else 0..1 through one flip from `old` to `new`.
fn draw_tile(canvas: &mut Canvas, tx: i64, ty: i64, old: Option<&Rows>, new: &Rows, phase: Option<f64>) {
    match phase {
        None => {
            put_half(canvas, tx, ty, new, Half::Top, 1.0, 0.0);
            put_half(canvas, tx, ty, new, Half::Bottom, 1.0, 0.0);
        }
        Some(phase) => {
            let old = old.expect("a turning flap needs its old face");
            // Behind the moving flap: the new top is already revealed, the old bottom still shows.
            put_half(canvas, tx, ty, new, Half::Top, 1.0, 0.0);
            put_half(canvas, tx, ty, old, Half::Bottom, 1.0, 0.0);
            if phase < 0.5 {
                let fold = (phase * PI).cos();
                put_half(canvas, tx, ty, old, Half::Top, fold, (1.0 - fold) * 0.55);
                // Shadow of the falling flap on the revealed face behind it.
                let height = py_round(HALF as f64 * fold);
                for x in 1..TILE_W - 1 {
                    canvas.put(tx + x, ty + HALF - 1 - height, SHADOW);
                    if 0 < height && height < HALF {
                        // The flap's free edge catches the light as it tips forward.
                        canvas.put(tx + x, ty + HALF - height, mix(FACE_EDGE, [255, 255, 255], 0.15));
                    }
                }
            } else {
                let fold = -(phase * PI).cos();
                // The flap's underside comes down: dark while near horizontal.
                put_half(canvas, tx, ty, new, Half::Bottom, fold, (1.0 - fold) * 0.6);
                let edge = HALF + py_round(HALF as f64 * fold);
                for x in 1..TILE_W - 1 {
                    for dy in 0..2 {
                        let y = edge + dy;
                        if HALF <= y && y < TILE_H - 1 {
                            let current = canvas.get(tx + x, ty + y);
                            canvas.put(tx + x, ty + y, mix(current, SHADOW, 0.6 - dy as f64 * 0.3));
                        }
                    }
                }
            }
        }
    }
    // Hinge: a dark slit across the middle and two pins in the gaps.
    for x in 0..TILE_W {
        canvas.put(tx + x, ty + HALF, HINGE);
    }
    for x in 1..TILE_W - 1 {
        let current = canvas.get(tx + x, ty + HALF + 1);
        canvas.put(tx + x, ty + HALF + 1, mix(current, HINGE, 0.45));
    }
    canvas.put(tx - 1, ty + HALF, PIN);
    canvas.put(tx + TILE_W, ty + HALF, PIN);
    canvas.put(tx - 1, ty + HALF - 1, mix(PIN, BOARD, 0.5));
    canvas.put(tx + TILE_W, ty + HALF - 1, mix(PIN, BOARD, 0.5));
}

fn draw_board() -> Canvas {
    let mut canvas = Canvas::with_background(W, H, BOARD);
    let mut rng = Random::new(7);
    for _ in 0..420 {
        let x = rng.randrange(4, W - 4);
        let y = rng.randrange(4, H - 4);
        canvas.put(x, y, BOARD_SPECK);
    }
    // Anodised frame, 3 px, lit from the top.
    for (i, color) in [FRAME_DARK, FRAME_LIGHT, FRAME_MID].into_iter().enumerate() {
        let i = i as i64;
        for x in i..W - i {
            canvas.put(x, i, if i != 1 { color } else { FRAME_LIGHT });
            canvas.put(x, H - 1 - i, if i != 2 { FRAME_DARK } else { FRAME_MID });
        }
        for y in i..H - i {
            canvas.put(i, y, color);
            canvas.put(W - 1 - i, y, if i != 2 { FRAME_DARK } else { FRAME_MID });
        }
    }
    for (x, y) in [(0, 0), (W - 1, 0), (0, H - 1), (W - 1, H - 1)] {
        canvas.put(x, y, PAGE);
    }
    // Inner shadow under the top frame bar.
    for x in 3..W - 3 {
        canvas.put(x, 3, [10, 10, 11]);
    }
    // Rivets.
    for (x, y) in [(7, 7), (W - 9, 7), (7, H - 9), (W - 9, H - 9)] {
        canvas.rect(x, y, x + 1, y + 1, FRAME_MID);
        canvas.put(x, y, RIVET);
        canvas.put(x + 2, y + 2, SHADOW);
    }
    // Tile wells: a recessed slot behind each column of flaps.
    for (x0, count) in [(VALUE_X, VALUE_TILES), (LABEL_X, LABEL_TILES)] {
        let x1 = x0 + count * PITCH - (PITCH - TILE_W);
        canvas.rect(x0 - 3, TILE_Y - 3, x1 + 2, TILE_Y + TILE_H + 2, [8, 8, 9]);
        for x in x0 - 3..x1 + 3 {
            canvas.put(x, TILE_Y + TILE_H + 3, [34, 34, 38]);
        }
    }
    canvas
}

fn footer(canvas: &mut Canvas, login: &str) {
    let text: String = login.to_uppercase().chars().take(30).collect();
    canvas.text((VALUE_X - 1) as f64, (H - 16) as f64, &text, [92, 88, 80], Font::F35, 2, None);
}

/// Per stat and per flap: when it starts turning and which characters it shows on the way.
fn make_plans(lines: &[(String, Cells)], rng: &mut Random) -> Vec<Vec<(i64, Cells)>> {
    let mut plans = Vec::new();
    for index in 0..lines.len() {
        let previous = &lines[(index + lines.len() - 1) % lines.len()].1;
        let cells = &lines[index].1;
        let mut plan = Vec::new();
        for (column, (old, new)) in previous.iter().zip(cells.iter()).enumerate() {
            if old == new {
                plan.push((0, vec![*new]));
                continue;
            }
            // Ripple left to right, with a little jitter so it never looks mechanical.
            let rounded = py_round((column as i64 * 4) as f64 / (VALUE_TILES + LABEL_TILES) as f64);
            let start = rounded + rng.choice(&[0i64, 0, 1]);
            let passes = rng.choice(&[1i64, 1, 2]);
            // Intermediate flaps follow the drum order, as on a real board.
            let at = DRUM.find(*old).map_or(-1, |position| position as i64);
            let mut seq = vec![*old];
            for k in 1..passes {
                let drum_index = py_imod(at + k, DRUM.len() as i64) as usize;
                seq.push(DRUM.as_bytes()[drum_index] as char);
            }
            seq.push(*new);
            plan.push((start, seq));
        }
        plans.push(plan);
    }
    plans
}

fn draw_frame(
    board: &Canvas,
    lines: &[(String, Cells)],
    plans: &[Vec<(i64, Cells)>],
    cache: &mut Cache,
    frame: usize,
) -> Canvas {
    let mut canvas = board.clone();
    let stat = frame / PER_STAT;
    let local = (frame % PER_STAT) as i64;
    // The caption and the lamp switch once most flaps have turned, not before.
    let shown = if local >= SWITCH_AT { stat } else { (stat + 3) % 4 };
    canvas.text((VALUE_X - 1) as f64, 10.0, &lines[shown].0, CAPTION, Font::F57, 2, Some(SHADOW));
    // Which stat is up: four lamps on the footer, the current one lit.
    for i in 0..4usize {
        let lx = W - 18 - (3 - i as i64) * 11;
        let ly = H - 15;
        let lit = i == shown;
        canvas.rect(lx, ly, lx + 5, ly + 5, [6, 6, 7]);
        canvas.rect(lx + 1, ly + 1, lx + 4, ly + 4, if lit { AMBER } else { LAMP_OFF });
        canvas.put(lx + 1, ly + 1, if lit { [255, 236, 180] } else { [70, 62, 48] });
        if lit {
            for (gx, gy) in [(lx - 1, ly + 2), (lx + 6, ly + 2), (lx + 2, ly - 1), (lx + 3, ly + 6)] {
                canvas.put(gx, gy, [90, 62, 20]);
            }
        }
    }
    for column in 0..(VALUE_TILES + LABEL_TILES) as usize {
        let tx = if (column as i64) < VALUE_TILES {
            VALUE_X + column as i64 * PITCH
        } else {
            LABEL_X + (column as i64 - VALUE_TILES) * PITCH
        };
        let (start, seq) = &plans[stat][column];
        let step = local - start;
        let last = seq.len() - 1;
        if seq.len() == 1 || step >= last as i64 * FLIP_FRAMES {
            let new = rows_for(cache, seq[last], column);
            draw_tile(&mut canvas, tx, TILE_Y, None, &new, None);
        } else if step < 0 {
            let new = rows_for(cache, seq[0], column);
            draw_tile(&mut canvas, tx, TILE_Y, None, &new, None);
        } else {
            let flip = (step / FLIP_FRAMES) as usize;
            let sub = step % FLIP_FRAMES;
            let phase = (sub as f64 + 0.5) / FLIP_FRAMES as f64;
            let old = rows_for(cache, seq[flip], column);
            let new = rows_for(cache, seq[flip + 1], column);
            draw_tile(&mut canvas, tx, TILE_Y, Some(&old), &new, Some(phase));
        }
        // Drop shadow of the flap on the well.
        for y in TILE_Y + 1..TILE_Y + TILE_H {
            if canvas.get(tx + TILE_W, y) == [8, 8, 9] {
                canvas.put(tx + TILE_W, y, [4, 4, 5]);
            }
        }
    }
    canvas
}

pub fn render(data: &Data) -> String {
    let lines = stats(data);
    let mut board = draw_board();
    footer(&mut board, &data.login);
    let mut rng = Random::new(11);
    let plans = make_plans(&lines, &mut rng);
    let mut cache = Cache::new();
    let frames: Vec<Canvas> = (0..LOOP_FRAMES)
        .map(|frame| draw_frame(&board, &lines, &plans, &mut cache, frame))
        .collect();
    // Still: the first stat fully settled, just before the board turns over.
    save(&frames, "departures", Save { still: Some(PER_STAT - 1), scale: SCALE, duration: FRAME_MS })
}
