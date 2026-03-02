use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    thread,
};

use anyhow::Ok;
use memchr::{memchr, memchr2};

// Include the generated perfect hash table
include!(concat!(env!("OUT_DIR"), "/station_hash.rs"));

const NEWLINE: u8 = b'\n';
const SEMICOLON: u8 = b';';

/// Statistics for a single station.
/// Uses #[repr(C)] for predictable layout.
#[repr(C)]
#[derive(Clone, Copy)]
struct Stat {
    count: u32,
    min: i16,
    max: i16,
    sum: i32,
}

impl Stat {
    const fn default() -> Self {
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
        // Branch prediction works well for min/max
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

/// Thread-local stats using perfect hash indexing.
/// Fixed-size array eliminates HashMap overhead.
#[repr(align(64))] // Prevent false sharing between threads
struct ThreadStats {
    stats: [Stat; STATION_COUNT],
}

impl ThreadStats {
    const fn new() -> Self {
        ThreadStats {
            stats: [Stat::default(); STATION_COUNT],
        }
    }
}

#[inline]
// Branchless temperature parser
fn parse_temperature(t: &[u8]) -> i16 {
    let tlen = t.len();
    // Guarantee to the compiler: all data is at least 3 bytes long, e.g. "0.0"
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
fn chunk_stats(m_chunks: &[u8]) -> (ThreadStats, u32) {
    let mut thread_stats = ThreadStats::new();
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

        // Perfect hash lookup - O(1) array access, no HashMap overhead
        let idx = station_to_index(name).expect("unknown station");
        thread_stats.stats[idx].add(t);
    }

    (thread_stats, line_count)
}

/// Find both semicolon and newline in a single pass using memchr2.
/// Returns (semicolon_position, newline_position) if both found.
#[inline(always)]
fn find_semicolon_newline(data: &[u8]) -> Option<(usize, usize)> {
    // memchr2 finds the first occurrence of either byte
    match memchr2(SEMICOLON, NEWLINE, data) {
        Some(semi_pos) if data[semi_pos] == SEMICOLON => {
            // Found semicolon, now find newline after it
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

    // Global stats accumulator
    let mut global_stats = [Stat::default(); STATION_COUNT];
    let mut line_count = 0u32;

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
        for (thread_stats, c) in rx {
            line_count += c;
            // Merge thread-local stats into global stats
            for (i, stat) in thread_stats.stats.iter().enumerate() {
                if stat.count > 0 {
                    global_stats[i].merge(stat);
                }
            }
        }
    });

    print_stats(&global_stats, line_count)?;

    Ok(())
}

#[inline(always)]
fn print_stats(global_stats: &[Stat; STATION_COUNT], line_count: u32) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let mut writer = BufWriter::new(&mut handle);

    // Build sorted output using BTreeMap
    let mut stats_map = BTreeMap::new();
    for (i, stat) in global_stats.iter().enumerate() {
        if stat.count > 0 {
            stats_map.insert(STATION_NAMES[i], stat);
        }
    }

    write!(writer, "Category: min / avg / max")?;
    for (name, s) in &stats_map {
        writeln!(
            writer,
            "{}: {:.1}  / {:.1} / {:.1}",
            name,
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
