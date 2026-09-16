# Zova correctness and scaling baseline (pre-optimization)

This report records the original serial, uncached engine. See
[OPTIMIZATION.md](OPTIMIZATION.md) for the bounded-parallel, fused-execution,
cached implementation and its before/after measurements.

## Outcome

The two product probes match the independently source-labeled corpus exactly.
The completed benchmark contains **48 cases, 480 measured scans, and 48 warmups**,
with per-rule output/evidence agreement in every measured batch. No parser
diagnostics or missing input files were accepted.

This supports correctness for the selected examples and establishes a baseline.
It does **not** establish that the engine is fast enough for an agent workflow,
that all repository findings are useful, or that 100 distinct engineering rules
have been tested. No latency or memory acceptance budget was set. No engine
optimization, Zig support, or cancellation-rule behavior was added.

## Correctness evidence

- Frozen Zova revision: `88197842e5a7b2d2477fc4e2c073c1f518817ba1`.
- 30 full-function excerpts; 46 labeled owners, including nested closures.
- 18 positive owners and 28 zero-match owners; 19 expected sites in total.
- Exact rule, owner name/kind/range, count, and evidence-range agreement at
  threshold zero. At threshold one, only the Go concurrency test is emitted.
- A concatenation test also preserves the distinct Rust owners when duplicate
  method names share one file.
- All 30 frozen excerpts were separately compared with the recorded source
  line intervals. Only the documented Go package header is added.

The labels were established by the implementing agent reading complete source
excerpts, with explicit text/line annotations converted to offsets; not by
exporting expected findings from Badbox. This is not an independent human audit.
Selected zero-match functions check for false positives, and enumerated sites
check for false negatives within these excerpts. This is not an exhaustive or
statistical repository-wide precision/recall estimate. The real corpus has at
most two sites per owner; higher counts remain covered by synthetic fixtures.

See [corpus provenance and labels](../tests/fixtures/zova/README.md).

## Environment and method

Recorded 2026-08-30T17:35:00.041Z. Apple M1, 8 logical CPUs,
16 GiB RAM, darwin/arm64,
Darwin 24.6.0, Bun 1.4.0, rustc 1.98.0 (88d9e12ae 2026-08-18).
Native addon built in release mode. This is a desktop baseline, without CPU
affinity, frequency control, or isolation from other desktop activity.

Five fresh processes and five measured calls after one warmup per case. All
samples are retained as min/median/max and raw arrays; no p95 claims from five
samples. Fresh means a new process, not cold filesystem caches. Repeated calls
are not incremental: rules are reloaded/recompiled and files are reparsed.

Each language is measured separately: all 1/10/50/100 rules are relevant.
There are ten varied templates per density/language; larger packs repeat the
mix with unique IDs. These are benchmark-only selectors. Thresholds are zero
to retain evidence. A one-rule case uses the first selector, so 10→50→100 is the
balanced-mix comparison; 1→100 changes the mix too.

The engine parses each file once **per scan**, then runs a separate matching
pass per relevant rule. Shared evaluator code is not fused matching work.
Untimed individual-rule references verify batching consistency; they are not
independent ground truth for the broader benchmark selectors.

### Inputs

| Input | Language | Files | Source bytes |
|---|---|---:|---:|
| corpus | rust | 25 | 11591 |
| corpus | go | 5 | 5358 |
| zova-bindings | rust | 11 | 233884 |
| zova-replicated-8x | rust | 88 | 1871072 |
| zova-bindings | go | 26 | 231144 |
| zova-replicated-8x | go | 208 | 1849152 |

Zova bindings exclude bundled source mirrors and build outputs. Go includes
binding tests; the Rust roots are library source only. The 8× input is synthetic
file replication, not a large independent repository or additional syntax
coverage. Full input, source, dependency-lock and binary hashes are in the raw
report.

## Rule scaling

Median repeated API latency in **milliseconds**. It includes native loading,
discovery, parsing, matching, counting, JSON serialization, and JS decoding.

| Input | Language | Pack | 1 rule | 10 rules | 50 rules | 100 rules |
|---|---|---|---:|---:|---:|---:|
| corpus | rust | sparse | 3.35 | 6.56 | 21.19 | 40.87 |
| corpus | rust | dense | 3.20 | 7.79 | 30.97 | 66.79 |
| corpus | go | sparse | 1.59 | 3.30 | 11.13 | 28.85 |
| corpus | go | dense | 2.18 | 4.42 | 14.63 | 29.37 |
| zova-bindings | rust | sparse | 24.62 | 66.84 | 256.77 | 494.96 |
| zova-bindings | rust | dense | 28.98 | 106.35 | 463.06 | 936.36 |
| zova-replicated-8x | rust | sparse | 192.77 | 530.60 | 2036.30 | 4065.15 |
| zova-replicated-8x | rust | dense | 235.14 | 924.43 | 3727.23 | 7449.12 |
| zova-bindings | go | sparse | 38.85 | 90.14 | 314.04 | 596.38 |
| zova-bindings | go | dense | 47.12 | 149.50 | 573.93 | 1093.66 |
| zova-replicated-8x | go | sparse | 316.98 | 746.64 | 2524.64 | 4680.52 |
| zova-replicated-8x | go | dense | 447.84 | 1203.00 | 4732.48 | 9085.09 |

For the balanced mixtures, adding rules and replicating input generally
increases cost roughly proportionally in these measurements. This is an
observation about this workload, not an asymptotic guarantee or a speed target
being met.

## Memory and output volume at 100 rules

| Input | Language | Pack | Fresh peak median (MiB) | Repeated-worker peak (MiB) | Findings | Evidence ranges |
|---|---|---|---:|---:|---:|---:|
| zova-bindings | rust | sparse | 34.9 | 50.5 | 830 | 850 |
| zova-bindings | rust | dense | 381.9 | 570.6 | 46960 | 210590 |
| zova-replicated-8x | rust | sparse | 67.1 | 90.5 | 6640 | 6800 |
| zova-replicated-8x | rust | dense | 1660.3 | 2435.9 | 375680 | 1684720 |
| zova-bindings | go | sparse | 37.5 | 57.4 | 1360 | 2290 |
| zova-bindings | go | dense | 378.4 | 812.3 | 34600 | 289260 |
| zova-replicated-8x | go | sparse | 84.1 | 162.3 | 10880 | 18320 |
| zova-replicated-8x | go | dense | 1995.5 | 2340.7 | 276800 | 2314080 |

These are whole-process RSS measurements, including Bun, native allocations,
result decoding and correctness validation. Fresh is the median of five
one-call worker peaks; repeated is one worker's peak over a warmup and five
calls, without forced GC. They are not isolated native-heap or per-call memory
measurements. A higher repeated peak does not prove a leak.

Dense selectors emit much more evidence and have markedly higher memory
requirements. The benchmark does not isolate how much cost comes from matching,
ownership/grouping, result construction/serialization, validation, or GC.
The largest cases need several seconds and multiple GiB; “Rust underneath” is
not sufficient evidence of low latency or low memory use.

## Fresh versus repeated calls on the real binding roots (100 rules)

| Language | Pack | Fresh API median (ms) | Fresh process median (ms) | Repeated API median (ms) | Repeated MiB/s |
|---|---|---:|---:|---:|---:|
| rust | sparse | 503.10 | 517.34 | 494.96 | 0.45 |
| rust | dense | 905.04 | 1004.40 | 936.36 | 0.24 |
| go | sparse | 590.44 | 605.64 | 596.38 | 0.37 |
| go | dense | 1162.17 | 1264.92 | 1093.66 | 0.20 |

Process time includes startup, validation, and exit; it is not a pure CLI
measurement. Repeated calls do not consistently improve over fresh calls, and
there is no implemented incremental/cache benefit to claim.

## Run integrity and limitations

The first matrix attempt aborted in the largest repeated Go case: a fixed
60-second worker deadline covered **six calls**, not one. Its raw sample arrays
were not saved by the original end-only writer; console summaries from that
attempt are **not** used in the tables above.

The harness now budgets 60 seconds times the number of warmup/measured calls
and checkpoints completed cases. A regression test deliberately interrupts
a run and checks that its completed rows and failed status remain available.
The entire matrix was rerun; [the canonical JSON](results/zova-baseline.json)
has `status: "complete"`. Every sample of that completed run is retained.

There is no claim of cross-platform performance, real large-repository coverage,
100 distinct rules, parser robustness beyond existing tests, or production
readiness. The labels assess structural correctness, not whether cloning or
goroutine spawning should be refactored.

## Verification and reproduction

```bash
bun run build:native
bun test
bun run typecheck
bun run benchmark ../zova
```

See [harness documentation](README.md) for output paths and measurement details.
The benchmark did not change Zova. The scanner and the two product rule files
were unchanged during this milestone.
