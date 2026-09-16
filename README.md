# Badbox

Badbox is a deterministic bad-pattern detector for codebases. It reports
suspicious structural evidence and leaves the decision to a developer or coding
agent.

This development checkout implements four capability probes:
`rust/excessive-clones`, `go/excessive-goroutines`,
`powershell/excessive-invoke-expression`, and `zig/excessive-as-casts`. All four
use the same native ownership, counting, threshold, and evidence pipeline. YAML
is currently an input frontend that compiles to a Badbox-owned rule IR;
TypeScript does not perform matching or aggregation.

The published npm `0.0.1` is still the earlier scaffold. The native scanner is
experimental, source-build only, and has not been released.

## Try the development checkout

Requires Bun 1.3.14+, Rust/Cargo, and a C compiler for Tree-sitter grammars.
The native build has been verified on macOS arm64; other platforms are unverified.

```bash
bun install
bun run build:native
bun run scan tests/fixtures/counts
```

Scan explicit source roots, for example with Zova checked out beside Badbox:

```bash
bun run scan ../zova/bindings/rust/zova/src ../zova/bindings/go
```

`badbox scan [path ...]` defaults to the working directory. It prints findings
and a scan summary. Findings do not cause a nonzero exit; invalid input, I/O
failures, or syntax diagnostics do. There are no CLI flags in this slice.

## Rule files and API

The two [YAML probes](rules/README.md) define syntax selection, nearest owner
kinds, counting, thresholds, and evidence labels. The current threshold of
`> 1` is a probe setting, not a universal engineering recommendation.

```ts
import { inspect } from "badbox/scanner";

const result = await inspect({
  paths: ["./src"],
  rulePaths: ["./rules/my-rule.yaml"], // omit to run the bundled probes
  threshold: 2, // optional override of every loaded rule's threshold
  profile: true, // optional phase timings and execution/cache counters
  maxFindings: 1_000, // optional; defaults to 10,000
});

console.log(result.findings);
console.log(result.performance);
```

In this checkout, import from `./src/scanner/index.ts` instead. The legacy
`defineRule()` export remains available, but its TypeScript callbacks are not
executed by this scanner.

One asynchronous native call compiles YAML into the Badbox rule IR, discovers
files for 30 statically bundled languages, compiles selectors through the
active structural backend, parses relevant files, assigns nearest owners,
counts, and returns bounded five-integer records in a native `Uint32Array`.
File paths and rule metadata are
interned once in tables. The current backend uses ast-grep-core behind a Rust
trait; no ast-grep nodes cross into evaluation or the public API. TypeScript
only invokes the engine and parses the small metadata document.

Within a process, compiled rules and parsed files use bounded, content-validated
caches. Files are scanned in batches of at most four native workers. Completed
batch results are merged before the next batch. Rules with the
same language, selector, and owner boundary share one execution, and all unique
selectors for a language are dispatched during one AST traversal per file. Final
findings are sorted after parallel work, preserving deterministic output.

## Current boundaries

- Counts are lexical syntax sites, not runtime executions or costs. No type
  resolution, macro expansion, conditional-compilation filtering, or leak proof.
- Nested Rust functions/closures and Go functions/methods/literals are separate
  owners. Findings contain file/rule indexes, owner byte ranges, and exact counts;
  thresholds and presentation metadata live in the rule table.
- Discovery respects ignore files and hidden paths, skips common build/vendor
  directories, and does not follow nested symlinks. Explicit roots opt into those
  roots. Bundled source copies and tests are not automatically deduplicated or
  separated: choose source roots deliberately.
- The native backend supports all 28 parsers built into ast-grep 0.45.1: Bash,
  C, C++, C#, CSS, Dart, Elixir, Go, Haskell, HCL, HTML, Java, JavaScript/JSX,
  JSON, Kotlin, Lua, Markdown, Nix, PHP, Python, Ruby, Rust, Scala, Solidity,
  Swift, TSX, TypeScript, and YAML, plus Badbox's statically linked PowerShell
  and Zig parsers.
  File extensions select relevant rules; this is not framework or dependency
  detection. The bundled probes target Rust, Go, PowerShell, and Zig.
- Syntax-error files yield diagnostics and no partial findings. Unreadable files
  fail the scan. Zero-count owners are not returned.
- File parallelism is bounded at four workers. Caches are process-local and
  bounded; they do not survive a CLI process or avoid reading files to validate
  their contents. There is no filesystem watcher or cross-process cache.
- Findings are five `u32` values each and are bounded by default while exact
  total and observed counts are retained. Raising `maxFindings` increases the
  native buffer linearly at 20 bytes per returned finding.
- SQL parsing, cancellation evidence, additional aggregates, and
  prebuilt native npm packages are not implemented. Rule format version `1` is
  experimental.

## Development checks

```bash
bun run build:native
bun test
bun run typecheck
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --manifest-path native/Cargo.toml --locked --all-targets -- -D warnings
```

Rebuild after changing Rust. Bun tests exercise the actual native addon, including
YAML-only changes, cross-language ownership, threshold boundaries, owner byte
ranges, duplicate owners, ignores, and diagnostics. No Zova checkout is required
for tests.

The [frozen Zova corpus](tests/fixtures/zova/README.md) adds independently
source-labeled owner/count/range checks. For reproducible rule-count, latency,
and memory measurements, see the [benchmark harness](benchmarks/README.md) and
the [optimization report](benchmarks/OPTIMIZATION.md).

## Project layout

- `src/cli.ts` — CLI entry point
- `src/scanner/` — thin native invocation and result types
- `native/src/rule_ir.rs` — backend- and serialization-independent rule model
- `native/src/frontends/` — YAML-to-IR compilation
- `native/src/backend/` — structural contract and ast-grep implementation
- `native/src/evaluator.rs` — language-independent aggregation and evidence
- `src/rules/` — legacy callback and base finding contracts
- `src/reporters/` — terminal output and reserved JSON reporter interface
- `rules/` — YAML capability probes and format documentation
- `tests/` — integration tests and syntax fixtures
