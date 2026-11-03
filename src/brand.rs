//! The footer band under every published widget: what it shows and when it was drawn on
//! the left, "✓ Looks good to README" in the middle, the repository on the right.
//!
//! Drawn at output resolution in Mona Sans (GitHub's typeface, SIL Open Font License, see
//! assets/fonts/OFL.txt), after the widget itself, so the raw frames of the parity bench stay
//! unmarked.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

use crate::pixel::{mix, Rgb};

pub const REPO: &str = "eliorpom-cmd/lgtm.md";
const MOTTO: &str = "✓ Looks good to README";
const HEIGHT: usize = 20;
const SIZE: f32 = 12.0;
const PAD: f32 = 8.0;
/// GitHub's dark page colour, so the band reads as a caption rather than part of the scene.
const BACKGROUND: Rgb = [13, 17, 23];
const QUIET: Rgb = [158, 166, 176];
const LINK: Rgb = [214, 220, 226];

static REGULAR: &[u8] = include_bytes!("../assets/fonts/MonaSans-Regular.ttf");
static MEDIUM: &[u8] = include_bytes!("../assets/fonts/MonaSans-Medium.ttf");

/// What a widget shows, for the left of the band.
fn caption(name: &str) -> String {
    let fixed = match name {
        "flame" => "Commit streak",
        "counter" => "Contributions · 12 months",
        "hourglass" => "Commits · 12 months",
        "ecg" => "Contributions · 7 days",
        "departures" => "Activity · 30 days",
        "donut" => "Languages",
        "grass" => "Activity · 90 days",
        "cherry-tree" => "Activity · 90 days",
        "forest" => "Weekly commits · 12 months",
        "fireworks" => "Merged PRs · 7 days",
        "fireworks-commits" => "Commits · 7 days",
        "records" => "Records · 12 months",
        "median" => "vs people I follow · 12 months",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match crate::widgets::parse_duel(name) {
        Some((login, days)) => format!("vs @{login} · {}", if days == 365 { "12 months".into() } else { format!("{days} days") }),
        None => String::new(),
    }
}

/// "updated Nov 3", in UTC like the data.
fn updated() -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (_, m, d) = crate::dates::civil_from_days(crate::dates::today());
    format!("updated {} {d}", MONTHS[(m - 1) as usize])
}

struct Run<'a> {
    text: String,
    font: &'a FontRef<'a>,
    color: Rgb,
}

fn width(run: &Run) -> f32 {
    let font = run.font.as_scaled(PxScale::from(SIZE));
    let mut total = 0.0;
    let mut previous = None;
    for ch in run.text.chars() {
        let id = font.glyph_id(ch);
        if let Some(prev) = previous {
            total += font.kern(prev, id);
        }
        total += font.h_advance(id);
        previous = Some(id);
    }
    total
}

/// Blends `run` into the band, its left edge at `x`. Coverage is snapped to eight steps so
/// the text adds only a handful of colours to the GIF palette.
fn draw(band: &mut [Rgb], band_width: usize, run: &Run, x: f32) {
    let scaled = run.font.as_scaled(PxScale::from(SIZE));
    let baseline = ((HEIGHT as f32 + scaled.ascent() + scaled.descent()) / 2.0).round();
    let mut caret = x;
    let mut previous = None;
    for ch in run.text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(SIZE), ab_glyph::point(caret.round(), baseline));
        if let Some(outline) = run.font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let (px, py) = (bounds.min.x as i64 + gx as i64, bounds.min.y as i64 + gy as i64);
                if px < 0 || py < 0 || px as usize >= band_width || py as usize >= HEIGHT {
                    return;
                }
                let step = (coverage.clamp(0.0, 1.0) * 8.0).round() / 8.0;
                if step > 0.0 {
                    let at = py as usize * band_width + px as usize;
                    band[at] = mix(BACKGROUND, run.color, step as f64);
                }
            });
        }
        caret += scaled.h_advance(id);
        previous = Some(id);
    }
}

/// The band for one widget, `band_width` output pixels wide: (height, pixels row by row).
pub fn band(name: &str, band_width: usize) -> (usize, Vec<Rgb>) {
    let regular = FontRef::try_from_slice(REGULAR).expect("bundled font");
    let medium = FontRef::try_from_slice(MEDIUM).expect("bundled font");
    let mut pixels = vec![BACKGROUND; band_width * HEIGHT];
    let caption = caption(name);
    let left = Run {
        text: if caption.is_empty() { updated() } else { format!("{caption} · {}", updated()) },
        font: &regular,
        color: QUIET,
    };
    let middle = Run { text: MOTTO.to_string(), font: &regular, color: QUIET };
    let right = Run { text: REPO.to_string(), font: &medium, color: LINK };
    let full = band_width as f32;
    let (lw, mw, rw) = (width(&left), width(&middle), width(&right));
    draw(&mut pixels, band_width, &right, full - PAD - rw);
    // The motto sits centred between the two ends, and only when it has room to breathe.
    let gap = 16.0;
    let middle_x = (full - mw) / 2.0;
    let fits_centred = middle_x >= PAD + lw + gap && middle_x + mw <= full - PAD - rw - gap;
    if fits_centred {
        draw(&mut pixels, band_width, &middle, middle_x);
        draw(&mut pixels, band_width, &left, PAD);
    } else if PAD + lw + gap + rw + PAD <= full {
        draw(&mut pixels, band_width, &left, PAD);
    }
    (HEIGHT, pixels)
}
