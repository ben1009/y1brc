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
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    let mut stats = HashMap::new();
    let mut line_count = 0;
    for line in buf.lines() {
        line_count += 1;
        let (c, t) = line.split_once(';').unwrap();
        let t = t.parse::<f32>().unwrap();
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

    let stats = BTreeMap::from_iter(stats);
    for (c, s) in &stats {
        println!(
            "{c}: {:.1}  / {:.1} / {:.1}",
            s.min,
            s.sum / s.count as f32,
            s.max
        );
    }
    println!("\ntotal {} measurements", line_count);
    println!(
        "Category: min / avg / max, total {} categories",
        stats.len()
    );

    Ok(())
}
