#![feature(slice_split_once)]

use anyhow::Context;
use fxhash::FxHashMap;
use memchr::memchr;
use std::io::{BufWriter, Write};
use std::{collections::BTreeMap, fs::File};

struct Stat {
    count: usize,
    min: f32,
    max: f32,
    sum: f32,
}

impl Stat {
    fn default() -> Self {
        Stat {
            count: 0,
            min: f32::MAX,
            max: f32::MIN,
            sum: 0.0,
        }
    }

    fn add(&mut self, value: f32) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
    }
}

#[inline(always)]
fn main() -> anyhow::Result<()> {
    let f = File::open("measurements.txt")?;
    // prefetch the whole file into memory
    let m = unsafe { memmap2::MmapOptions::new().populate().map(&f) }?;

    let mut stats = FxHashMap::default();
    let mut line_count = 0;
    let mut m = &m[..];
    while let Some(end) = memchr::memchr(b'\n', m) {
        let separate = memchr(b';', m).context("invalid file format")?;
        let name = &m[..separate];
        let value = &m[separate + 1..end];
        m = &m[end + 1..];

        line_count += 1;
        let t = unsafe {
            String::from_utf8_unchecked(value.to_vec())
                .parse::<f32>()
                .context("parse to f32 fail")?
        };
        stats.entry(name).or_insert_with(Stat::default).add(t);
    }

    for line in m.split(|&b| b == b'\n') {
        if let Some((c, t)) = line.split_once(|&c| c == b';') {
            line_count += 1;
            let t = unsafe {
                String::from_utf8_unchecked(t.to_vec())
                    .parse::<f32>()
                    .context("parse to f32 fail")?
            };
            stats.entry(c).or_insert_with(Stat::default).add(t);
        }
    }

    let stats = BTreeMap::from_iter(stats);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let mut writer = BufWriter::new(&mut handle);

    write!(writer, "Category: min / avg / max")?;
    for (c, s) in &stats {
        writeln!(
            writer,
            "{}: {:.1}  / {:.1} / {:.1}",
            unsafe { String::from_utf8_unchecked(c.to_vec()) },
            s.min,
            s.sum / s.count as f32,
            s.max
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
