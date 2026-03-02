use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufWriter, Write},
    ops::BitXor,
    thread,
};

use anyhow::Ok;
use fxhash::{FxBuildHasher, FxHashMap};
use memchr::{memchr, memchr2};

const NEWLINE: u8 = b'\n';
const SEMICOLON: u8 = b';';

// Fast hash key computation (first 4 bytes + length)
const K: u64 = 0x517cc1b727220a95;

#[inline(always)]
fn add_to_hash(x: u64, i: u64) -> u64 {
    x.rotate_left(5).bitxor(i).wrapping_mul(K)
}

#[inline(always)]
fn to_key(name: &[u8]) -> u64 {
    // All station names have at least 3 bytes, most have 4+
    let mut ret = 0;
    ret = add_to_hash(ret, name[0] as u64);
    ret = add_to_hash(ret, name[1] as u64);
    ret = add_to_hash(ret, name[2] as u64);
    // Handle short names (e.g., "Jos", "Wau")
    let b3 = if name.len() > 3 { name[3] } else { 0 };
    ret = add_to_hash(ret, b3 as u64);
    add_to_hash(ret, name.len() as u64)
}

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

    #[inline(always)]
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

    #[inline(always)]
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

/// Combined entry for station name and statistics.
/// Using a single HashMap entry eliminates duplicate hash computation and lookup.
struct Entry<'a> {
    name: &'a [u8],
    stat: Stat,
}

#[inline(always)]
fn parse_temperature(t: &[u8]) -> i16 {
    let tlen = t.len();
    // Guarantee to the compiler, all data is at least 3 bytes long, e.g. "0.0"
    unsafe { std::hint::assert_unchecked(tlen >= 3) };
    // Deal with sign
    let is_neg = std::hint::select_unpredictable(t[0] == b'-', true, false);
    // If neg, !is_neg = 0, 0*2-1 = -1, else 1*2-1 = 1
    let sign = i16::from(!is_neg) * 2 - 1;
    // Skip if neg
    let skip = usize::from(is_neg);
    // Deal with 12.3 or 1.2, double digit before dot or not
    let has_dd = std::hint::select_unpredictable(tlen - skip == 4, true, false);
    let mul = i16::from(has_dd) * 100;
    // Highest digit if have
    let t1 = mul * i16::from(t[skip] - b'0');
    // Middle digit
    let t2 = 10 * i16::from(t[tlen - 3] - b'0');
    // Lowest digit
    let t3 = i16::from(t[tlen - 1] - b'0');

    sign * (t1 + t2 + t3)
}

#[inline(always)]
fn chunk_stats(m_chunks: &[u8]) -> (FxHashMap<u64, Entry<'_>>, u32) {
    // Exact capacity for 413 stations with no reallocation
    let mut entries: FxHashMap<u64, Entry<'_>> =
        HashMap::with_capacity_and_hasher(413, FxBuildHasher::default());
    let mut line_count = 0u32;
    let mut m = m_chunks;

    // Single-pass SIMD-accelerated scanning using memchr2
    loop {
        let Some((semi_pos, nl_pos)) = find_semicolon_newline(m) else {
            break;
        };
        let name = unsafe { m.get_unchecked(..semi_pos) };
        let value = unsafe { m.get_unchecked(semi_pos + 1..nl_pos) };
        m = unsafe { m.get_unchecked(nl_pos + 1..) };

        line_count += 1;
        let t = parse_temperature(value);
        let k = to_key(name);

        // Single HashMap lookup
        match entries.get_mut(&k) {
            Some(entry) => entry.stat.add(t),
            None => {
                let _ = entries.insert(
                    k,
                    Entry {
                        name,
                        stat: Stat::default(),
                    },
                );
                entries.get_mut(&k).unwrap().stat.add(t);
            }
        }
    }

    (entries, line_count)
}

/// Find both semicolon and newline in a single pass using memchr2.
#[inline(always)]
fn find_semicolon_newline(data: &[u8]) -> Option<(usize, usize)> {
    match memchr2(SEMICOLON, NEWLINE, data) {
        Some(semi_pos) if data[semi_pos] == SEMICOLON => {
            let after_semi = semi_pos + 1;
            memchr(NEWLINE, &data[after_semi..]).map(|nl| (semi_pos, after_semi + nl))
        }
        _ => None,
    }
}

#[inline(always)]
fn main() -> anyhow::Result<()> {
    let f = File::open("measurements.txt")?;
    // Prefetch the whole file into memory, enable huge page
    let m = unsafe {
        memmap2::MmapOptions::new()
            .populate()
            .huge(Some(21))
            .map(&f)
    }?;

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
        for (entries, c) in rx {
            line_count += c;
            for (_key, entry) in entries {
                stats_map
                    .entry(unsafe { String::from_utf8_unchecked(entry.name.to_vec()) })
                    .or_insert_with(Stat::default)
                    .merge(&entry.stat);
            }
        }
    });

    print_stats(&stats_map, line_count)?;

    Ok(())
}

#[inline(always)]
fn print_stats(stats_map: &BTreeMap<String, Stat>, line_count: u32) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let mut writer = BufWriter::new(&mut handle);

    write!(writer, "Category: min / avg / max")?;
    for (c, s) in stats_map {
        writeln!(
            writer,
            "{}: {:.1}  / {:.1} / {:.1}",
            c,
            (s.min as f32) / 10.0,
            (s.sum / s.count as i32) as f32 / 10.0,
            s.max as f32 / 10.0,
        )?;
    }
    assert_eq!(line_count, 1000000000);
    assert_eq!(stats_map.len(), 413);
    writeln!(writer, "\ntotal {} measurements", line_count)?;
    writeln!(
        writer,
        "Category: min / avg / max, total {} categories",
        stats_map.len()
    )?;

    Ok(())
}
