# Badbox Agent Guide

## Purpose

Badbox is a deterministic bad-pattern detector. It reports structural evidence in valid source
code; it does not claim to prove runtime behavior or engineering intent. Findings are observations,
not automatic failures or rewrite instructions.

The product direction is:

```text
discover a questionable pattern -> encode a deterministic rule -> detect it forever
```

Keep the core cheap, reproducible, language-aware, and usable without an LLM. Do not add automatic
rewriting or LLM reasoning to the native engine.

## User Contract

- The CLI is `badbox check`, not `scan`.
- Project rules live under `.badbox/`. No policies or example rules run by default.
- `badbox create <name.badbox>` creates `.badbox/<name.badbox>` with the current `#badbox 1`
  header and an editable example. It must not overwrite an existing file.
- Findings do not make the CLI fail. Invalid rules, invalid input, I/O errors, or syntax diagnostics
  make the check incomplete and produce a nonzero exit.
- DSL is the primary authoring format. YAML is a compatibility frontend. Both compile to the same
  Badbox-owned Rule IR.
- Keep output deterministic: stable discovery, rule loading, findings, and diagnostics ordering are
  part of the contract.

## Architecture

```text
CLI / TypeScript inspect()
        |
        v
native N-API inspect() boundary
        |
        v
DSL or YAML frontend -> Badbox Rule IR
        |
        v
per-language execution groups + shared FinderPlan
        |
        v
file discovery -> parse/cache -> ast-grep structural backend
        |
        v
rule-specific predicates / ownership / relations / aggregation
        |
        v
compact findings + metadata -> TypeScript decoding / terminal reporting
```

TypeScript is intentionally thin. Parsing, structural matching, ownership, relational evaluation,
counting, thresholds, caches, and compact result construction belong in Rust. Do not move native
work into TypeScript merely because the public API is TypeScript.

Badbox owns the public abstractions:

- `native/src/rule_ir.rs` is backend- and serialization-independent.
- `native/src/backend/mod.rs` is the structural backend boundary.
- ast-grep is the current backend, not the product API.
- ast-grep nodes and matcher-specific types must not escape into rule evaluation or the public API.

The finder plan interns identical primary and relational selectors across a language plan. Each
unique selector is dispatched once per relevant AST node during one file traversal. Rules still
apply their own capture predicates, owners, relations, thresholds, severity, and reporting. Preserve
this split when extending execution; do not reintroduce per-rule tree scans.

## Important Paths

- `src/cli.ts` — `check` and `create` commands.
- `src/scanner/index.ts` — public `inspect()` wrapper and compact finding iterator.
- `src/scanner/native.ts` — development addon and platform-package loading.
- `src/scanner/types.ts` — public result and profiling types.
- `src/reporters/terminal.ts` — human-readable CLI output.
- `native/src/lib.rs` — N-API entrypoint, rule loading, planning, discovery, caches, parallel file
  execution, and compact result assembly.
- `native/src/rule_ir.rs` — engine-owned rule model.
- `native/src/frontends/dsl.rs` — tiny-DSL-to-IR lowering and validation.
- `native/src/frontends/yaml.rs` — YAML compatibility lowering.
- `native/src/backend/ast_grep.rs` — parser registry, compiled selectors, shared finder planning,
  structural matching, ownership, and relational fact collection.
- `native/src/evaluator.rs` — language-independent aggregation and finding construction.
- `native/tiny-dsl/` — standalone Rust parser crate and authoritative DSL reference.
- `examples/rules/` and `examples/yaml/` — runnable examples, never defaults.
- `tests/` — Bun integration, CLI, packaging, corpus, and benchmark-contract tests.
- `tests/fixtures/zova/` — frozen, independently labeled real-code corpus.
- `benchmarks/` — reproducible rule-scaling, latency, and memory harness.
- `scripts/ci.ts` — canonical local verification pipeline.
- `.github/workflows/ci.yml` and `.github/workflows/release.yml` — platform CI and npm release.

## Rule Model and Current Capabilities

The executable pipeline is:

```text
find -> where -> nearest owner -> distinct-range count -> threshold -> finding
```

The DSL currently supports:

- source-shaped `code(...)` selectors with declared captures;
- raw `node ...` selectors;
- `group by nearest callable` for Rust, Go, PowerShell, and Zig;
- explicit nearest node kinds for all supported languages;
- capture text operators `==`, `!=`, `in`, `not in`, and Rust-regex `matches`;
- `where match inside any|all { ... }`;
- `where group has any|all { ... }`;
- `where group lacks any|all { ... }`;
- `where match follows any|all { ... }` and `where match precedes any|all { ... }` for Rust, Go,
  PowerShell, and Zig callable groups;
- strictly greater-than count thresholds and scalar parameter overrides.

Ordering is statement-level within the same nearest callable and lexical block; intervening
siblings are allowed. Repeating a single-node capture name in the primary and an ordering selector
requires exact captured source-text equality. Immediate ordering and unsupported target/relation
combinations are rejected explicitly. Never silently ignore a parsed rule condition.

Badbox ships the 28 ast-grep builtin parsers plus statically linked PowerShell and Zig parsers.
Extension detection selects relevant language rules; it is not framework, dependency, or semantic
type detection.

## Correctness Boundaries

- Badbox reports lexical/structural evidence. Do not describe a goroutine as leaked, a transaction
  as non-atomic, or a query plan as inefficient based only on syntax.
- Syntax-error files produce diagnostics and no partial findings.
- Owners are byte ranges. Preserve exact offsets, including equal-looking owners at different
  positions.
- Counts deduplicate selected ranges within an owner.
- Compact findings are five `u32` values: file ID, rule ID, owner start, owner end, and observed
  count. Paths and rule metadata are interned separately.
- Normal output is bounded by `maxFindings`, while exact total finding and observed counts remain
  available.
- File concurrency is bounded. Rule, parse, and compact result caches are process-local,
  content-validated, and memory-bounded.

## Change Guidelines

- Extend the Rule IR before adding backend-specific public behavior.
- A new DSL construct needs parser coverage, DSL lowering, explicit unsupported/error behavior where
  applicable, an end-to-end native test, and an update to `native/tiny-dsl/README.md`.
- Express language differences in rule files, callable mappings, or parser registration. Do not add
  one handwritten detector per language for behavior the generic evaluator can express.
- Share exact selectors through `FinderPlan`; keep rule evaluation independent. Treat more ambitious
  selector-subsumption or query-optimizer work as a measured optimization, not a correctness shortcut.
- Preserve YAML compatibility unless removal is explicitly requested.
- Do not add bundled default rules. Examples belong under `examples/`.
- Do not add unbounded AST caches, result buffers, worker counts, or cross-file state.
- Do not claim a performance improvement from selector counts alone. Use the benchmark harness for
  wall-clock or memory claims and preserve its correctness fingerprints.

## Development Workflow

Requirements: Bun, Rust/Cargo, cargo-nextest, and a C compiler for Tree-sitter grammars.

```bash
bun install
bun run build:native
bun run ci
```

`bun run ci` is the canonical full check. It runs Rust formatting, Clippy, both cargo-nextest suites,
the native release build, TypeScript checking, and all Bun tests.

Useful focused commands:

```bash
cargo nextest run --manifest-path native/Cargo.toml
cargo nextest run --manifest-path native/tiny-dsl/Cargo.toml
bun test tests/scanner.test.ts
bun run typecheck
```

After any Rust change, rebuild with `bun run build:native` before running Bun tests; otherwise the
tests may exercise a stale `.node` addon. Use `cargo nextest run`, not ordinary `cargo test`, for
normal Rust test execution.

For performance work:

```bash
bun run build:native
bun run benchmark                 # frozen corpus only
bun run benchmark ../zova         # clean adjacent Zova checkout
```

Keep benchmark setup outside measured work, compare the same workload/build mode, and report raw
scope and uncertainty. Never commit generated `native/build/`, Cargo targets, `release/`, or local
benchmark result files.

## Native Packaging and Release

The root npm package loads one of five optional platform packages:

- `badbox-darwin-arm64`
- `badbox-darwin-x64`
- `badbox-linux-arm64-gnu`
- `badbox-linux-x64-gnu`
- `badbox-windows-x64`

All six npm packages and both Rust manifests must use the same release version. The GitHub release
workflow builds the five native addons, packages and smoke-tests tarballs, publishes platform
packages first, and publishes `badbox` last through npm trusted publishing. Do not publish, bump
versions, create tags, or modify release artifacts unless the user explicitly requests it.
