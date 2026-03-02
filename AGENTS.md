# AGENTS.md - 1 Billion Row Challenge (1brc)

This document provides essential information for AI coding agents working on this project.

## Project Overview

This is a Rust implementation of the **1 Billion Row Challenge (1brc)** - a performance-oriented programming challenge that processes a large text file containing 1 billion temperature measurements from 413 weather stations worldwide.

The goal is to calculate min, mean, and max temperatures per station as fast as possible. The current implementation achieves approximately **1 second** processing time on an Intel i9-13900T.

**Key Performance Features:**
- Memory-mapped file I/O with 2MB huge pages
- Multi-threaded parallel processing using scoped threads
- Branchless temperature parsing
- Custom fxhash-based hashing
- SIMD-accelerated line scanning (via `memchr`)

## Technology Stack

- **Language**: Rust (Edition 2024)
- **Toolchain**: Nightly (`nightly-2025-11-30`)
- **Build System**: Cargo with cargo-make
- **Task Runner**: `./dev` script (wrapper for cargo-make)

## Project Structure

```
.
├── Cargo.toml              # Package configuration
├── Cargo.lock              # Dependency lock file
├── Makefile.toml           # cargo-make task definitions
├── rust-toolchain.toml     # Rust toolchain specification
├── rustfmt.toml            # Code formatting rules
├── .typos.toml             # Spell check configuration
├── .cargo/config.toml      # Cargo build configuration (target flags)
├── dev                     # Development script wrapper
├── measurements.txt        # Input data file (~13GB, 1 billion rows)
├── measurements-small.txt  # Smaller test data
├── src/
│   ├── main.rs             # Main processing program
│   └── bin/
│       └── generate.rs     # Data generation utility
└── target/                 # Build output
```

## Build Configuration

### Release Profile (`Cargo.toml`)

```toml
[profile.release]
debug = true
lto = "fat"           # Full Link Time Optimization
codegen-units = 1     # Single codegen unit for max optimization
panic = "abort"       # Abort on panic (smaller/faster code)
```

### Target Flags (`.cargo/config.toml`)

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-Ctarget-cpu=native", "-Cforce-frame-pointers=yes"]
```

## Development Commands

All development tasks are managed through the `./dev` script, which wraps cargo-make.

### Setup

```bash
# The dev script auto-installs cargo-binstall and cargo-make on first run
./dev --help          # List all available tasks
```

### Code Quality Checks

```bash
./dev check           # Run all checks (format, deps, clippy, typos)
./dev check-fmt       # Check code formatting
./dev check-clippy    # Run clippy lints (-D warnings)
./dev check-typos     # Check for typos
./dev check-machete   # Check for unused dependencies
./dev check-dep-sort  # Check dependency sorting in Cargo.toml
./dev check-hakari    # Check workspace-hack management
```

### Testing

```bash
./dev test            # Run unit tests with cargo-nextest
./dev test-cov        # Run tests with coverage report (HTML)
```

**Note:** Tests require `cargo-nextest` and `cargo-llvm-cov` which are auto-installed.

### Performance Measurement

```bash
./dev time            # Build release and measure execution time
./dev perf            # Run with Linux perf stat for detailed metrics
```

### Data Generation

```bash
cargo run --bin generate -- <N>   # Generate N measurements to measurements.txt
# Example: cargo run --bin generate -- 1000000
```

## Dependencies

### Runtime Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `anyhow` | 1.0.79 | Error handling |
| `crossbeam` | 0.8.4 | Cross-platform channel for thread communication |
| `fxhash` | 0.2.1 | Fast non-cryptographic hashing |
| `memchr` | 2 | SIMD-accelerated byte searching |
| `memmap2` | 0.9 | Memory-mapped file I/O |
| `rand` | 0.9.2 | Random number generation (for data generator) |
| `rand_distr` | 0.5.1 | Statistical distributions (for data generator) |

### Development Tools (Auto-installed)

- `cargo-nextest` 0.9.96 - Test runner
- `cargo-llvm-cov` 0.6.15 - Code coverage
- `typos-cli` 1.38.1 - Spell checking
- `cargo-hakari` 0.9.34 - Workspace-hack management
- `cargo-machete` 0.8.0 - Dead dependency detection
- `cargo-sort` 1.0.9 - Dependency sorting

## Code Style Guidelines

The project uses `rustfmt.toml` with Rust 2024 style edition:

- **Comment width**: 120 characters
- **Tab spaces**: 4
- **Import granularity**: Crate-level
- **Import grouping**: Std → External → Crate
- **Features enabled**:
  - Format code in doc comments
  - Normalize comments and doc attributes
  - Reorder imports and impl items
  - Wrap comments

### Formatting Commands

```bash
cargo fmt --all       # Format all code
cargo fmt --all -- --check   # Check formatting without changes
```

## Input/Output Format

### Input (`measurements.txt`)

Each line: `<station_name>;<temperature>`

```
Hamburg;12.0
Bulawayo;8.9
Palembang;38.8
...
```

- Temperatures have exactly 1 decimal place
- Station names may contain Unicode characters
- 1 billion rows total
- 413 unique stations

### Output

```
Category: min / avg / max
Abha: 1.0  / 18.0 / 35.0
Abidjan: 15.0  / 26.0 / 38.0
...
total 1000000000 measurements
Category: min / avg / max, total 413 categories
```

## Key Implementation Details

### Main Algorithm (`src/main.rs`)

1. **Memory Mapping**: Maps `measurements.txt` with 2MB huge pages for faster access
2. **Chunking**: Divides file into chunks at line boundaries for each CPU thread
3. **Per-thread Processing**: 
   - Uses `memchr` to find newlines (SIMD acceleration)
   - Parses temperature with branchless logic
   - Aggregates stats into `FxHashMap`
4. **Merging**: Main thread merges results from all workers into a `BTreeMap` (for sorted output)
5. **Output**: Prints formatted results with assertions for validation

### Temperature Parsing

Uses a custom branchless parser that leverages `std::hint::select_unpredictable`:

```rust
fn parse_temperature(t: &[u8]) -> i16
```

Handles:
- Optional negative sign
- One or two digits before decimal
- Exactly one digit after decimal

### Custom Hashing

Uses fxhash algorithm for fast string hashing:

```rust
fn to_key(name: &[u8]) -> u64
```

Hashes first 4 bytes + length for speed (collisions unlikely with this dataset).

## Testing Strategy

The project has minimal unit tests (currently commented out in CI). Validation is primarily done through:

1. **Assertion checks** in `main.rs`:
   - `assert_eq!(line_count, 1000000000)` - verifies all rows processed
   - `assert_eq!(stats_map.len(), 413)` - verifies all 413 stations found

2. **Benchmarking** via `./dev time` and `./dev perf`

3. **External verification** using `hyperfine`:
   ```bash
   hyperfine --warmup 3 ./target/release/y1brc
   ```

## Security Considerations

- Uses `unsafe` blocks for performance-critical sections:
  - Memory mapping operations
  - SIMD-accelerated string parsing with `memchr`
  - `String::from_utf8_unchecked` for station names (input is known ASCII)
  - `std::hint::assert_unchecked` for branch hints
  
- These are safe given the controlled input format but should not be used with untrusted input.

- The `panic = "abort"` setting means panics terminate immediately without unwinding.

## Common Tasks

### Run the main program

```bash
cargo run --release
# Or after building:
./target/release/y1brc
```

### Generate test data

```bash
cargo run --bin generate -- 1000000  # 1 million rows
```

### Clean build artifacts

```bash
cargo clean
# Or: ./dev clean
```

### Check all quality gates (before commit)

```bash
./dev check
```

## Notes for AI Agents

- **Always run `./dev check`** before committing changes
- The project prioritizes **performance over readability** in hot paths
- Use `unsafe` sparingly and only with clear safety comments
- Maintain compatibility with the expected output format
- The nightly toolchain is required for `std::hint::select_unpredictable`
- Large test files (`measurements.txt`) are gitignored but exist locally
