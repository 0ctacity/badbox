# tiny-dsl

`tiny-dsl` parses Badbox's versioned rule language into a Rust syntax model.

The crate does not scan source code and does not depend on ast-grep. Badbox lowers its output into
the engine-owned Rule IR, then the selected structural backend compiles and executes that IR.

```text
.badbox source -> tiny-dsl parser -> Badbox Rule IR -> ast-grep backend -> findings
```

The crate is currently an internal path dependency of `badbox-native`; it is not published
separately.

## Complete example

```text
badbox 1

rule rust/excessive-clones for rust {
  summary "Excessive clone calls within one callable"

  param limit = 4

  find code(value) `value.clone()`
  group by nearest callable
  when count > limit

  report {
    severity info
    message "Callable contains more clone call sites than the configured limit"
    evidence "clone call sites"
  }
}

test rust/excessive-clones "reports excessive clones" {
  input "fixtures/excessive.rs"
  expect {
    findings 1
    count 5
  }
}

test rust/excessive-clones "accepts limited clones" {
  input "fixtures/acceptable.rs"
  expect findings 0
}
```

One file may contain multiple `rule` and `test` blocks. A directory passed as a Badbox rule path
is a pack: Badbox recursively loads its `.badbox`, `.yaml`, and `.yml` files in deterministic path
order. Rule IDs must remain unique across the complete scan.

## File version

Every file starts with:

```text
badbox 1
```

The version belongs to the DSL syntax, not to an individual rule. Unsupported versions are errors.

## Rules

```text
rule namespace/name for language {
  ...
}
```

IDs use lowercase letters, digits, and hyphens and contain exactly one `/`. For example,
`rust/excessive-clones` is valid.

A rule requires one `summary`, `find`, `group`, `when`, and `report` entry. `param` and `where`
entries may repeat. Other duplicate or unknown entries are errors.

### Parameters

Parameters are rule-local scalar values whose type is inferred:

```text
param limit = 4
param enabled = true
param label = "clone call sites"
```

The count condition currently accepts an unsigned integer literal or an integer parameter:

```text
when count > 4
when count > limit
```

Callers override a parameter with its qualified key:

```ts
await inspect({
  paths: ["src"],
  parameters: { "rust/excessive-clones.limit": 8 },
});
```

Unknown parameters and values of the wrong inferred type are errors. Parameter values cannot replace
patterns, node kinds, or other structural rule syntax. Effective values participate in Badbox's
native result-cache fingerprint.

### Structural selection

Use source-shaped code when the pattern is easiest to recognize as code:

```text
find code(value) `value.clone()`
find code(callee, args...) `callee(args)`
```

Names declared inside `code(...)` are captures. The declaration is what makes a name a hole;
undeclared identifiers in the backtick pattern remain literal. `name...` declares a multiple-node
capture. Badbox lowers declared names to the backend's capture notation before compilation.

Use an explicit syntax-node kind when the grammar already has a precise node:

```text
find node go_statement
```

### Ownership

Findings are grouped under their nearest owning scope:

```text
group by nearest callable
```

`callable` is intentionally semantic. Badbox currently maps it for Rust, Go, PowerShell, and Zig:

| Language | Node kinds |
| --- | --- |
| Rust | `function_item`, `closure_expression` |
| Go | `function_declaration`, `method_declaration`, `func_literal` |
| PowerShell | `function_statement` |
| Zig | `function_declaration` |

For every supported parser language, or whenever exact control is preferable, use raw node kinds:

```text
group by nearest node(function_item, closure_expression)
```

### Conditions

Version 1 aggregates distinct selected ranges per owner and compares the exact count:

```text
when count > limit
```

The comparison is strictly greater-than.

### Reporting

```text
report {
  severity warning
  message "Callable contains too many clone call sites"
  evidence "clone call sites"
}
```

Severity is `info`, `warning`, or `error`. It labels evidence; a finding does not automatically make
the Badbox process fail. Version 1 messages are plain strings and do not interpolate captures.

## Relational conditions

The syntax model preserves capture predicates and structural evidence clauses:

```text
where text(method) in ["unwrap", "expect"]

where group lacks any {
  code(ctx) `ctx.cancel()`
  node select_expression
}
```

Text operators are `==`, `!=`, `in`, `not in`, and `matches`. Relational targets are `match` and
`group`; relations are `has`, `lacks`, `inside`, `follows`, and `precedes`; the quantifier is `any`
or `all`. Repeated `where` clauses mean all clauses must hold.

The parser represents these clauses, but the current Badbox evaluator does not execute them yet.
The integration returns an explicit error for such a rule instead of ignoring its conditions.

## Tests and fixtures

Tests live in the DSL and source code lives in external fixture files:

```text
test rust/excessive-clones "reports excessive clones" {
  input "fixtures/excessive.rs"
  expect {
    findings 1
    count 5
  }
}
```

`findings` is required. `count` optionally checks the observed count of the expected finding.
Badbox parses these declarations today; a dedicated rule-test runner is not part of the current CLI.

## Comments and literals

`#` starts a line comment outside strings and code patterns. Strings use double quotes and support
`\"`, `\\`, `\n`, and `\t`. Structural code uses backticks; write `\`` for a literal backtick.

## Rust API

```rust
let document = tiny_dsl::parse(source)?;
for rule in document.rules {
    println!("{}", rule.id);
}
```

`parse` returns a `Document` or a `ParseError` containing a source line and column. The public syntax
types contain owned data so Badbox can cache or lower a parsed document without retaining the input
buffer.

## Current boundaries

- The crate parses rules and fixtures; it does not read files or execute tests.
- Badbox executes the current `find -> group -> count -> threshold -> report` subset.
- Relational `where` clauses are represented but rejected by the scanner until native evaluation is
  implemented.
- `callable` has semantic mappings for Rust, Go, PowerShell, and Zig. Other languages use explicit
  `node(...)` ownership for now.
- PowerShell code patterns support single-node declared captures; multiple captures are rejected.
- YAML remains an optional Badbox frontend and compiles to the same Rule IR.
