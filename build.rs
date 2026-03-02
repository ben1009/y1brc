use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Write},
    ops::BitXor,
    path::Path,
};

fn main() {
    // Read station names from the small measurements file
    let stations = read_stations("measurements-small.txt");
    assert_eq!(stations.len(), 413, "Expected 413 unique stations");

    // Generate perfect hash function using fxhash
    let phf = generate_perfect_hash(&stations);

    // Write the generated code
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("station_hash.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(
        f,
        "// Auto-generated perfect hash for 413 weather stations\n"
    )
    .unwrap();

    // Generate station index lookup
    writeln!(f, "pub const STATION_COUNT: usize = 413;\n").unwrap();

    // Generate station name table for index -> name mapping
    writeln!(f, "pub static STATION_NAMES: [&str; 413] = [").unwrap();
    for (i, name) in stations.iter().enumerate() {
        writeln!(f, "    \"{}\", // {}", escape_string(name), i).unwrap();
    }
    writeln!(f, "];\n").unwrap();

    // Generate the perfect hash function
    writeln!(f, "const HASH_MODULUS: u64 = {};\n", phf.modulus).unwrap();

    writeln!(f, "#[inline(always)]").unwrap();
    writeln!(
        f,
        "pub fn station_to_index(name: &[u8]) -> Option<usize> {{"
    )
    .unwrap();
    writeln!(f, "    let h = fxhash(name) % HASH_MODULUS;").unwrap();
    writeln!(f, "    Some(PHF_TABLE[h as usize])").unwrap();
    writeln!(f, "}}\n").unwrap();

    // Write PHF lookup table
    writeln!(f, "#[allow(clippy::large_const_arrays)]").unwrap();
    writeln!(f, "const PHF_TABLE: [usize; {}] = [", phf.modulus).unwrap();
    for i in 0..phf.modulus {
        let idx = phf.hash_to_index.get(&i).copied().unwrap_or(0);
        writeln!(f, "    {},", idx).unwrap();
    }
    writeln!(f, "];\n").unwrap();

    // Write fxhash function (using ^ operator instead of bitxor for const fn compatibility)
    writeln!(f, "#[inline(always)]").unwrap();
    writeln!(f, "const fn fxhash(bytes: &[u8]) -> u64 {{").unwrap();
    writeln!(f, "    const K: u64 = 0x517cc1b727220a95;").unwrap();
    writeln!(f, "    let mut hash: u64 = 0;").unwrap();
    writeln!(f, "    let mut i = 0;").unwrap();
    writeln!(f, "    while i < bytes.len() {{").unwrap();
    writeln!(
        f,
        "        hash = (hash.rotate_left(5) ^ (bytes[i] as u64)).wrapping_mul(K);"
    )
    .unwrap();
    writeln!(f, "        i += 1;").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f, "    hash").unwrap();
    writeln!(f, "}}").unwrap();

    println!("cargo:rerun-if-changed=measurements-small.txt");
    println!("cargo:rerun-if-changed=build.rs");
}

fn read_stations(path: &str) -> Vec<String> {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);
    let mut stations: Vec<String> = reader
        .lines()
        .map(|l| l.unwrap().split(';').next().unwrap().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    stations.sort();
    stations
}

struct PerfectHash {
    modulus: u64,
    hash_to_index: HashMap<u64, usize>,
}

fn generate_perfect_hash(stations: &[String]) -> PerfectHash {
    const K: u64 = 0x517cc1b727220a95;

    fn fxhash(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0;
        for &b in bytes {
            hash = hash.rotate_left(5).bitxor(b as u64).wrapping_mul(K);
        }
        hash
    }

    // Try increasing modulus until we find a perfect hash
    let mut modulus = stations.len() as u64;
    loop {
        let mut hash_to_index: HashMap<u64, usize> = HashMap::new();
        let mut collision = false;

        for (idx, station) in stations.iter().enumerate() {
            let h = fxhash(station.as_bytes()) % modulus;
            if hash_to_index.contains_key(&h) {
                collision = true;
                break;
            }
            hash_to_index.insert(h, idx);
        }

        if !collision {
            // Convert to direct lookup table format
            let mut table: HashMap<u64, usize> = HashMap::new();
            for (idx, station) in stations.iter().enumerate() {
                let h = fxhash(station.as_bytes()) % modulus;
                table.insert(h, idx);
            }
            return PerfectHash {
                modulus,
                hash_to_index: table,
            };
        }

        modulus += 1;
        if modulus > 10000 {
            panic!("Could not find perfect hash");
        }
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
