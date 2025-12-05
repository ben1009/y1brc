#![feature(split_array)]

use anyhow::Context;
use arrayref::array_ref;
use fxhash::FxHashMap;
use memchr::memchr;
use std::io::{BufWriter, Write};
use std::{collections::BTreeMap, fs::File};

struct Stat {
    count: i32,
    min: i32,
    max: i32,
    sum: i32,
}

impl Stat {
    fn default() -> Self {
        Stat {
            count: 0,
            min: i32::MAX,
            max: i32::MIN,
            sum: 0,
        }
    }

    fn add(&mut self, value: i32) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
    }
}

#[inline(always)]
fn parse_temp(s: &[u8]) -> i32 {
    let mut i = 0;
    let mut sign = 1;
    if s[0] == b'-' {
        i = 1;
        sign = -1;
    }

    let mut val = 0;
    // Since input is guaranteed to have 1 decimal place, we can simplify parsing
    // Standard format is digits + dot + one digit, e.g. "Palermo", 18.5
    while i < s.len() {
        let b = s[i];
        if b != b'.' {
            val = val * 10 + (b - b'0') as i32;
        }
        i += 1;
    }

    sign * val
}

fn to_key(name: &[u8]) -> u32 {
    // Hash the first and last 2 bytes. since the minimum length is 3, e.g. ("Wau", 27.8),
    let head: [u8; 2] = *array_ref![name, 0, 2];
    let tail: [u8; 2] = *array_ref![name, name.len() - 2, 2];
    let shift = 32usize.saturating_sub(2 * name.len());
    let khead = u16::from_ne_bytes(head) << shift;
    let ktail = u16::from_ne_bytes(tail) >> shift;

    khead as u32 + ktail as u32
}

#[inline(always)]
fn main() -> anyhow::Result<()> {
    let f = File::open("measurements.txt")?;
    // prefetch the whole file into memory
    let m = unsafe { memmap2::MmapOptions::new().populate().map(&f) }?;

    let mut stats = FxHashMap::default();
    let mut key_names = FxHashMap::default();
    let mut line_count = 0;
    let mut m = &m[..];
    // simd to speed up matching
    while let Some(end) = memchr::memchr(b'\n', m) {
        let separate = memchr(b';', m).context("invalid file format")?;
        let name = &m[..separate];
        let value = &m[separate + 1..end];
        m = &m[end + 1..];

        line_count += 1;
        let t = parse_temp(value);
        let k = to_key(name);
        key_names.entry(k).or_insert(name);
        stats.entry(k).or_insert_with(Stat::default).add(t);
    }

    let mut stats_map = BTreeMap::new();
    for (k, v) in &stats {
        stats_map.insert(
            unsafe { String::from_utf8_unchecked(key_names[k].to_vec()) },
            v,
        );
    }
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let mut writer = BufWriter::new(&mut handle);

    write!(writer, "Category: min / avg / max")?;
    for (c, s) in &stats_map {
        writeln!(
            writer,
            "{}: {:.1}  / {:.1} / {:.1}",
            c,
            (s.min as f32) / 10.0,
            (s.sum / s.count) as f32 / 10.0,
            s.max as f32 / 10.0,
        )?;
    }
    writeln!(writer, "\ntotal {} measurements", line_count)?;
    writeln!(
        writer,
        "Category: min / avg / max, total {} categories",
        stats.len()
    )?;

    Ok(())
}
