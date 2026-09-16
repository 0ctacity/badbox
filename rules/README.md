# Rule frontends (experimental version 1)

Rules are data, not TypeScript callbacks. The bundled probes use `.badbox` files
written in the Rust-parsed [tiny DSL](../native/tiny-dsl/README.md). That README
is the DSL reference.

Badbox also accepts the earlier YAML format as an optional compatibility
frontend. Both formats are validated and compiled into the same
serialization-independent Rule IR before structural matching is compiled.
This YAML is Badbox's schema, not arbitrary ast-grep YAML.

```yaml
version: 1
id: rust/excessive-clones
language: rust
summary: Function contains more clone call sites than the configured threshold
severity: info
select:
  pattern: $VALUE.clone()
owner:
  nearest: [function_item, closure_expression]
aggregate: count
threshold:
  gt: 1
evidence:
  subject: clone call sites
```

The [Go YAML equivalent](go/excessive-goroutines.yaml) uses the same schema and evaluator,
with `select: { kind: go_statement }` and Go's callable node kinds.
The [Zig probe](zig/excessive-as-casts.yaml) counts `@as` patterns inside Zig
function declarations through that same pipeline.
The [PowerShell probe](powershell/excessive-invoke-expression.yaml) counts
`Invoke-Expression` calls inside PowerShell functions. PowerShell patterns use
uppercase `#NAME` captures so ordinary `$name` and `$NAME` variables remain
literal syntax. Lowercase hash capture names are rejected.

## Contract

- All fields shown above are required. Unknown fields, unknown languages or
  kinds, duplicate rule IDs, unsupported versions/aggregates, and invalid
  thresholds are rejected. IDs use lowercase `namespace/name` with digits and
  hyphens allowed after the first letter of each part.
- `language` is one of `bash`, `c`, `cpp`, `csharp`, `css`, `dart`, `elixir`,
  `go`, `haskell`, `hcl`, `html`, `java`, `javascript`, `json`, `kotlin`, `lua`,
  `markdown`, `nix`, `php`, `powershell`, `python`, `ruby`, `rust`, `scala`,
  `solidity`, `swift`, `tsx`, `typescript`, `yaml`, or `zig`. JSX files use `javascript`.
  The scanner's registry maps extensions to statically linked parsers. A rule
  does not select its own parser binary or execute code.
- `select` contains exactly one `pattern` or `kind`. Patterns currently use
  ast-grep structural pattern semantics, including metavariables. Kind names
  are Tree-sitter grammar names. These are explicit backend dependencies of
  this experimental schema, not a promise of effortless backend replacement.
- `owner.nearest` is a nonempty set of allowed ancestor kinds. Each match is
  assigned to its nearest **strict ancestor** in that set. A match without an
  owner is ignored. Order does not change the result. Rules must include all
  boundaries they intend to respect; the bundled probes include closures and
  anonymous functions, so their contents do not inflate enclosing counts.
- `aggregate: count` counts distinct matched node ranges per owner. Owner
  identity is file plus range, never the function name. Overlapping ranges of
  different nodes count separately (e.g. `x.clone().clone()` counts twice).
- `threshold.gt` is an unsigned 32-bit integer. Only counts strictly greater
  than it produce findings. The API's optional `threshold` scalar overrides
  each loaded rule. Zero-count owners cannot exceed a nonnegative threshold
  and are not returned.
- `severity` is `info`, `warning`, or `error`; it is a label, not an exit policy.
  `message`, when provided, becomes the finding message; otherwise `summary` is
  used. `evidence.subject` labels the count.
  Both must be nonempty. The probe thresholds are illustrative.

## Results

`inspect()` returns `languages`, interned `files` and `rules` tables,
`scannedFiles`, `findingCount`, compact `findings`, `truncated`, and
`diagnostics`.
`languages` lists supported languages discovered, even if no loaded rule targets
one. `scannedFiles` counts error-free files actually analyzed with relevant rules.

`findings` is a flat `Uint32Array` with a record width of five:
`fileId`, `ruleId`, `ownerStart`, `ownerEnd`, and exact `observed` count. File
paths occur once in `files`; IDs, thresholds, severity, messages, language, and
evidence labels occur once in `rules`. By default, at most 10,000 records are
returned. `maxFindings` can lower or raise that budget, and zero provides a
count-only result. `truncated` reports omitted records while `findingCount`
remains exact.

Owner offsets are zero-based UTF-8 bytes and end-exclusive. They are not
JavaScript UTF-16 string indices. Line/column presentation is intentionally not
materialized in the scan result.

Files and returned findings are sorted deterministically; repeated/overlapping input
paths do not duplicate files. Syntax-error files generate a diagnostic and no
findings. Findings are observations, not assertions that code must change.

## Capability check

The fixture suite proves that Rust clone calls, Go launch statements, PowerShell
`Invoke-Expression` calls, and Zig `@as` calls use the same ownership and
evaluator code, exercises Python end to end, and verifies that all 30 bundled
parsers accept representative source and file extensions.
It also changes a test rule's pattern and threshold using YAML alone and verifies
the changed evidence through the native API. No language-specific detector
function is involved.

The cancellation-evidence rule and SQL parsing are intentionally not
part of this slice.
