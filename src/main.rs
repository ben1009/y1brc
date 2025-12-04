#![feature(slice_split_once)]

use std::io::{BufWriter, Write};
use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    fs::File,
    io::Read,
};
struct Stat {
    count: usize,
    min: f32,
    max: f32,
    sum: f32,
}

impl Stat {
    fn new() -> Self {
        Stat {
            count: 1,
            min: f32::MAX,
            max: f32::MIN,
            sum: 0.0,
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut f = File::open("measurements.txt")?;
    let mut buf = vec![];
    f.read_to_end(&mut buf)?;
    let mut stats = HashMap::new();
    let mut line_count = 0;
    for line in buf.split(|&b| b == b'\n') {
        if let Some((c, t)) = line.split_once(|&c| c == b';') {
            line_count += 1;
            let t = String::from_utf8_lossy(t).parse::<f32>().unwrap();
            match stats.entry(c) {
                Entry::Vacant(e) => {
                    let mut s = Stat::new();
                    s.max = s.max.max(t);
                    s.min = s.min.min(t);
                    s.sum += t;
                    e.insert(s);
                }
                Entry::Occupied(mut e) => {
                    let s = e.get_mut();
                    s.count += 1;
                    s.max = s.max.max(t);
                    s.min = s.min.min(t);
                    s.sum += t;
                }
            }
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
            String::from_utf8_lossy(c),
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
