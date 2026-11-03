//! Bit-exact clone of CPython's `random.Random` (Mersenne Twister MT19937).
//!
//! The widgets were designed in Python with seeded generators; reproducing the same
//! sequence is what lets the Rust port draw identical scenes. Every method mirrors the
//! CPython 3.12+ implementation, including the order of the underlying draws.

use sha2::{Digest, Sha512};

const N: usize = 624;
const M: usize = 397;

/// Seed accepted by `Random::new`: an integer or a string, like Python.
pub enum Seed<'a> {
    Int(i64),
    Str(&'a str),
}

impl From<i64> for Seed<'_> {
    fn from(value: i64) -> Self {
        Seed::Int(value)
    }
}
impl From<i32> for Seed<'_> {
    fn from(value: i32) -> Self {
        Seed::Int(value as i64)
    }
}
impl From<usize> for Seed<'_> {
    fn from(value: usize) -> Self {
        Seed::Int(value as i64)
    }
}
impl<'a> From<&'a str> for Seed<'a> {
    fn from(value: &'a str) -> Self {
        Seed::Str(value)
    }
}

pub struct Random {
    state: [u32; N],
    index: usize,
    gauss_next: Option<f64>,
}

impl Random {
    /// Same as `random.Random(seed)` in Python.
    pub fn new<'a>(seed: impl Into<Seed<'a>>) -> Self {
        let key = match seed.into() {
            Seed::Int(value) => words_from_le_bytes(&value.unsigned_abs().to_le_bytes()),
            Seed::Str(text) => {
                // Python (version 2 seeding): int.from_bytes(s + sha512(s).digest(), "big").
                let mut bytes = text.as_bytes().to_vec();
                bytes.extend_from_slice(&Sha512::digest(text.as_bytes()));
                bytes.reverse();
                words_from_le_bytes(&bytes)
            }
        };
        let mut rng = Random { state: [0; N], index: N, gauss_next: None };
        rng.init_by_array(&key);
        rng
    }

    fn init_genrand(&mut self, seed: u32) {
        self.state[0] = seed;
        for i in 1..N {
            let prev = self.state[i - 1];
            self.state[i] = 1812433253u32.wrapping_mul(prev ^ (prev >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        let mut k = N.max(key.len());
        while k > 0 {
            let prev = self.state[i - 1];
            self.state[i] = (self.state[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            let prev = self.state[i - 1];
            self.state[i] =
                (self.state[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
            k -= 1;
        }
        self.state[0] = 0x8000_0000;
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for kk in 0..N {
                let y = (self.state[kk] & 0x8000_0000) | (self.state[(kk + 1) % N] & 0x7fff_ffff);
                let mag = if y & 1 == 1 { 0x9908_b0df } else { 0 };
                self.state[kk] = self.state[(kk + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `random.random()`: 53-bit float in [0, 1).
    pub fn random(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67108864.0 + b) * (1.0 / 9007199254740992.0)
    }

    /// `random.getrandbits(k)` for k <= 64.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        assert!(k <= 64, "getrandbits above 64 bits is not needed by the widgets");
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return (self.next_u32() >> (32 - k)) as u64;
        }
        // Words are generated least significant first, the last one is shifted.
        let low = self.next_u32() as u64;
        let high = (self.next_u32() >> (64 - k)) as u64;
        low | (high << 32)
    }

    fn randbelow(&mut self, n: u64) -> u64 {
        let k = 64 - n.leading_zeros();
        let mut r = self.getrandbits(k);
        while r >= n {
            r = self.getrandbits(k);
        }
        r
    }

    /// `random.uniform(a, b)`.
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// `random.randrange(start, stop)` with step 1.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        let width = stop - start;
        assert!(width > 0, "empty range for randrange({start}, {stop})");
        start + self.randbelow(width as u64) as i64
    }

    /// `random.randrange(stop)`.
    pub fn randrange_to(&mut self, stop: i64) -> i64 {
        assert!(stop > 0, "empty range for randrange({stop})");
        self.randbelow(stop as u64) as i64
    }

    /// `random.randrange(start, stop, step)` with any non-zero step.
    pub fn randrange_step(&mut self, start: i64, stop: i64, step: i64) -> i64 {
        if step == 1 {
            return self.randrange(start, stop);
        }
        let width = stop - start;
        let n = if step > 0 { (width + step - 1).div_euclid(step) } else { floor_div(width + step + 1, step) };
        assert!(n > 0, "empty range for randrange({start}, {stop}, {step})");
        start + step * self.randbelow(n as u64) as i64
    }

    /// `random.randint(a, b)`, both ends included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange(a, b + 1)
    }

    /// Index drawn by `random.choice(seq)` for a sequence of `len` items.
    pub fn choice_index(&mut self, len: usize) -> usize {
        assert!(len > 0, "cannot choose from an empty sequence");
        self.randbelow(len as u64) as usize
    }

    /// `random.choice(seq)`.
    pub fn choice<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.choice_index(items.len())].clone()
    }

    /// `random.shuffle(list)`.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.randbelow(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }

    /// `random.choices(population, weights)[0]`, as an index (k = 1).
    pub fn choices_weighted_index(&mut self, weights: &[f64]) -> usize {
        let mut cumulative = Vec::with_capacity(weights.len());
        let mut total = 0.0;
        for weight in weights {
            total += weight;
            cumulative.push(total);
        }
        let point = self.random() * total;
        // bisect_right(cum_weights, point, 0, n - 1)
        let hi = weights.len() - 1;
        cumulative[..hi].partition_point(|&value| value <= point)
    }

    /// `random.choices(population)[0]` without weights, as an index (k = 1).
    pub fn choices_index(&mut self, len: usize) -> usize {
        (self.random() * len as f64).floor() as usize
    }

    /// `random.gauss(mu, sigma)`, which caches its second value like Python.
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        let z = match self.gauss_next.take() {
            Some(z) => z,
            None => {
                let x2pi = self.random() * std::f64::consts::TAU;
                let g2rad = (-2.0 * (1.0 - self.random()).ln()).sqrt();
                self.gauss_next = Some(x2pi.sin() * g2rad);
                x2pi.cos() * g2rad
            }
        };
        mu + z * sigma
    }
}

fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) { q - 1 } else { q }
}

/// Little-endian bytes of a non-negative integer to 32-bit key words, trimmed like CPython.
fn words_from_le_bytes(bytes: &[u8]) -> Vec<u32> {
    let mut words: Vec<u32> = bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect();
    while words.len() > 1 && *words.last().unwrap() == 0 {
        words.pop();
    }
    if words.is_empty() {
        words.push(0);
    }
    words
}
