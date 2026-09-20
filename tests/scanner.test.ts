import { afterAll, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect, iterateFindings, type CheckResult, type FindingRecord } from "../src/scanner/index.ts";

const fixtureRoot = fileURLToPath(new URL("./fixtures/counts", import.meta.url));
const exampleRulePack = fileURLToPath(new URL("../examples/rules", import.meta.url));
const rustRulePath = fileURLToPath(new URL("../examples/yaml/rust/excessive-clones.yaml", import.meta.url));
const goRulePath = fileURLToPath(new URL("../examples/yaml/go/excessive-goroutines.yaml", import.meta.url));
const powershellRulePath = fileURLToPath(new URL(
  "../examples/yaml/powershell/excessive-invoke-expression.yaml", import.meta.url,
));
const zigRulePath = fileURLToPath(new URL("../examples/yaml/zig/excessive-as-casts.yaml", import.meta.url));
const temporaryRoots: string[] = [];

const records = (result: CheckResult) => [...iterateFindings(result)];

async function ownerText(finding: FindingRecord): Promise<string> {
  const bytes = new Uint8Array(await Bun.file(finding.file).arrayBuffer());
  return new TextDecoder().decode(bytes.slice(finding.ownerStart, finding.ownerEnd));
}

async function temporaryRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "badbox-test-"));
  temporaryRoots.push(root);
  return root;
}

afterAll(async () => {
  await Promise.all(temporaryRoots.map((root) => rm(root, { recursive: true, force: true })));
});

describe("native counting capability probes", () => {
  test("loads a multi-rule tiny DSL pack through the native engine", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.rs");
    const rules = join(root, "rules");
    await mkdir(rules);
    await Bun.write(
      source,
      "fn example() { a.clone(); b.clone(); a.unwrap(); b.unwrap(); }",
    );
    await Bun.write(join(rules, "rust.badbox"), `#badbox 1

rule rust/excessive-clones for rust {
  summary "Function contains multiple clone calls"
  param limit = 1
  find code(value) \`value.clone()\`
  group by nearest callable
  when count > limit
  report {
    severity info
    message "Callable contains too many clone call sites"
    evidence "clone call sites"
  }
}

rule rust/excessive-unwraps for rust {
  summary "Function contains multiple unwrap calls"
  find code(value) \`value.unwrap()\`
  group by nearest node(function_item, closure_expression)
  when count > 1
  report {
    severity warning
    message "Callable contains too many unwrap call sites"
    evidence "unwrap call sites"
  }
}

test rust/excessive-clones "reports excessive clones" {
  input "fixtures/excessive.rs"
  expect findings 1
}
`);

    const result = await inspect({ paths: [source], rulePaths: [rules] });

    expect(result.diagnostics).toEqual([]);
    expect(records(result).map((finding) => [
      finding.rule.id,
      finding.rule.message,
      finding.observed,
    ])).toEqual([
      ["rust/excessive-clones", "Callable contains too many clone call sites", 2],
      ["rust/excessive-unwraps", "Callable contains too many unwrap call sites", 2],
    ]);

    const overridden = await inspect({
      paths: [source],
      rulePaths: [rules],
      parameters: { "rust/excessive-clones.limit": 2 },
    });
    expect(records(overridden).map((finding) => finding.rule.id)).toEqual([
      "rust/excessive-unwraps",
    ]);
    expect(overridden.rules[0]?.threshold).toBe(2);

    await expect(inspect({
      paths: [source],
      rulePaths: [rules],
      parameters: { "rust/excessive-clones.unknown": 2 },
    })).rejects.toThrow("unknown parameter");
    await expect(inspect({
      paths: [source],
      rulePaths: [rules],
      parameters: { "rust/excessive-clones.limit": "two" },
    })).rejects.toThrow("must be an integer");
  });

  test("Rust, Go, PowerShell, and Zig share compact nearest-owner counting", async () => {
    const result = await inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], threshold: 1 });
    const findings = records(result);
    expect(result.languages).toEqual(["go", "powershell", "rust", "zig"]);
    expect(result.checkedFiles).toBe(4);
    expect(result.diagnostics).toEqual([]);
    expect(result.findingCount).toBe(9);
    expect(findings).toHaveLength(9);
    expect(findings.filter((finding) => finding.rule.language === "rust")).toHaveLength(4);
    expect(findings.filter((finding) => finding.rule.language === "go")).toHaveLength(3);
    expect(findings.filter((finding) => finding.rule.language === "powershell")).toHaveLength(1);
    expect(findings.filter((finding) => finding.rule.language === "zig")).toHaveLength(1);
    for (const finding of findings) {
      expect(finding.observed).toBe(2);
      expect(finding.ownerStart).toBeLessThan(finding.ownerEnd);
      expect((await ownerText(finding)).length).toBeGreaterThan(0);
    }
  });

  test("a built-in ast-grep language outside the original Rust and Go pair scans end to end", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.py");
    const rule = join(root, "python.yaml");
    await Bun.write(source, [
      "def example():",
      '    print("first")',
      '    print("second")',
      "",
    ].join("\n"));
    await Bun.write(rule, [
      "version: 1",
      "id: python/excessive-prints",
      "language: python",
      "summary: Function contains multiple print calls",
      "severity: info",
      "select:",
      "  pattern: print($VALUE)",
      "owner:",
      "  nearest: [function_definition]",
      "aggregate: count",
      "threshold:",
      "  gt: 1",
      "evidence:",
      "  subject: print call sites",
      "",
    ].join("\n"));

    const result = await inspect({ paths: [source], rulePaths: [rule] });
    expect(result.languages).toEqual(["python"]);
    expect(result.checkedFiles).toBe(1);
    expect(result.diagnostics).toEqual([]);
    expect(records(result).map((finding) => [finding.rule.language, finding.observed]))
      .toEqual([["python", 2]]);
  });

  test("Zig uses the generic nearest-owner counting pipeline", async () => {
    const source = join(fixtureRoot, "sample.zig");
    const result = await inspect({ paths: [source], rulePaths: [zigRulePath] });
    const repeated = await inspect({ paths: [source], rulePaths: [zigRulePath] });
    const findings = records(result);

    expect(result.languages).toEqual(["zig"]);
    expect(result.checkedFiles).toBe(1);
    expect(result.diagnostics).toEqual([]);
    expect(result.findingCount).toBe(1);
    expect(findings).toHaveLength(1);
    expect(findings[0]?.rule.id).toBe("zig/excessive-as-casts");
    expect(findings[0]?.observed).toBe(2);
    expect(await ownerText(findings[0]!)).toContain("fn crowded");
    expect(repeated.files).toEqual(result.files);
    expect(repeated.rules).toEqual(result.rules);
    expect([...repeated.findings]).toEqual([...result.findings]);
  });

  test("PowerShell uses hash captures in the generic nearest-owner counting pipeline", async () => {
    const source = join(fixtureRoot, "sample.ps1");
    const result = await inspect({ paths: [source], rulePaths: [powershellRulePath] });
    const repeated = await inspect({ paths: [source], rulePaths: [powershellRulePath] });
    const findings = records(result);

    expect(result.languages).toEqual(["powershell"]);
    expect(result.checkedFiles).toBe(1);
    expect(result.diagnostics).toEqual([]);
    expect(result.findingCount).toBe(1);
    expect(findings).toHaveLength(1);
    expect(findings[0]?.rule.id).toBe("powershell/excessive-invoke-expression");
    expect(findings[0]?.observed).toBe(2);
    expect(await ownerText(findings[0]!)).toContain("function Invoke-Repeatedly");
    expect(repeated.files).toEqual(result.files);
    expect(repeated.rules).toEqual(result.rules);
    expect([...repeated.findings]).toEqual([...result.findings]);
  });

  test("PowerShell syntax errors produce diagnostics without partial findings", async () => {
    const root = await temporaryRoot();
    const source = join(root, "broken.ps1");
    await Bun.write(source, "function Broken { Invoke-Expression $first");
    const result = await inspect({ paths: [source], rulePaths: [powershellRulePath] });

    expect(result.languages).toEqual(["powershell"]);
    expect(result.checkedFiles).toBe(0);
    expect(result.findingCount).toBe(0);
    expect(result.diagnostics).toHaveLength(1);
    expect(result.diagnostics[0]?.message).toContain("syntax errors");
  });

  test("counts lexical sites, respects strict greater-than, and is reproducible", async () => {
    expect((await inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], threshold: 2 })).findingCount).toBe(0);
    const first = await inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], threshold: 0 });
    const second = await inspect({ paths: [fixtureRoot, fixtureRoot], rulePaths: [exampleRulePack], threshold: 0 });
    expect(second.files).toEqual(first.files);
    expect(second.rules).toEqual(first.rules);
    expect([...second.findings]).toEqual([...first.findings]);
    expect(records(first).filter((finding) => finding.observed === 1)).toHaveLength(4);
  });

  test("rejects invalid scalar options instead of coercing them", async () => {
    await expect(inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], threshold: -1 })).rejects.toThrow();
    await expect(inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], threshold: 1.5 })).rejects.toThrow();
    await expect(inspect({ paths: [], rulePaths: [exampleRulePack] })).rejects.toThrow();
    await expect(inspect({
      paths: [fixtureRoot], rulePaths: [exampleRulePack], maxFindings: 0x1_0000_0000,
    })).rejects.toThrow();
  });

  test("changing only YAML selects different syntax and metadata", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.rs");
    await Bun.write(source, 'fn example() { let label = "🦀"; value.clone().clone(); value.clone(); value.unwrap(); value.unwrap(); }');
    const yaml = await Bun.file(rustRulePath).text();
    const changedRule = join(root, "changed.yaml");
    await Bun.write(changedRule, yaml
      .replace("rust/excessive-clones", "probe/changed-pattern")
      .replace("$VALUE.clone()", "$VALUE.unwrap()")
      .replaceAll("clone call sites", "unwrap call sites"));

    const result = await inspect({ paths: [source], rulePaths: [rustRulePath, changedRule] });
    expect(records(result).map((finding) => [finding.rule.id, finding.observed])).toEqual([
      ["rust/excessive-clones", 3], ["probe/changed-pattern", 2],
    ]);
    expect(result.rules[1]?.evidenceSubject).toBe("unwrap call sites");

    await Bun.write(changedRule, (await Bun.file(changedRule).text()).replace("gt: 1", "gt: 2"));
    expect((await inspect({ paths: [source], rulePaths: [changedRule] })).findingCount).toBe(0);
  });

  test("only relevant rules run while discovery reports every supported language", async () => {
    const root = await temporaryRoot();
    for (const file of ["a.rs", "b.rs"]) {
      await Bun.write(join(root, file), "fn same() { a.clone(); b.clone(); }");
    }
    await Bun.write(join(root, "broken.go"), "func malformed(");
    await Bun.write(join(root, "unrelated.ts"), "not even valid TypeScript");
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath] });
    expect(result.languages).toEqual(["go", "rust", "typescript"]);
    expect(result.checkedFiles).toBe(2);
    expect(result.diagnostics).toEqual([]);
    expect(new Set(records(result).map((finding) => finding.file)).size).toBe(2);
  });

  test("gitignored files, hidden directories, build output, and symlink loops are skipped", async () => {
    const root = await temporaryRoot();
    const source = "fn counted() { a.clone(); b.clone(); }";
    await Bun.write(join(root, ".gitignore"), "ignored.rs\n");
    await Bun.write(join(root, "ignored.rs"), source);
    await Bun.write(join(root, "kept.rs"), source);
    for (const directory of [".cache", "target", "vendor", "node_modules"]) {
      await mkdir(join(root, directory));
      await Bun.write(join(root, directory, "excluded.rs"), source);
    }
    await symlink(root, join(root, "loop"), "dir");
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath] });
    expect(result.checkedFiles).toBe(1);
    expect(result.findingCount).toBe(1);
    expect(records(result).map((finding) => basename(finding.file))).toEqual(["kept.rs"]);
  });

  test("syntax errors produce diagnostics without partial findings", async () => {
    const root = await temporaryRoot();
    await Bun.write(join(root, "broken.rs"), "fn broken() { value.clone(); value.clone(); fn (");
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath] });
    expect(result.checkedFiles).toBe(0);
    expect(result.findings.length).toBe(0);
    expect(result.diagnostics).toHaveLength(1);
    expect(result.diagnostics[0]?.message).toContain("syntax errors");
  });

  test("invalid rules and unreadable paths fail explicitly", async () => {
    const root = await temporaryRoot();
    const original = await Bun.file(rustRulePath).text();
    const rulePath = join(root, "invalid.yaml");
    for (const yaml of [
      original.replace("version: 1", "version: 99"),
      original.replace("rust/excessive-clones", "no-namespace"),
      original.replace("language: rust", "language: zig"),
      original.replace("aggregate: count", "aggregate: sum"),
      original.replace("gt: 1", "gt: -1"),
      original.replace("[function_item, closure_expression]", "[]"),
      original.replace("function_item", "nonexistent_kind"),
      original.replace("pattern: $VALUE.clone()", "kind: nonexistent_kind"),
      original.replace("pattern: $VALUE.clone()", 'pattern: ""'),
      original.replace("pattern: $VALUE.clone()", "pattern: $VALUE.clone()\n  kind: call_expression"),
      original + "unexpected: true\n",
    ]) {
      await Bun.write(rulePath, yaml);
      await expect(inspect({ paths: [fixtureRoot], rulePaths: [rulePath] })).rejects.toThrow();
    }
    await expect(inspect({ paths: [fixtureRoot], rulePaths: [] })).rejects.toThrow("rulePaths");
    await expect(inspect({ paths: [fixtureRoot], rulePaths: [rustRulePath, rustRulePath] })).rejects.toThrow("duplicate rule ID");
    await expect(inspect({ paths: [join(root, "missing")], rulePaths: [goRulePath] })).rejects.toThrow("source path");
    await expect(inspect({ paths: [fixtureRoot], rulePaths: [join(root, "missing.yaml")] })).rejects.toThrow("rule");
  });

  test("profiles phases, shares execution, and invalidates native caches", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.rs");
    await Bun.write(source, "fn example() { a.clone(); b.clone(); }");
    const original = await Bun.file(rustRulePath).text();
    const firstRule = join(root, "first.yaml");
    const secondRule = join(root, "second.yaml");
    await Bun.write(firstRule, original.replace("rust/excessive-clones", "probe/first"));
    await Bun.write(secondRule, original.replace("rust/excessive-clones", "probe/second"));
    const run = async () => await inspect({
      paths: [source], rulePaths: [firstRule, secondRule], profile: true,
    });

    const cold = await run();
    expect(cold.findingCount).toBe(2);
    expect(cold.performance?.selectorExecutions).toBe(1);
    expect(cold.performance?.ruleEvaluations).toBe(2);
    expect(cold.performance?.ruleCacheMisses).toBe(2);
    expect(cold.performance?.resultCacheMisses).toBe(1);
    expect(cold.performance?.parseCacheMisses).toBe(1);
    const warm = await run();
    expect([...warm.findings]).toEqual([...cold.findings]);
    expect(warm.performance?.ruleCacheHits).toBe(2);
    expect(warm.performance?.resultCacheHits).toBe(1);
    expect(warm.performance?.parseCacheHits).toBe(0);
    expect(warm.performance?.selectorExecutions).toBe(0);
    expect(warm.performance?.ruleEvaluations).toBe(0);

    await Bun.write(source, "fn example() { a.clone(); b.clone(); c.clone(); }");
    const changedSource = await run();
    expect(changedSource.performance?.resultCacheMisses).toBe(1);
    expect(changedSource.performance?.parseCacheMisses).toBe(1);
    expect(records(changedSource).map((finding) => finding.observed)).toEqual([3, 3]);

    await Bun.write(firstRule, (await Bun.file(firstRule).text()).replace("severity: info", "severity: warning"));
    const changedRule = await run();
    expect(changedRule.performance?.ruleCacheHits).toBe(1);
    expect(changedRule.performance?.ruleCacheMisses).toBe(1);
    expect(changedRule.performance?.resultCacheMisses).toBe(1);
    expect(changedRule.performance?.parseCacheHits).toBe(1);
  });

  test("reuses compact results beyond the bounded AST cache and remaps file IDs", async () => {
    const root = await temporaryRoot();
    const fileCount = 300;
    for (let index = 0; index < fileCount; index++) {
      await Bun.write(
        join(root, `${String(index).padStart(3, "0")}.rs`),
        `fn owner_${index}() { a.clone(); b.clone(); }`,
      );
    }

    const options = { paths: [root], rulePaths: [rustRulePath], profile: true } as const;
    const cold = await inspect(options);
    const warm = await inspect(options);

    expect(cold.findingCount).toBe(fileCount);
    expect(cold.performance?.resultCacheMisses).toBe(fileCount);
    expect(warm.findingCount).toBe(fileCount);
    expect(warm.performance?.resultCacheHits).toBe(fileCount);
    expect(warm.performance?.resultCacheMisses).toBe(0);
    expect(warm.performance?.parseCacheHits).toBe(0);
    expect(warm.performance?.parseCacheMisses).toBe(0);
    expect(warm.performance?.selectorExecutions).toBe(0);
    expect(records(warm).map((finding) => finding.file)).toEqual([...warm.files]);
  });

  test("bounds native file parallelism", async () => {
    const root = await temporaryRoot();
    for (let index = 0; index < 8; index++) {
      await Bun.write(join(root, `${index}.rs`), `fn owner_${index}() { a.clone(); b.clone(); }`);
    }
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath], profile: true });
    expect(result.checkedFiles).toBe(8);
    expect(result.performance?.workerThreads).toBeGreaterThanOrEqual(1);
    expect(result.performance?.workerThreads).toBeLessThanOrEqual(4);
  });

  test("bounds compact findings without hiding the exact count", async () => {
    const root = await temporaryRoot();
    for (let index = 0; index < 5; index++) {
      await Bun.write(join(root, `${index}.rs`),
        `fn owner_${index}() { a.clone(); b.clone(); c.clone(); }`);
    }
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath], maxFindings: 2 });
    expect(result.findingCount).toBe(5);
    expect(result.findings.length / result.recordWidth).toBe(2);
    expect(result.truncated).toBe(true);
    expect(records(result).map((finding) => finding.observed)).toEqual([3, 3]);
  });

  test("profiles aggregation and compact output construction separately", async () => {
    const result = await inspect({ paths: [fixtureRoot], rulePaths: [exampleRulePack], profile: true });
    expect(result.performance?.aggregationMs).toBeGreaterThanOrEqual(0);
    expect(result.performance?.outputBuildMs).toBeGreaterThanOrEqual(0);
    expect(result.performance?.resultMergeMs).toBeGreaterThanOrEqual(0);
  });

  test("executes capture text, match ancestry, and group evidence conditions", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.rs");
    const rules = join(root, "relational.badbox");
    await Bun.write(source, `
fn flagged() { loop { unwrap(value); break; } }
fn wrong_method() { loop { inspect(value); break; } }
fn outside_loop() { unwrap(value); }
fn guarded() { loop { expect(value); break; } cancel(); }
fn has_evidence() { write(value); transaction(); }
fn lacks_evidence() { write(value); }
`);
    await Bun.write(rules, `#badbox 1
rule rust/loop-without-cancel for rust {
  summary "Selected calls inside loops without cancellation evidence"
  find code(method, value) \`method(value)\`
  group by nearest callable
  where text(method) in ["unwrap", "expect"]
  where text(method) != "inspect"
  where text(method) not in ["inspect", "debug"]
  where text(method) matches "^(unwrap|expect)$"
  where match inside any { node loop_expression }
  where group lacks any { code() \`cancel()\` }
  when count > 0
  report {
    severity warning
    message "Loop call has no recognized cancellation evidence"
    evidence "selected loop calls"
  }
}

rule rust/write-with-transaction for rust {
  summary "Writes in functions with transaction evidence"
  find code(method, value) \`method(value)\`
  group by nearest callable
  where text(method) == "write"
  where group has all {
    code() \`transaction()\`
    node function_item
  }
  where group lacks any { code() \`cancel()\` }
  when count > 0
  report {
    severity info
    message "Write has recognized transaction evidence"
    evidence "write calls"
  }
}
`);

    const result = await inspect({ paths: [source], rulePaths: [rules], profile: true });
    expect(result.diagnostics).toEqual([]);
    expect(result.performance?.selectorExecutions).toBe(5);
    expect(result.performance?.ruleEvaluations).toBe(2);
    const actual = await Promise.all(records(result).map(async (finding) => [
      finding.rule.id, await ownerText(finding), finding.observed,
    ]));
    expect(actual).toEqual([
      ["rust/loop-without-cancel", "fn flagged() { loop { unwrap(value); break; } }", 1],
      ["rust/write-with-transaction", "fn has_evidence() { write(value); transaction(); }", 1],
    ]);
  });

  test("executes follows and precedes only between statements in the same lexical block and callable", async () => {
    const root = await temporaryRoot();
    const source = join(root, "ordering.rs");
    const rules = join(root, "ordering.badbox");
    await Bun.write(source, `
fn follows_any() { prepare(); middle(); target(); }
fn precedes_any() { target(); middle(); cleanup(); }
fn follows_all() { prepare(); lock(); target(); }
fn missing_all() { prepare(); target(); }
fn wrong_order() { target(); prepare(); }
fn separate_branches(flag: bool) { if flag { prepare(); } else { target(); } }
fn match_branches(flag: bool) { match flag { true => prepare(), false => target(), } }
fn nested_block() { prepare(); loop { target(); break; } }
fn same_statement() { combine(prepare(), target()); }
fn nested_callable() { prepare(); let inner = || { target(); }; inner(); }
`);
    await Bun.write(rules, `#badbox 1
rule rust/follows-any for rust {
  summary "Target follows preparation"
  find code() \`target()\`
  group by nearest callable
  where match follows any { code() \`prepare()\` }
  when count > 0
  report {
    severity warning
    message "Target follows preparation"
    evidence "target calls"
  }
}

rule rust/precedes-any for rust {
  summary "Target precedes cleanup"
  find code() \`target()\`
  group by nearest callable
  where match precedes any { code() \`cleanup()\` }
  when count > 0
  report {
    severity warning
    message "Target precedes cleanup"
    evidence "target calls"
  }
}

rule rust/follows-all for rust {
  summary "Target follows all preparation selectors"
  find code() \`target()\`
  group by nearest callable
  where match follows all {
    code() \`prepare()\`
    code() \`lock()\`
  }
  when count > 0
  report {
    severity warning
    message "Target follows all preparation selectors"
    evidence "target calls"
  }
}
`);

    const result = await inspect({ paths: [source], rulePaths: [rules], profile: true });
    expect(result.diagnostics).toEqual([]);
    expect(result.performance?.selectorExecutions).toBe(4);
    expect(result.performance?.ruleEvaluations).toBe(3);
    const actual = await Promise.all(records(result).map(async (finding) => [
      finding.rule.id, await ownerText(finding), finding.observed,
    ] as const));
    actual.sort((left, right) => `${left[0]}\0${left[1]}`.localeCompare(`${right[0]}\0${right[1]}`));
    expect(actual).toEqual([
      ["rust/follows-all", "fn follows_all() { prepare(); lock(); target(); }", 1],
      ["rust/follows-any", "fn follows_all() { prepare(); lock(); target(); }", 1],
      ["rust/follows-any", "fn follows_any() { prepare(); middle(); target(); }", 1],
      ["rust/follows-any", "fn missing_all() { prepare(); target(); }", 1],
      ["rust/precedes-any", "fn precedes_any() { target(); middle(); cleanup(); }", 1],
    ]);
  });

  test("executes statement ordering across Go, PowerShell, and Zig callable groups", async () => {
    const cases = [
      {
        language: "go",
        extension: "go",
        source: `package probe
func ordered() { prepare(); target() }
func branches(flag bool) { if flag { prepare() } else { target() } }
func nested() { prepare(); inner := func() { target() }; inner() }
`,
        owner: "func ordered() { prepare(); target() }",
        prepare: "prepare()",
        target: "target()",
      },
      {
        language: "powershell",
        extension: "ps1",
        source: `function Ordered { Invoke-Prepare; Invoke-Target }
function Branches { if ($true) { Invoke-Prepare } else { Invoke-Target } }
function Nested { Invoke-Prepare; function Inner { Invoke-Target }; Inner }
`,
        owner: "function Ordered { Invoke-Prepare; Invoke-Target }",
        prepare: "Invoke-Prepare",
        target: "Invoke-Target",
      },
      {
        language: "zig",
        extension: "zig",
        source: `fn ordered() void { prepare(); target(); }
fn branches(flag: bool) void { if (flag) { prepare(); } else { target(); } }
fn nested() void { prepare(); const Inner = struct { fn run() void { target(); } }; Inner.run(); }
`,
        owner: "fn ordered() void { prepare(); target(); }",
        prepare: "prepare()",
        target: "target()",
      },
    ] as const;

    for (const probe of cases) {
      const root = await temporaryRoot();
      const source = join(root, `ordering.${probe.extension}`);
      const rules = join(root, `ordering-${probe.language}.badbox`);
      await Bun.write(source, probe.source);
      await Bun.write(rules, `#badbox 1
rule ${probe.language}/ordering for ${probe.language} {
  summary "Target follows preparation"
  find code() \`${probe.target}\`
  group by nearest callable
  where match follows any { code() \`${probe.prepare}\` }
  when count > 0
  report {
    severity warning
    message "Target follows preparation"
    evidence "target sites"
  }
}
`);

      const result = await inspect({ paths: [source], rulePaths: [rules] });
      expect(result.diagnostics).toEqual([]);
      expect(result.findingCount).toBe(1);
      expect(await ownerText(records(result)[0]!)).toBe(probe.owner);
    }
  });

  test("correlates repeated capture names across primary and ordering selectors", async () => {
    const root = await temporaryRoot();
    const source = join(root, "capture-ordering.zig");
    const rules = join(root, "capture-ordering.badbox");
    await Bun.write(source, `
fn sameReceiver() void { stmt.reset(); stmt.clearBindings(); }
fn differentReceivers() void { first.reset(); second.clearBindings(); }
fn anotherSameReceiver() void { other.reset(); other.clearBindings(); }
`);
    await Bun.write(rules, `#badbox 1
rule zig/correlated-ordering for zig {
  summary "A statement is reset before its bindings are cleared"
  find code(statement) \`statement.clearBindings()\`
  group by nearest callable
  where match follows any {
    code(statement) \`statement.reset()\`
  }
  when count > 0
  report {
    severity info
    message "Statement reset is followed by clearing its bindings"
    evidence "same-receiver reset and clear sites"
  }
}
`);

    const result = await inspect({ paths: [source], rulePaths: [rules] });
    expect(result.diagnostics).toEqual([]);
    expect(result.recordWidth).toBe(5);
    const owners = await Promise.all(records(result).map(ownerText));
    expect(owners).toEqual([
      "fn sameReceiver() void { stmt.reset(); stmt.clearBindings(); }",
      "fn anotherSameReceiver() void { other.reset(); other.clearBindings(); }",
    ]);
  });

  test("requires every shared capture in an ordering selector to match", async () => {
    const root = await temporaryRoot();
    const source = join(root, "multi-capture-ordering.zig");
    const rules = join(root, "multi-capture-ordering.badbox");
    await Bun.write(source, `
fn bothMatch() void { db.prepare(sql); db.execute(sql); }
fn databaseDiffers() void { first.prepare(sql); second.execute(sql); }
fn statementDiffers() void { db.prepare(first_sql); db.execute(second_sql); }
`);
    await Bun.write(rules, `#badbox 1
rule zig/multi-capture-ordering for zig {
  summary "A database executes the statement it prepared"
  find code(database, statement) \`database.execute(statement)\`
  group by nearest callable
  where match follows any {
    code(database, statement) \`database.prepare(statement)\`
  }
  when count > 0
  report {
    severity info
    message "Database execution follows matching preparation"
    evidence "same-database and same-statement sites"
  }
}
`);

    const result = await inspect({ paths: [source], rulePaths: [rules] });
    expect(result.diagnostics).toEqual([]);
    expect(result.findingCount).toBe(1);
    expect(await ownerText(records(result)[0]!)).toBe(
      "fn bothMatch() void { db.prepare(sql); db.execute(sql); }",
    );
  });

  test("interns exact selectors across distinct rule plans", async () => {
    const root = await temporaryRoot();
    const source = join(root, "input.rs");
    const rules = join(root, "shared-selectors.badbox");
    await Bun.write(source, "fn example() { clone(value); }");
    const definitions = Array.from({ length: 50 }, (_, index) => `
rule rust/shared-${index} for rust {
  summary "Shared selector probe ${index}"
  find code(method, value) \`method(value)\`
  group by nearest callable
  where text(method) matches "^(clone|never_${index})$"
  where group lacks any { code() \`cancel()\` }
  when count > 0
  report {
    severity info
    message "Shared selector matched"
    evidence "call sites"
  }
}`).join("\n");
    await Bun.write(rules, `#badbox 1\n${definitions}\n`);

    const result = await inspect({ paths: [source], rulePaths: [rules], profile: true });
    expect(result.findingCount).toBe(50);
    expect(result.performance?.selectorExecutions).toBe(2);
    expect(result.performance?.ruleEvaluations).toBe(50);
  });
});
