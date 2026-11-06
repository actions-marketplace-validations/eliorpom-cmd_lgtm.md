//! Port of `pixel.py`: canvas, fonts, panel, colour helpers and statistics.

use crate::data::Day;
use crate::pymath::{py_round, Coord};

pub type Rgb = [u8; 3];

/// Every loop lasts 64 frames by default; cyclic motion uses whole cycles.
pub const FRAMES: usize = 64;
pub const FRAME_MS: u32 = 80;
/// Output pixels per art pixel.
pub const SCALE: usize = 2;

pub const BAYER: [[i64; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

pub const BG: Rgb = [16, 20, 28];
pub const BORDER: Rgb = [42, 49, 64];
pub const TEXT: Rgb = [230, 237, 243];
pub const MUTED: Rgb = [139, 148, 158];
pub const GREEN: Rgb = [63, 185, 80];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    F57,
    F35,
}

impl Font {
    pub fn size(self) -> (i64, i64) {
        match self {
            Font::F57 => (5, 7),
            Font::F35 => (3, 5),
        }
    }

    /// Glyph bits row by row; unknown characters fall back to a space, like Python.
    pub fn glyph(self, ch: char) -> &'static str {
        let found = match self {
            Font::F57 => font57(ch),
            Font::F35 => font35(ch),
        };
        found.unwrap_or(match self {
            Font::F57 => "00000000000000000000000000000000000",
            Font::F35 => "000000000000000",
        })
    }
}

pub fn font57(ch: char) -> Option<&'static str> {
    Some(match ch {
        '0' => "01110100011001110101110011000101110", '1' => "00100011000010000100001000010001110",
        '2' => "01110100010000100010001000100011111", '3' => "11111000100010000010000011000101110",
        '4' => "00010001100101010010111110001000010", '5' => "11111100001111000001000011000101110",
        '6' => "00110010001000011110100011000101110", '7' => "11111000010001000100010000100001000",
        '8' => "01110100011000101110100011000101110", '9' => "01110100011000101111000010001001100",
        'A' => "01110100011000111111100011000110001", 'B' => "11110100011000111110100011000111110",
        'C' => "01110100011000010000100001000101110", 'D' => "11100100101000110001100011001011100",
        'E' => "11111100001000011110100001000011111", 'F' => "11111100001000011110100001000010000",
        'G' => "01110100011000010111100011000101111", 'H' => "10001100011000111111100011000110001",
        'I' => "01110001000010000100001000010001110", 'J' => "00111000100001000010000101001001100",
        'K' => "10001100101010011000101001001010001", 'L' => "10000100001000010000100001000011111",
        'M' => "10001110111010110101100011000110001", 'N' => "10001100011100110101100111000110001",
        'O' => "01110100011000110001100011000101110", 'P' => "11110100011000111110100001000010000",
        'Q' => "01110100011000110001101011001001101", 'R' => "11110100011000111110101001001010001",
        'S' => "01111100001000001110000010000111110", 'T' => "11111001000010000100001000010000100",
        'U' => "10001100011000110001100011000101110", 'V' => "10001100011000110001100010101000100",
        'W' => "10001100011000110101101011010101010", 'X' => "10001100010101000100010101000110001",
        'Y' => "10001100011000101010001000010000100", 'Z' => "11111000010001000100010001000011111",
        ' ' => "00000000000000000000000000000000000", '.' => "00000000000000000000000000110001100",
        '-' => "00000000000000011111000000000000000", '+' => "00000001000010011111001000010000000",
        '/' => "00001000100001000100010001000010000", '%' => "11000110010001000100010001001100011",
        ':' => "00000011000110000000011000110000000",
        _ => return None,
    })
}

pub fn font35(ch: char) -> Option<&'static str> {
    Some(match ch {
        'A' => "010101111101101", 'B' => "110101110101110", 'C' => "011100100100011", 'D' => "110101101101110",
        'E' => "111100110100111", 'F' => "111100110100100", 'G' => "011100101101011", 'H' => "101101111101101",
        'I' => "111010010010111", 'J' => "001001001101010", 'K' => "101101110101101", 'L' => "100100100100111",
        'M' => "101111111101101", 'N' => "110101101101101", 'O' => "010101101101010", 'P' => "110101110100100",
        'Q' => "010101101110011", 'R' => "110101110101101", 'S' => "011100010001110", 'T' => "111010010010010",
        'U' => "101101101101111", 'V' => "101101101101010", 'W' => "101101111111101", 'X' => "101101010101101",
        'Y' => "101101010010010", 'Z' => "111001010100111", '0' => "111101101101111", '1' => "010110010010111",
        '2' => "110001010100111", '3' => "110001010001110", '4' => "101101111001001", '5' => "111100110001110",
        '6' => "011100111101111", '7' => "111001010010010", '8' => "111101111101111", '9' => "111101111001110",
        ' ' => "000000000000000", '.' => "000000000000010", '+' => "000010111010000", '-' => "000000111000000",
        '/' => "001001010100100",
        _ => return None,
    })
}

/// `rgb("#3572A5")`.
pub fn rgb(hex: &str) -> Rgb {
    let hex = hex.trim_start_matches('#');
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    [part(0), part(2), part(4)]
}

/// Inverse of `rgb`: `[53, 114, 165]` → `"#3572a5"`.
pub fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

/// `mix(a, b, ratio)`: per channel `round(x + (y - x) * ratio)`, half to even.
pub fn mix(a: Rgb, b: Rgb, ratio: f64) -> Rgb {
    let channel = |i: usize| {
        let (x, y) = (a[i] as f64, b[i] as f64);
        py_round(x + (y - x) * ratio) as u8
    };
    [channel(0), channel(1), channel(2)]
}

/// Number of characters Python's `len(text)` would count.
pub fn char_len(text: &str) -> i64 {
    text.chars().count() as i64
}

pub fn text_width(text: &str, font: Font, scale: i64) -> i64 {
    let (width, _) = font.size();
    (char_len(text) * (width + 1) - 1) * scale
}

#[derive(Clone)]
pub struct Canvas {
    pub width: i64,
    pub height: i64,
    pub pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(width: i64, height: i64) -> Self {
        Self::with_background(width, height, BG)
    }

    pub fn with_background(width: i64, height: i64, background: Rgb) -> Self {
        Canvas { width, height, pixels: vec![background; (width * height) as usize] }
    }

    /// `put(x, y, color)`: coordinates rounded half to even, out-of-bounds ignored.
    pub fn put(&mut self, x: impl Coord, y: impl Coord, color: Rgb) {
        let (x, y) = (x.px(), y.px());
        if 0 <= x && x < self.width && 0 <= y && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    pub fn get(&self, x: i64, y: i64) -> Rgb {
        // Python indexing accepts negatives from the end; widgets only read inside the image.
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn rect(&mut self, x0: impl Coord, y0: impl Coord, x1: impl Coord, y1: impl Coord, color: Rgb) {
        for y in y0.px()..=y1.px() {
            for x in x0.px()..=x1.px() {
                self.put(x, y, color);
            }
        }
    }

    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, color: Rgb) {
        let y_start = (cy - r).floor() as i64;
        let y_end = (cy + r).ceil() as i64;
        let x_start = (cx - r).floor() as i64;
        let x_end = (cx + r).ceil() as i64;
        for y in y_start..=y_end {
            for x in x_start..=x_end {
                if (x as f64 - cx).powf(2.0) + (y as f64 - cy).powf(2.0) <= r * r {
                    self.put(x, y, color);
                }
            }
        }
    }

    /// Bresenham line between rounded end points.
    pub fn line(&mut self, x0: impl Coord, y0: impl Coord, x1: impl Coord, y1: impl Coord, color: Rgb) {
        let (mut x0, mut y0, x1, y1) = (x0.px(), y0.px(), x1.px(), y1.px());
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                return;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// `text(x, y, text, color, font, scale, shadow)`. `x` and `y` are floats because
    /// Python adds offsets before rounding, and half-to-even rounding does not commute.
    pub fn text(&mut self, x: f64, y: f64, text: &str, color: Rgb, font: Font, scale: i64, shadow: Option<Rgb>) {
        let (width, _) = font.size();
        for (offset, ch) in text.to_uppercase().chars().enumerate() {
            let glyph = font.glyph(ch);
            for (index, bit) in glyph.bytes().enumerate() {
                if bit != b'1' {
                    continue;
                }
                let index = index as i64;
                let gx = x + ((offset as i64 * (width + 1) + index % width) * scale) as f64;
                let gy = y + ((index / width) * scale) as f64;
                for sy in 0..scale {
                    for sx in 0..scale {
                        if let Some(shadow) = shadow {
                            self.put(gx + sx as f64 + 1.0, gy + sy as f64 + 1.0, shadow);
                        }
                        self.put(gx + sx as f64, gy + sy as f64, color);
                    }
                }
            }
        }
    }

    pub fn text_center(&mut self, cx: f64, y: f64, text: &str, color: Rgb, font: Font, scale: i64, shadow: Option<Rgb>) {
        let left = py_round(cx - text_width(text, font, scale) as f64 / 2.0);
        self.text(left as f64, y, text, color, font, scale, shadow);
    }

    pub fn text_right(&mut self, x: f64, y: f64, text: &str, color: Rgb, font: Font, scale: i64, shadow: Option<Rgb>) {
        self.text(x - text_width(text, font, scale) as f64 + 1.0, y, text, color, font, scale, shadow);
    }
}

/// Card background with a 1 px border and cut corners.
pub fn panel(canvas: &mut Canvas, background: Rgb, border: Rgb) {
    let (w, h) = (canvas.width, canvas.height);
    canvas.rect(0, 0, w - 1, h - 1, background);
    for x in 1..w - 1 {
        canvas.put(x, 0, border);
        canvas.put(x, h - 1, border);
    }
    for y in 1..h - 1 {
        canvas.put(0, y, border);
        canvas.put(w - 1, y, border);
    }
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        canvas.put(x, y, [13, 17, 23]);
    }
}

pub fn dither_on(x: i64, y: i64, ratio: f64) -> bool {
    ratio * 16.0 > BAYER[y.rem_euclid(4) as usize][x.rem_euclid(4) as usize] as f64
}

/// 0 to 1 on a fixed log scale, so quiet accounts really look quiet.
pub fn log_level(count: i64, full: i64) -> f64 {
    if count > 0 { 1.0f64.min((count as f64).ln_1p() / (full as f64).ln_1p()) } else { 0.0 }
}

// Statistics derived from the collected calendar.

pub fn counts(days: &[Day]) -> Vec<i64> {
    days.iter().map(|day| day.count).collect()
}

pub fn longest_run(days: &[Day], active: bool) -> i64 {
    let values = counts(days);
    // Days before the first contribution are not a break, the account was not in use yet.
    let first = values.iter().position(|&count| count > 0).unwrap_or(values.len());
    let (mut best, mut run) = (0, 0);
    for &count in &values[first..] {
        run = if (count > 0) == active { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// First day with the highest count, like Python's `max(key=...)`.
pub fn best_day(days: &[Day]) -> &Day {
    let mut best = &days[0];
    for day in &days[1..] {
        if day.count > best.count {
            best = day;
        }
    }
    best
}

/// Sums per 7-day block, ending yesterday, oldest first.
pub fn weekly_totals(days: &[Day]) -> Vec<i64> {
    let values = counts(days);
    let values = &values[values.len() % 7..];
    values.chunks(7).map(|chunk| chunk.iter().sum()).collect()
}

/// "2026-11-02" -> "NOV 02".
pub fn short_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let month: usize = iso[5..7].parse().unwrap_or(1);
    format!("{} {}", MONTHS[month - 1], &iso[8..10])
}

/// Fallback colours for languages whose GitHub colours look alike: bright, far apart, and
/// readable on the dark cards.
const DISTINCT: [Rgb; 10] = [
    [64, 160, 255], [247, 185, 52], [120, 200, 80], [230, 72, 46], [170, 110, 230],
    [240, 110, 170], [60, 200, 190], [250, 240, 120], [200, 200, 210], [150, 90, 50],
];

fn lab(color: Rgb) -> [f64; 3] {
    let linear = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (linear(color[0]), linear(color[1]), linear(color[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
}

/// Perceptual distance between two colours (CIE76).
pub fn color_distance(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (lab(a), lab(b));
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// One colour per language that stays tellable apart. In order (most used first), a language
/// keeps its GitHub colour when it is far from the colours already kept; the others then get
/// the fallback colour farthest from every colour in use, so a replacement never takes a
/// colour people expect for another language. "Other" stays grey.
pub fn distinct_colors(languages: &[(String, Rgb)]) -> Vec<(String, Rgb)> {
    const TOO_CLOSE: f64 = 45.0;
    const OTHER: Rgb = [139, 148, 158];
    let nearest = |c: Rgb, used: &[Rgb]| used.iter().map(|o| color_distance(c, *o)).fold(f64::MAX, f64::min);
    let mut kept: Vec<Option<Rgb>> = Vec::new();
    let mut used: Vec<Rgb> = Vec::new();
    for (name, color) in languages {
        let color = if name == "Other" { OTHER } else { *color };
        if name == "Other" || nearest(color, &used) >= TOO_CLOSE {
            kept.push(Some(color));
            used.push(color);
        } else {
            kept.push(None);
        }
    }
    let mut result = Vec::new();
    for ((name, _), kept) in languages.iter().zip(kept) {
        let color = kept.unwrap_or_else(|| {
            let pick = *DISTINCT
                .iter()
                .max_by(|a, b| nearest(**a, &used).partial_cmp(&nearest(**b, &used)).unwrap())
                .unwrap();
            used.push(pick);
            pick
        });
        result.push((name.clone(), color));
    }
    result
}
