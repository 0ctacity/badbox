# Badbox

Badbox is a deterministic bad-pattern detector for codebases. It reports
suspicious structural evidence and leaves the decision to a developer or coding
agent.

This repository currently contains the initial Bun/TypeScript scaffold. The
first planned vertical slice is `react/multiple-effects`; scanning and rule
execution are intentionally not implemented yet.

## Install

Badbox requires Bun 1.3.14 or newer.

```bash
bun add badbox
```

The package currently exposes the executable TypeScript rule format. The
`badbox scan` command remains scaffolded while the first rule is implemented.

## Development

Install dependencies:

```bash
bun install
```

Inspect the initial CLI:

```bash
bun run scan
```

Type-check the scaffold:

```bash
bun run typecheck
```

## Project layout

- `src/cli.ts` — CLI entry point
- `src/scanner/` — project detection and scanning boundary
- `src/rules/` — Badbox-owned rule and finding contracts
- `src/reporters/` — terminal and JSON output boundaries
- `rules/` — built-in rule packs grouped by scope
- `tests/` — tests and future rule fixtures
