# Rule-scaling baseline

This harness measures the current engine; it does not assert a latency target.
See the [pre-optimization report](REPORT.md), [optimization report](OPTIMIZATION.md),
[baseline samples](results/zova-baseline.json), and
[optimized samples](results/zova-optimized.json).

## Run

From the Badbox checkout, with Zova beside it:

```bash
bun run build:native
bun run benchmark ../zova
```

Without a Zova argument, only the frozen corpus is measured. An optional second
argument names a new JSON output file; existing files are never overwritten:

```bash
bun run benchmark ../zova /tmp/badbox-new-baseline.json
```

The default output is a timestamped file in `benchmarks/results/`. Building is
not timed. Rebuild after native edits; the report records source and binary
hashes, toolchain, OS, CPU, RAM, input hashes, and working-tree status. The Zova
roots must be clean. The harness copies inputs into a temporary snapshot and
does not modify Zova. Temporary input/rule files are removed after the run.
Completed cases are checkpointed between workers. Only `status: "complete"`
reports represent a finished matrix; failed runs retain completed cases and an
error. A worker's deadline is 60 seconds times its warmup/measured-call count,
not a 60-second limit shared by six calls.

## Workloads

- **Corpus:** 30 frozen Zova excerpts, split into Rust and Go inputs.
- **Zova bindings:** tracked `.rs` under `bindings/rust/zova/src` and `.go` under
  `bindings/go`. Go includes tests; Rust's separate tests directory is excluded.
  Bundled native/Python copies, vendor directories, and generated build outputs
  are outside these roots.
- **Replicated:** eight copies of those binding files. This tests input-volume
  scaling but is not an independent large repository or new syntax diversity.

Each language is measured separately, so 1/10/50/100 means that many **relevant**
rules. Sparse packs use specific calls/statements; dense packs use broader node
kinds. There are ten distinct selector templates per language/density; 50 and
100 repeat that mix with unique IDs. These are benchmark-only rules, not 100
distinct engineering checks. Count thresholds are zero to retain findings rather
than measure an empty-output shortcut. The harness raises `maxFindings` to the
maximum unsigned 32-bit integer. It therefore remains a full-output stress test;
ordinary `inspect()` calls use a bounded default.

One-rule cases use the first selector (clone or go-statement for sparse, call
expression for dense). Comparisons from 10 to 50 to 100 preserve the selector
mix; a 100/1 ratio also changes that mix. The run order is 10/1/100/50, not an
always-increasing load. No other benchmark worker runs concurrently.

## Measurements and correctness guards

Every case has five fresh worker processes and another worker with one warmup
followed by five measured calls. No samples are discarded; min/median/max and
all raw samples are retained. Five samples are not enough to claim tail latency.

- **API time:** elapsed `inspect()` time including native rule loading,
  discovery, parsing, matching, ownership/counting, compact record construction,
  metadata JSON serialization and JS decoding. Input/rule generation and result
  validation are outside this timer.
- **Fresh process wall time:** launch through exit, including runtime startup
  and correctness validation. It is not a pure CLI timing.
- **Peak RSS:** whole worker lifetime, including Bun, native allocations and
  validation. Repeated-worker peak spans warmup plus all five calls; it is not
  isolated per-scan memory. No forced GC. This does not diagnose leaks.
- **Throughput:** input files/s and MiB/s from median repeated API time, with
  input bytes, finding counts, and compact finding-buffer bytes recorded alongside.
- **Phase profile:** a separate fresh worker performs one profiled scan per case.
  Its rule loading, discovery, reading, parsing, matching, ownership,
  aggregation, output construction, result merging, serialization, JS decoding,
  cache, selector, and worker counters are stored in `performance`. Per-file
  phase durations are summed when work is parallel, so they are diagnostic work
  totals rather than end-to-end wall time.

Fresh means a new process, **not a cold filesystem cache**. Inputs were just
copied/read and reference scans ran first. Repeated calls reuse the process and
its bounded compiled-rule and parsed-file caches. Source files are still read so
cache entries can be validated against exact contents.

Before timing, each of the ten selector templates runs individually on each
input. Every measured batched run must match those per-rule finding/evidence
fingerprints exactly and check every input file with no diagnostics. This catches
batching omissions and nondeterminism; it is not independent semantic ground
truth for benchmark-only selectors. The separately labeled corpus supplies
ground truth for the two product probes.

The memory API is [Bun subprocess resourceUsage](https://bun.sh/reference/bun/Subprocess/resourceUsage),
whose `maxRSS` is bytes and CPU time is microseconds. The harness normalizes the
runtime's bigint CPU counters for JSON serialization.
