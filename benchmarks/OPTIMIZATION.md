# Native scanner optimization report

## Outcome

The optimized engine preserves the existing finding fingerprints and frozen
Zova ground truth while reducing repeated scan latency substantially. The main
changes are fused selector dispatch, exact execution sharing, bounded file
parallelism, deferred location construction, and bounded process-local caches.

The comparison uses five retained samples per case on the same Apple M1 host.
The baseline used Bun 1.4.0 and the optimized run used Bun 1.4.2, so ratios are
strong local evidence rather than controlled cross-platform guarantees.

### Real Zova bindings, 100 rules

| Rules | Baseline repeated median | Optimized repeated median | Speedup | Baseline fresh peak | Optimized fresh peak |
|---|---:|---:|---:|---:|---:|
| Rust sparse | 494.96 ms | 11.09 ms | 44.6x | 34.9 MiB | 38.2 MiB |
| Go sparse | 596.38 ms | 12.12 ms | 49.2x | 37.5 MiB | 43.2 MiB |
| Rust dense | 936.36 ms | 193.74 ms | 4.8x | 381.9 MiB | 373.3 MiB |
| Go dense | 1093.66 ms | 188.12 ms | 5.8x | 378.4 MiB | 367.3 MiB |

The 50/100-rule packs repeat ten selector templates with unique rule IDs. Sparse
speedups therefore demonstrate selector sharing and caching, not performance for
100 unrelated selectors. Dense cases remain dominated by constructing and
decoding tens of thousands of findings and hundreds of thousands of ranges.

## Implemented changes

- Optional `inspect({ profile: true })` phase timings and execution/cache counts.
- One AST traversal dispatches all unique selectors for a file by potential node
  kind instead of calling `find_all` separately for every rule.
- Rules with identical language, selector, and nearest-owner specification share
  matching results while retaining independent thresholds and finding metadata.
- File work uses a shared Rayon pool capped at four threads; results are sorted
  afterward to preserve deterministic output.
- Matching stores byte ranges first. Line and Unicode-column positions are built
  only for owner groups that exceed their threshold, using one line index per file.
- Compiled rules are cached by canonical path plus exact file contents, capped at
  256 entries.
- Parsed files are cached by canonical path, language, and exact source contents,
  capped at 256 entries and 16 MiB of retained source text. Changed contents miss
  the cache; stale entries for the same path are removed.
- Benchmark phase profiling runs in a separate worker so its additional scan does
  not contaminate latency or peak-RSS summaries.

## Memory and evidence decision

> Historical note: this section records the decision at the time of the measured
> optimized baseline. The current scanner now transports findings as five-u32
> records with interned file/rule tables. The benchmark harness raises the record
> limit for comparable full-output stress measurements.

Fresh dense peak memory decreased slightly on the real binding roots. Repeated
dense-worker peak memory increased (Rust 570.6 to 887.4 MiB; Go 812.3 to 925.5
MiB) because five much faster scans create very large decoded results before Bun
necessarily reclaims earlier allocations. In the replicated dense stress input,
exact output still reaches hundreds of thousands of findings and millions of
ranges, with multi-GiB peaks.

At the time of this report, Badbox still returned every evidence range. The
dense selectors are benchmark-only grammar-node probes, not real engineering
rules, so that optimization did not silently truncate evidence or change the
public result contract.

## Reproduction

```bash
bun run build:native
bun test
bun run typecheck
bun run benchmark ../zova
```

Raw before/after samples are in
[`results/zova-baseline.json`](results/zova-baseline.json) and
[`results/zova-optimized.json`](results/zova-optimized.json).
