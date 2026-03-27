use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufWriter, Write},
    ops::BitXor,
    thread,
};

use anyhow::Context;
use fxhash::{FxBuildHasher, FxHashMap};
use memchr::memchr;

const NEWLINE: u8 = b'\n';
const SEMICOLON: u8 = b';';

struct Stat {
    count: u32,
    min: i16,
    max: i16,
    sum: i32,
}

impl Stat {
    fn default() -> Self {
        Stat {
            count: 0,
            min: i16::MAX,
            max: i16::MIN,
            sum: 0,
        }
    }

    fn add(&mut self, value: i16) {
        self.count += 1;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.sum += value as i32;
    }

    fn merge(&mut self, other: &Stat) {
        self.count += other.count;
        if other.min < self.min {
            self.min = other.min;
        }
        if other.max > self.max {
            self.max = other.max;
        }
        self.sum += other.sum;
    }
}

#[inline(always)]
fn parse_temperature(t: &[u8]) -> i16 {
    let tlen = t.len();
    unsafe { std::hint::assert_unchecked(tlen >= 3) };
    let is_neg = std::hint::select_unpredictable(t[0] == b'-', true, false);
    let sign = i16::from(!is_neg) * 2 - 1;
    let skip = usize::from(is_neg);
    let has_dd = std::hint::select_unpredictable(tlen - skip == 4, true, false);
    let mul = i16::from(has_dd) * 100;
    let t1 = mul * i16::from(t[skip] - b'0');
    let t2 = 10 * i16::from(t[tlen - 3] - b'0');
    let t3 = i16::from(t[tlen - 1] - b'0');

    sign * (t1 + t2 + t3)
}

const K: u64 = 0x517cc1b727220a95;
#[inline(always)]
fn add_to_hash(x: u64, i: u64) -> u64 {
    x.rotate_left(5).bitxor(i).wrapping_mul(K)
}

#[inline(always)]
fn to_key(name: &[u8]) -> u64 {
    unsafe { std::hint::assert_unchecked(name.len() >= 4) };
    let mut ret = 0;
    ret = add_to_hash(ret, name[0] as u64);
    ret = add_to_hash(ret, name[1] as u64);
    ret = add_to_hash(ret, name[2] as u64);
    ret = add_to_hash(ret, name[3] as u64);
    add_to_hash(ret, name.len() as u64)
}

#[inline(always)]
fn chunk_stats(m_chunks: &[u8]) -> (FxHashMap<u64, Stat>, FxHashMap<u64, &[u8]>, u32) {
    let mut stats = HashMap::with_capacity_and_hasher(1024, FxBuildHasher::default());
    let mut key_names = HashMap::with_capacity_and_hasher(1024, FxBuildHasher::default());
    let mut line_count = 0;
    let mut m = m_chunks;

    while let Some(end) = memchr::memchr(NEWLINE, m) {
        let separate = memchr(SEMICOLON, m).context("invalid file format").unwrap();
        let name = unsafe { m.get_unchecked(..separate) };
        let value = unsafe { m.get_unchecked(separate + 1..end) };
        let key = unsafe { m.get_unchecked(..end) };
        m = unsafe { m.get_unchecked(end + 1..) };

        line_count += 1;
        let t = parse_temperature(value);
        let k = to_key(key);
        key_names.entry(k).or_insert(name);
        stats.entry(k).or_insert_with(Stat::default).add(t);
    }

    (stats, key_names, line_count)
}

#[cfg_attr(feature = "hotpath", hotpath::main)]
fn main() -> anyhow::Result<()> {
    let f = File::open("measurements-small.txt")?;
    let m = unsafe { memmap2::MmapOptions::new().populate().map(&f) }?;

    let mut stats_map = BTreeMap::new();
    let mut line_count = 0;
    thread::scope(|s| {
        let num_threads = std::thread::available_parallelism().unwrap().get();
        let chunk_size = m.len() / num_threads;
        let mut start = 0;
        let (tx, rx) = crossbeam::channel::bounded(num_threads);
        while start < m.len() {
            let mut end = m.len().min(start + chunk_size);
            if end < m.len() {
                let e = memchr(NEWLINE, unsafe { m.get_unchecked(end..) }).unwrap();
                end += e + 1;
            }
            let m_chunks = unsafe { m.get_unchecked(start..end) };
            start = end;
            let tx = tx.clone();
            s.spawn(move || tx.send(chunk_stats(m_chunks)));
        }

        drop(tx);
        for (s, k, c) in rx {
            line_count += c;
            for (key, stat) in s {
                stats_map
                    .entry(unsafe { String::from_utf8_unchecked(k[&key].to_vec()) })
                    .or_insert_with(Stat::default)
                    .merge(&stat);
            }
        }
    });

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
            (s.sum / s.count as i32) as f32 / 10.0,
            s.max as f32 / 10.0,
        )?;
    }
    writeln!(writer, "\ntotal {} measurements", line_count)?;
    writeln!(
        writer,
        "Category: min / avg / max, total {} categories",
        stats_map.len()
    )?;

    Ok(())
}
