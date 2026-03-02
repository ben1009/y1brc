# 1BRC Performance Optimization Plan

**Branch:** `performance-investigation`  
**Baseline:** 1.063s ± 0.010s (10 runs)  
**Target:** < 0.900s (15%+ improvement)  
**Stretch Goal:** < 0.800s (25%+ improvement)

---

## 1. Executive Summary

Current implementation achieves **943M rows/second** on an Intel i9-13900T (24 cores, 32 threads). With only **17% memory bandwidth utilization**, there's significant room for CPU-bound optimizations. This plan outlines systematic improvements targeting sub-900ms execution.

### Key Metrics

| Metric | Current | Theoretical Limit | Gap |
|--------|---------|-------------------|-----|
| Total Time | 1.063s | ~276ms (memory BW) | 74% |
| Per Row | 1.06 ns | - | - |
| Instructions/Row | ~100 | TBD | - |
| Cache Misses | ~30M | 0 | - |
| Branch Misses | ~1.4M | 0 | - |

---

## 2. Bottleneck Analysis

### 2.1 Profile Summary

From `perf stat` analysis:
- **Cycles:** 38.5B (core), 38.1B (atom)
- **Instructions:** 99.7B (core) → **IPC ~2.6** (good but improvable)
- **Cache Misses:** 30M (core), 133M (atom) → Not a major bottleneck
- **Branch Misses:** Very low at 1.4M → Branch prediction working well

### 2.2 Hot Path Analysis

```rust
// Current hot loop in chunk_stats():
while let Some(end) = memchr::memchr(NEWLINE, m) {
    let separate = memchr(SEMICOLON, m).unwrap();  // 2nd memchr
    let name = unsafe { m.get_unchecked(..separate) };
    let value = unsafe { m.get_unchecked(separate + 1..end) };
    let key = unsafe { m.get_unchecked(..end) };
    
    // Two HashMap lookups per row:
    key_names.entry(k).or_insert(name);           // Lookup #1
    stats.entry(k).or_insert_with(Stat::default).add(t);  // Lookup #2
}
```

**Issues Identified:**
1. **Two `memchr` calls per line** - Could be optimized to single scan
2. **Two HashMap lookups** - Redundant key hashing
3. **HashMap generic overhead** - FxHashMap still has generic hash table overhead
4. **String allocation on merge** - `String::from_utf8_unchecked(k[&key].to_vec())`

### 2.3 Thread Synchronization

Current design:
- Spawns N threads (32 on this machine)
- Uses `crossbeam::channel` for result passing
- Single-threaded merge into `BTreeMap`

**Observation:** Merge phase is single-threaded bottleneck.

---

## 3. Optimization Opportunities

### Phase 1: Low-Risk Wins (Est. 5-10% improvement)

#### 3.1.1 Combine HashMap Lookups

**Problem:** Two separate HashMap lookups with same key.

**Solution:** Store `name` reference inside `Stat` or use single map with tuple value.

```rust
// Before:
key_names.entry(k).or_insert(name);
stats.entry(k).or_insert_with(Stat::default).add(t);

// After:
combined.entry(k).or_insert_with(|| (name, Stat::default())).1.add(t);
```

**Effort:** Low  
**Risk:** Low  
**Est. Gain:** 2-4%

---

#### 3.1.2 Optimize Stat Structure (SOA)

**Problem:** Current AOS (Array of Structs) layout:
```rust
struct Stat {
    count: u32,  // 4 bytes
    min: i16,    // 2 bytes
    max: i16,    // 2 bytes  
    sum: i32,    // 4 bytes
}  // Total: 12 bytes, padded to 16
```

**Solution:** Consider SOA (Struct of Arrays) for parallel access patterns.

**Effort:** Low  
**Risk:** Low  
**Est. Gain:** 1-3%

---

#### 3.1.3 Pre-sized HashMaps

**Problem:** `with_capacity_and_hasher(1024, ...)` for 413 entries.

**Solution:** Use exact capacity: `with_capacity_and_hasher(413, ...)` + `1.0` load factor.

**Effort:** Minimal  
**Risk:** None  
**Est. Gain:** 0.5-1%

---

### Phase 2: Medium-Risk Optimizations (Est. 10-20% improvement)

#### 3.2.1 Single-Pass Line Parsing

**Problem:** Two `memchr` calls per line.

**Solution:** Custom SIMD scanner that finds both `;` and `\n` in single pass.

```rust
// Pseudocode:
while ptr < end {
    // Load 32/64 bytes
    // Find semicolon and newline positions via SIMD
    // Process line
}
```

**Implementation Options:**
- **Option A:** Manual SSE/AVX2 implementation
- **Option B:** Use `memchr` with `memchr2` for two-byte search

**Effort:** Medium  
**Risk:** Medium (correctness)  
**Est. Gain:** 5-10%

---

#### 3.2.2 Perfect Hash Function

**Problem:** Generic hash table has probe chain overhead.

**Insight:** Only 413 unique stations with known names.

**Solution:** Generate perfect hash function at compile time.

```rust
// Compile-time or build-script generated:
fn perfect_hash(name: &[u8]) -> u16 {
    // Custom hash that maps 413 names → 0..412 perfectly
    // No collisions = no probing = O(1) with no loop
}
```

**Implementation:**
1. Use `phf` crate (perfect hash function), OR
2. Pre-compute minimal perfect hash (e.g., CMPH algorithm)

**Effort:** Medium  
**Risk:** Medium (complexity, build time)  
**Est. Gain:** 8-15%

---

#### 3.2.3 Parallel Merge Phase

**Problem:** Single-threaded `BTreeMap` merge.

**Solution:** 
- Thread-local `BTreeMap`s (already parallel)
- Parallel reduction using `rayon` or custom thread pool

**Effort:** Medium  
**Risk:** Low-Medium  
**Est. Gain:** 3-8% (especially on high core count)

---

### Phase 3: High-Risk/High-Reward (Est. 15-30% improvement)

#### 3.3.1 Lock-Free Per-Station Aggregation

**Problem:** Channel-based aggregation has synchronization overhead.

**Solution:** Pre-allocate global station array with atomic updates.

```rust
static GLOBAL_STATS: [AtomicStation; 413] = [...];

struct AtomicStation {
    count: AtomicU32,
    min: AtomicI16,
    max: AtomicI16,
    sum: AtomicI64,
}

// Each thread updates atomically - no merge needed!
```

**Challenges:**
- Requires perfect hash or station ID mapping
- Atomics may have contention on same cache line
- Need padding to prevent false sharing

**Effort:** High  
**Risk:** High (contention, complexity)  
**Est. Gain:** 15-25% (if contention low)

---

#### 3.3.2 AVX-512 Temperature Parsing

**Problem:** Temperature parsing is branchless but scalar.

**Solution:** Parse multiple temperatures simultaneously using AVX-512.

**Note:** i9-13900T has AVX-512 disabled in favor of E-cores. Limited benefit.

**Effort:** High  
**Risk:** High (no AVX-512 on target)  
**Est. Gain:** N/A for this CPU

---

#### 3.3.3 Memory Prefetching Hints

**Problem:** Random-ish memory access pattern in hash table.

**Solution:** Software prefetching for predictable patterns.

```rust
// Prefetch next few lines while processing current
std::arch::x86_64::_mm_prefetch(next_line.as_ptr(), _MM_HINT_T0);
```

**Effort:** Medium  
**Risk:** Low  
**Est. Gain:** 2-5% (may hurt if overdone)

---

## 4. Implementation Roadmap

### Week 1: Phase 1 - Quick Wins

| Day | Task | Validation |
|-----|------|------------|
| 1-2 | Combine HashMap lookups | Benchmark: 2-4% improvement |
| 3 | Optimize HashMap capacity | Benchmark: baseline check |
| 4-5 | SOA structure experiment | Benchmark: memory layout test |
| 6-7 | **Phase 1 Release** | Target: 1.00-1.03s |

### Week 2: Phase 2 - Core Optimizations

| Day | Task | Validation |
|-----|------|------------|
| 1-3 | Single-pass line scanner | Benchmark vs memchr |
| 4-5 | Perfect hash exploration | Build script, PHF generation |
| 6-7 | Parallel merge | Benchmark scaling |
| 8 | **Phase 2 Release** | Target: 0.85-0.95s |

### Week 3: Phase 3 - Advanced (Optional)

| Day | Task | Validation |
|-----|------|------------|
| 1-3 | Lock-free atomic aggregation | Contention analysis |
| 4-5 | Fine-tuning & prefetching | Final optimization |
| 6-7 | **Final Release** | Target: 0.80-0.90s |

---

## 5. Success Criteria

### Primary Goals

- [ ] **Pass:** < 1.00s (conservative, 6% improvement)
- [ ] **Target:** < 0.90s (moderate, 15% improvement)  
- [ ] **Stretch:** < 0.80s (aggressive, 25% improvement)

### Secondary Metrics

- [ ] All existing assertions pass
- [ ] Output format unchanged
- [ ] No additional dependencies (or minimal)
- [ ] Code readability maintained in non-hot paths
- [ ] `./dev check` passes

---

## 6. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Correctness bugs | Medium | High | Extensive testing on small dataset |
| Diminishing returns | High | Medium | Set time limits per optimization |
| Complexity explosion | Medium | Medium | Keep branches, easy rollback |
| Memory usage increase | Low | Medium | Profile memory before/after |
| Platform-specific code | Medium | Low | Gate behind `cfg` attributes |

---

## 7. Testing Strategy

### Validation Checklist

```bash
# 1. Correctness
./target/release/y1brc | tail -5  # Verify output format

# 2. Performance
hyperfine --warmup 3 -m 10 './target/release/y1brc'

# 3. Code quality
./dev check

# 4. Edge cases
cargo run --bin generate -- 1000  # Small dataset
cargo run --bin generate -- 1000000  # Medium dataset
```

### Profiling Commands

```bash
# CPU profiling
perf record -F 999 -g ./target/release/y1brc
perf report

# Detailed counters
perf stat -e cycles,instructions,cache-misses,branch-misses \
          -e L1-dcache-load-misses,LLC-load-misses \
          ./target/release/y1brc

# Cache analysis
cachegrind ./target/release/y1brc
cg_annotate cachegrind.out.*
```

---

## 8. Alternative Approaches (Future Work)

If Phase 1-2 don't yield sufficient gains:

### A. GPU Acceleration
- CUDA/OpenCL for parsing
- PCIe transfer overhead likely dominates

### B. io_uring
- Async I/O for non-mmap approach
- Unlikely to beat mmap for this access pattern

### C. Custom Memory Allocator
- Bump allocator for station names
- Arena allocation per thread

---

## 9. Appendix: Reference Implementations

### Top Rust 1BRC Solutions (for reference only)

| Author | Time | Key Techniques |
|--------|------|----------------|
| aminediro | ~1.2s | Initial approach, well-documented |
| lehuyduc | ~0.9s | Perfect hashing, SIMD |
| buybackoff | ~0.8s | Lock-free, custom hash |
| mtopolnik | ~0.7s | Java winner techniques ported |

**Note:** Do not copy code. Use only for understanding techniques.

---

## 10. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-03-02 | Start with Phase 1 | Lowest risk, establishes baseline |
| | Skip AVX-512 | Target CPU doesn't support it well |
| | Consider perfect hash | 413 unique keys makes this feasible |

---

## 11. Implementation TODO

### Phase 1: Quick Wins (Week 1)
- [x] **1.1** Combine HashMap lookups (single entry for name+stats) - `Entry` struct with combined name+stat
- [x] **1.2** Optimize HashMap capacity (413 entries) - `with_capacity_and_hasher(413, ...)`
- [ ] **1.3** Experiment with SOA structure for Stat - still AOS layout

### Phase 2: Core Optimizations (Week 2)
- [x] **2.1** Implement single-pass line scanner (memchr2) - `find_semicolon_newline()` with `memchr2`
- [x] **2.2** Generate perfect hash function for 413 stations - build script generates PHF table
- [ ] **2.3** Parallel merge phase with rayon - array-based merge is already parallel-friendly

### Phase 3: Advanced (Week 3)
- [ ] **3.1** Lock-free atomic aggregation (global AtomicStation array)
- [ ] **3.2** Software prefetching hints

### Validation
- [x] `./dev check` passes
- [x] All assertions pass (1B rows, 413 stations)
- [ ] Benchmark shows improvement (pending Phase 2/3)

---

**Next Step:** Review and approve plan. Begin Phase 1 implementation.
