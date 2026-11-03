//! Python numeric semantics that differ from Rust's defaults.
//!
//! Porting rules:
//! - `round(x)` in Python rounds half to even: use `py_round`, never `f64::round`.
//! - `a % b` in Python takes the sign of `b`: use `py_mod` (floats) or `py_imod` (ints).
//! - `a // b` floors: use `py_floordiv`.
//! - `int(x)` truncates toward zero: `x as i64` matches.
//! - `x ** y` with a float: `x.powf(y)` (same libm call as CPython).
//! - `math.hypot(a, b)` is CPython's own algorithm, not libm: use `py_hypot`.
//! - `sum(floats)` is compensated since Python 3.12: use `py_sum`.

/// Python `round(x)` to an integer: half to even.
pub fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// Python float `a % b`.
pub fn py_mod(a: f64, b: f64) -> f64 {
    let m = a % b;
    if m != 0.0 {
        if (b < 0.0) != (m < 0.0) { m + b } else { m }
    } else {
        0.0f64.copysign(b)
    }
}

/// Python int `a % b`.
pub fn py_imod(a: i64, b: i64) -> i64 {
    let m = a % b;
    if m != 0 && ((m < 0) != (b < 0)) { m + b } else { m }
}

/// Python int `a // b`.
pub fn py_floordiv(a: i64, b: i64) -> i64 {
    let q = a / b;
    if a % b != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q }
}

/// Python float `a // b`.
pub fn py_ffloordiv(a: f64, b: f64) -> f64 {
    let m = a % b;
    let mut div = (a - m) / b;
    if m != 0.0 && ((b < 0.0) != (m < 0.0)) {
        div -= 1.0;
    }
    if div != 0.0 {
        let floor = div.floor();
        if div - floor > 0.5 { floor + 1.0 } else { floor }
    } else {
        0.0f64.copysign(a / b)
    }
}

/// `math.hypot(a, b)` exactly as CPython computes it (`vector_norm` in mathmodule.c).
pub fn py_hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a.is_infinite() || b.is_infinite() {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = if b > a { b } else { a };
    if max == 0.0 {
        return max;
    }
    let max_e = frexp_exponent(max);
    if max_e < -1023 {
        // Subnormal range: never reached by the widgets, kept for completeness.
        return f64::MIN_POSITIVE * py_hypot(a / f64::MIN_POSITIVE, b / f64::MIN_POSITIVE);
    }
    let scale = ldexp1(-max_e);
    let (mut csum, mut frac1, mut frac2) = (1.0f64, 0.0f64, 0.0f64);
    for value in [a, b] {
        let x = value * scale;
        let (hi, lo) = dl_mul(x, x);
        let (s_hi, s_lo) = dl_fast_sum(csum, hi);
        csum = s_hi;
        frac1 += lo;
        frac2 += s_lo;
    }
    let mut h = (csum - 1.0 + (frac1 + frac2)).sqrt();
    let (hi, lo) = dl_mul(-h, h);
    let (s_hi, s_lo) = dl_fast_sum(csum, hi);
    csum = s_hi;
    frac1 += lo;
    frac2 += s_lo;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    h / scale
}

fn dl_mul(x: f64, y: f64) -> (f64, f64) {
    let hi = x * y;
    (hi, x.mul_add(y, -hi))
}

fn dl_fast_sum(a: f64, b: f64) -> (f64, f64) {
    let hi = a + b;
    (hi, (a - hi) + b)
}

/// Exponent `e` of C `frexp(x)`, with x = m * 2^e and 0.5 <= m < 1.
fn frexp_exponent(x: f64) -> i32 {
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        // Subnormal: normalise first.
        return frexp_exponent(x * 2f64.powi(64)) - 64;
    }
    exp - 1022
}

/// `ldexp(1.0, e)` for the exponents `py_hypot` needs.
fn ldexp1(e: i32) -> f64 {
    if (-1022..=1023).contains(&e) {
        f64::from_bits(((e + 1023) as u64) << 52)
    } else {
        2f64.powi(e)
    }
}

/// Python 3.12+ `sum()` over floats (Neumaier compensated summation).
pub fn py_sum<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let (mut total, mut c) = (0.0f64, 0.0f64);
    for x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            c += (total - t) + x;
        } else {
            c += (x - t) + total;
        }
        total = t;
    }
    if c != 0.0 && c.is_finite() {
        total += c;
    }
    total
}

/// `statistics.median` over integers, as a float.
pub fn py_median(values: &[i64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let n = sorted.len();
    if n % 2 == 1 {
        Some(sorted[n / 2] as f64)
    } else {
        // Python: (data[i - 1] + data[i]) / 2 with exact ints, then true division.
        Some((sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0)
    }
}

/// Coordinates accepted by the canvas: Python rounds whatever it gets, ints stay as is.
pub trait Coord: Copy {
    fn px(self) -> i64;
}
impl Coord for f64 {
    fn px(self) -> i64 {
        py_round(self)
    }
}
impl Coord for i64 {
    fn px(self) -> i64 {
        self
    }
}
impl Coord for i32 {
    fn px(self) -> i64 {
        self as i64
    }
}
impl Coord for usize {
    fn px(self) -> i64 {
        self as i64
    }
}
