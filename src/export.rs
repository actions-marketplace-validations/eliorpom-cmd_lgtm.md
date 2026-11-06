//! Port of `pixel.save`: GIF with one palette for the whole loop, plus a still PNG on request.
//!
//! The raw dump (`--raw DIR`) writes the art-size frames before any quantization, in the
//! same format as `bench/dump.py`; the parity bench compares those, so the GIF palette does
//! not have to match Pillow's median cut.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::pixel::{Canvas, Rgb, FRAME_MS, SCALE};

/// Output pixels per CSS pixel: GIFs are drawn twice as large as they are shown, for
/// high-density screens. The README snippets set the display width to half the pixel width.
pub const HIDPI: usize = 2;

pub struct Output {
    pub dir: PathBuf,
    pub raw: Option<PathBuf>,
    /// Skip GIF/PNG encoding (bench runs only need the raw frames).
    pub raw_only: bool,
    /// Also write a still PNG next to each GIF (`--stills`).
    pub stills: bool,
    /// Loop every animation, even those made to play once (`--loop`, for showcases).
    pub loop_all: bool,
}

static OUTPUT: OnceLock<Output> = OnceLock::new();

pub fn configure(output: Output) {
    OUTPUT.set(output).ok().expect("output configured twice");
}

fn output() -> &'static Output {
    OUTPUT.get().expect("export::configure must run first")
}

/// Options of `save(frames, name, still=None, scale=SCALE, duration=FRAME_MS)`.
pub struct Save {
    pub still: Option<usize>,
    pub scale: usize,
    pub duration: u32,
    /// Play the animation once and stay on the last frame, instead of looping.
    pub once: bool,
}

impl Default for Save {
    fn default() -> Self {
        Save { still: None, scale: SCALE, duration: FRAME_MS, once: false }
    }
}

pub fn save(frames: &[Canvas], name: &str, options: Save) -> String {
    let out = output();
    let still = options.still.unwrap_or(frames.len() - 1);
    if let Some(raw) = &out.raw {
        write_raw(&raw.join(format!("{name}.raw")), frames, options.duration, still);
    }
    let options = Save { scale: options.scale * HIDPI, ..options };
    // A play-once animation shown in a loop holds its last frame for 3 s before starting over.
    let held: Vec<Canvas>;
    let (frames, options) = if out.loop_all && options.once {
        let hold = (3000 / options.duration.max(1)) as usize;
        held = frames.iter().cloned().chain(std::iter::repeat_n(frames[frames.len() - 1].clone(), hold)).collect();
        (&held[..], Save { once: false, ..options })
    } else {
        (frames, options)
    };
    if !out.raw_only {
        std::fs::create_dir_all(&out.dir).expect("cannot create output directory");
        let band = crate::brand::band(name, frames[0].width as usize * options.scale);
        write_gif(&out.dir.join(format!("{name}.gif")), frames, options.scale, options.duration, options.once, &band);
        if out.stills {
            write_png(&out.dir.join(format!("{name}.png")), &frames[still], options.scale, &band);
        }
    }
    name.to_string()
}

/// `PXW1`, then width, height, frame count, frame duration (ms), still index as u32 LE,
/// then every frame as RGB bytes, row by row.
fn write_raw(path: &PathBuf, frames: &[Canvas], duration: u32, still: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("cannot create raw directory");
    let mut file = BufWriter::new(File::create(path).expect("cannot write raw dump"));
    file.write_all(b"PXW1").unwrap();
    for value in [frames[0].width as u32, frames[0].height as u32, frames.len() as u32, duration, still as u32] {
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    for frame in frames {
        for pixel in &frame.pixels {
            file.write_all(pixel).unwrap();
        }
    }
}

fn write_png(path: &PathBuf, frame: &Canvas, scale: usize, (band_h, band): &(usize, Vec<Rgb>)) {
    let (w, art_h) = (frame.width as usize * scale, frame.height as usize * scale);
    let h = art_h + band_h;
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let mut data = Vec::with_capacity(w * h * 3);
    for y in 0..art_h {
        for x in 0..w {
            data.extend_from_slice(&frame.pixels[(y / scale) * frame.width as usize + x / scale]);
        }
    }
    for pixel in band {
        data.extend_from_slice(pixel);
    }
    writer.write_image_data(&data).unwrap();
}

/// The footer band is part of the first frame only: later frames keep it (disposal "keep").
fn write_gif(path: &PathBuf, frames: &[Canvas], scale: usize, duration: u32, once: bool, (band_h, band): &(usize, Vec<Rgb>)) {
    let (mut palette, index_of) = build_palette(frames, band);
    // One extra slot after the colours: transparent, for pixels unchanged since the
    // previous frame. Same trick as Pillow, it roughly halves the file size.
    let transparent = palette.len() as u8;
    palette.push([0, 0, 0]);
    let palette = padded(palette);
    let (aw, ah) = (frames[0].width as usize, frames[0].height as usize);
    let (w, h) = (aw * scale, ah * scale + band_h);
    let mut flat = Vec::with_capacity(palette.len() * 3);
    for color in &palette {
        flat.extend_from_slice(color);
    }
    let file = BufWriter::new(File::create(path).unwrap());
    let mut encoder = gif::Encoder::new(file, w as u16, h as u16, &flat).unwrap();
    // Without the looping extension, a GIF plays once and stays on its last frame.
    if !once {
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
    }

    let indexed: Vec<Vec<u8>> =
        frames.iter().map(|frame| frame.pixels.iter().map(|p| index_of[p]).collect()).collect();
    for (n, frame) in indexed.iter().enumerate() {
        // Like Pillow: frames after the first only carry the box that changed.
        let (x0, y0, x1, y1) = if n == 0 {
            (0, 0, aw - 1, ah - 1)
        } else {
            changed_box(&indexed[n - 1], frame, aw, ah).unwrap_or_default()
        };
        let (bw, bh) = ((x1 - x0 + 1) * scale, (y1 - y0 + 1) * scale);
        let rows = if n == 0 { bh + band_h } else { bh };
        let mut buffer = Vec::with_capacity(bw * bh);
        for y in 0..bh {
            for x in 0..bw {
                let at = (y0 + y / scale) * aw + x0 + x / scale;
                let unchanged = n > 0 && indexed[n - 1][at] == frame[at];
                buffer.push(if unchanged { transparent } else { frame[at] });
            }
        }
        if n == 0 {
            buffer.extend(band.iter().map(|pixel| index_of[pixel]));
        }
        let mut gif_frame = gif::Frame::default();
        gif_frame.left = (x0 * scale) as u16;
        gif_frame.top = (y0 * scale) as u16;
        gif_frame.width = bw as u16;
        gif_frame.height = rows as u16;
        gif_frame.buffer = buffer.into();
        // Pillow writes int(duration / 10) hundredths of a second.
        gif_frame.delay = (duration / 10) as u16;
        gif_frame.dispose = gif::DisposalMethod::Keep;
        if n > 0 {
            gif_frame.transparent = Some(transparent);
        }
        encoder.write_frame(&gif_frame).unwrap();
    }
}

fn changed_box(a: &[u8], b: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if a[y * w + x] != b[y * w + x] {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != usize::MAX).then_some((x0, y0, x1, y1))
}

/// One palette for the whole loop: exact when the loop has at most 255 colours,
/// otherwise a median cut down to 255 colours with nearest-colour mapping.
/// The 256th slot is kept free for transparency.
fn build_palette(frames: &[Canvas], extra: &[Rgb]) -> (Vec<Rgb>, HashMap<Rgb, u8>) {
    let mut histogram: HashMap<Rgb, u64> = HashMap::new();
    for pixel in frames.iter().flat_map(|frame| frame.pixels.iter()).chain(extra) {
        *histogram.entry(*pixel).or_default() += 1;
    }
    let mut colors: Vec<(Rgb, u64)> = histogram.into_iter().collect();
    colors.sort();
    if colors.len() <= 255 {
        let palette: Vec<Rgb> = colors.iter().map(|(c, _)| *c).collect();
        let index = palette.iter().enumerate().map(|(i, c)| (*c, i as u8)).collect();
        return (palette, index);
    }
    let palette = median_cut(colors.clone(), 255);
    let index = colors
        .iter()
        .map(|(color, _)| {
            let nearest = palette
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| (0..3).map(|i| (p[i] as i32 - color[i] as i32).pow(2)).sum::<i32>())
                .unwrap()
                .0;
            (*color, nearest as u8)
        })
        .collect();
    (palette, index)
}

/// GIF palettes have a power-of-two size; pad with black.
fn padded(mut palette: Vec<Rgb>) -> Vec<Rgb> {
    let size = palette.len().max(2).next_power_of_two();
    palette.resize(size, [0, 0, 0]);
    palette
}

fn median_cut(colors: Vec<(Rgb, u64)>, target: usize) -> Vec<Rgb> {
    let mut boxes = vec![colors];
    while boxes.len() < target {
        // Split the box with the widest channel range (ties: the most populated).
        let Some((pick, channel)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let range = |c: usize| {
                    let lo = b.iter().map(|(p, _)| p[c]).min().unwrap();
                    let hi = b.iter().map(|(p, _)| p[c]).max().unwrap();
                    hi - lo
                };
                let channel = (0..3).max_by_key(|&c| range(c)).unwrap();
                (i, channel, range(channel), b.iter().map(|(_, n)| n).sum::<u64>())
            })
            .max_by_key(|&(_, _, range, population)| (range, population))
            .map(|(i, channel, _, _)| (i, channel))
        else {
            break;
        };
        let mut chosen = boxes.swap_remove(pick);
        chosen.sort_by_key(|(p, _)| p[channel]);
        let total: u64 = chosen.iter().map(|(_, n)| n).sum();
        let mut running = 0;
        let mut cut = 1;
        for (i, (_, n)) in chosen.iter().enumerate() {
            running += n;
            if running * 2 >= total {
                cut = (i + 1).clamp(1, chosen.len() - 1);
                break;
            }
        }
        let rest = chosen.split_off(cut);
        boxes.push(chosen);
        boxes.push(rest);
    }
    boxes
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|(_, n)| n).sum();
            let mean = |c: usize| (b.iter().map(|(p, n)| p[c] as u64 * n).sum::<u64>() as f64 / total as f64).round() as u8;
            [mean(0), mean(1), mean(2)]
        })
        .collect()
}
