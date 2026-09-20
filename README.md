# Badbox

Badbox is a deterministic bad-pattern detector for codebases. It reports
suspicious structural evidence and leaves the decision to a developer or coding
agent.

The repository includes four example rules:
`rust/excessive-clones`, `go/excessive-goroutines`,
`powershell/excessive-invoke-expression`, and `zig/excessive-as-casts`. All four
use the same native ownership, counting, threshold, and evidence pipeline. The
examples use Badbox's Rust-parsed [tiny DSL](native/tiny-dsl/README.md).
The DSL and the optional YAML frontend both compile to a Badbox-owned rule IR;
TypeScript does not perform matching or aggregation.

The npm `0.1.0` release installs a prebuilt native engine through one of five
optional platform packages:
`badbox-darwin-arm64`, `badbox-darwin-x64`, `badbox-linux-arm64-gnu`,
`badbox-linux-x64-gnu`, or `badbox-windows-x64`.

## Install and check

Each project owns its rules under `.badbox/`. Badbox recursively loads every
`.badbox`, `.yaml`, and `.yml` rule file there. It never runs example or built-in
policy automatically.

```bash
bun add --dev badbox
bunx badbox create rust-rules.badbox
bunx badbox check
```

`create` writes `.badbox/rust-rules.badbox` with the current `#badbox` format
header and an editable Rust example. Users replace or remove the example and
write their own rules. Badbox refuses to overwrite an existing file.

Source paths are optional and default to the current project:

```bash
bunx badbox check src packages
```

The [example rules](examples/rules) are runnable DSL references, not defaults.
YAML equivalents live separately under [examples/yaml](examples/yaml), so each
directory can be loaded without duplicate rule IDs.

## Try the development checkout

Requires Bun 1.3.14+, Rust/Cargo, and a C compiler for Tree-sitter grammars.
The native build has been verified on macOS arm64; other platforms are unverified.

```bash
bun install
bun run build:native
bun run src/cli.ts create rust-rules.badbox
bun run check tests/fixtures/counts
```

To check another project, run Badbox from that project so its `.badbox/`
directory supplies the rules:

```bash
cd ../zova
bunx badbox check bindings/rust/zova/src bindings/go
```

`badbox check [path ...]` prints findings and a check summary. Findings do not
cause a nonzero exit; a missing or invalid `.badbox/` rule pack, invalid input,
I/O failures, or syntax diagnostics do.

## Rule files and API

The [tiny DSL](native/tiny-dsl/README.md) defines syntax selection, nearest
ownership, counting, thresholds, structured reports, and external test fixture
declarations. The current threshold of `> 1` is a probe setting, not a universal
engineering recommendation. YAML remains supported as a compatibility frontend.

```ts
import { inspect } from "badbox/checker";

const result = await inspect({
  paths: ["./src"],
  rulePaths: ["./.badbox"], // required file or recursive directory pack
  parameters: { "rust/excessive-clones.limit": 4 },
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

One asynchronous native call compiles DSL or YAML rules into the Badbox rule IR, discovers
files for 30 statically bundled languages, compiles selectors through the
active structural backend, parses relevant files, assigns nearest owners,
counts, and returns bounded five-integer records in a native `Uint32Array`.
File paths and rule metadata are
interned once in tables. The current backend uses ast-grep-core behind a Rust
trait; no ast-grep nodes cross into evaluation or the public API. TypeScript
only invokes the engine and parses the small metadata document.

Within a process, compiled rules and parsed files use bounded, content-validated
caches. Files are checked in batches of at most four native workers. Completed
batch results are merged before the next batch. Rules with identical matching and
owner conditions share one execution group, and all unique primary and relational
selectors are interned into a shared finder plan. Each unique
selector is dispatched during one AST traversal per file; rule-specific predicates,
ownership, aggregation, thresholds, and reporting remain independent. Final
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
  detection. The repository examples target Rust, Go, PowerShell, and Zig.
- Syntax-error files yield diagnostics and no partial findings. Unreadable files
  fail the check. Zero-count owners are not returned.
- File parallelism is bounded at four workers. Caches are process-local and
  bounded; they do not survive a CLI process or avoid reading files to validate
  their contents. There is no filesystem watcher or cross-process cache.
- Findings are five `u32` values each and are bounded by default while exact
  total and observed counts are retained. Raising `maxFindings` increases the
  native buffer linearly at 20 bytes per returned finding.
- Relational DSL conditions are parsed but rejected explicitly until native
  relational evaluation exists. SQL parsing, cancellation evidence, additional aggregates, and
  Rule format version `1` is experimental.

## Releasing to npm

Reserve the five platform package names once from an npm-authenticated local
shell. Inspect all five package payloads first, then publish version `0.0.0`
under the non-default `bootstrap` tag:

```bash
bun run bootstrap:platform-packages --dry-run
bun run bootstrap:platform-packages --publish
```

The bootstrap packages deliberately contain no native binaries. Their only
purpose is to create the npm packages so trusted publishing can be configured.

Set the same new version in `package.json`, both native `Cargo.toml` files, and
all five `optionalDependencies`, then commit and push it. Run the **Release npm
packages** workflow with that version. It builds and tests each native target,
creates and smoke-tests the tarballs, publishes the five platform packages, and
publishes `badbox` last. Trusted publishing for all six packages authenticates
the workflow through GitHub OIDC; no npm token is required.

The workflow refuses to publish if the root `badbox` version already exists.
The first native release uses version `0.1.0` consistently across all six npm
packages and both Rust manifests.

## Development checks

```bash
bun run build:native
bun test
bun run typecheck
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --manifest-path native/Cargo.toml --locked --all-targets -- -D warnings
cargo nextest run --manifest-path native/Cargo.toml
cargo nextest run --manifest-path native/tiny-dsl/Cargo.toml
```

Rebuild after changing Rust. Bun tests exercise the actual native addon, including
rule-only changes, cross-language ownership, threshold boundaries, owner byte
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
- `native/tiny-dsl/` — Rust parser, syntax model, tests, and DSL reference
- `native/src/frontends/` — DSL/YAML-to-IR compilation
- `native/src/backend/` — structural contract and ast-grep implementation
- `native/src/evaluator.rs` — language-independent aggregation and evidence
- `src/rules/` — legacy callback and base finding contracts
- `src/reporters/` — terminal output and reserved JSON reporter interface
- `examples/rules/` — runnable DSL examples, never loaded automatically
- `examples/yaml/` — equivalent YAML compatibility examples
- `tests/` — integration tests and syntax fixtures
