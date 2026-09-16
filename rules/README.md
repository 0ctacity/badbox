# Counting probe format (experimental version 1)

Rules are data, not TypeScript callbacks. Rust loads one rule per YAML file.
YAML is an input frontend: it is validated and compiled into Badbox's
serialization-independent rule IR before structural matching is compiled. This
is not arbitrary ast-grep YAML and not a finalized DSL.

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

The [Go probe](go/excessive-goroutines.yaml) uses the same schema and evaluator,
with `select: { kind: go_statement }` and Go's callable node kinds.

## Contract

- All fields shown above are required. Unknown fields, unknown languages or
  kinds, duplicate rule IDs, unsupported versions/aggregates, and invalid
  thresholds are rejected. IDs use lowercase `namespace/name` with digits and
  hyphens allowed after the first letter of each part.
- `language` is `rust` or `go`. The scanner's language registry maps extensions
  to parsers. A rule does not select its own parser binary or execute code.
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
  `summary` becomes the finding message; `evidence.subject` labels the count.
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

The fixture suite proves that Rust clone calls and Go launch statements use the
same ownership and evaluator code. It also changes a test rule's pattern and
threshold using YAML alone and verifies the changed evidence through the native
API. No language-specific detector function is involved.

The cancellation-evidence rule and Zig probes are intentionally not part of this
first slice.
