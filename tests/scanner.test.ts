import { afterAll, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect, iterateFindings, type FindingRecord, type ScanResult } from "../src/scanner/index.ts";

const fixtureRoot = fileURLToPath(new URL("./fixtures/counts", import.meta.url));
const rustRulePath = fileURLToPath(new URL("../rules/rust/excessive-clones.yaml", import.meta.url));
const goRulePath = fileURLToPath(new URL("../rules/go/excessive-goroutines.yaml", import.meta.url));
const powershellRulePath = fileURLToPath(new URL(
  "../rules/powershell/excessive-invoke-expression.yaml", import.meta.url,
));
const zigRulePath = fileURLToPath(new URL("../rules/zig/excessive-as-casts.yaml", import.meta.url));
const temporaryRoots: string[] = [];

const records = (result: ScanResult) => [...iterateFindings(result)];

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
  test("Rust, Go, PowerShell, and Zig share compact nearest-owner counting", async () => {
    const result = await inspect({ paths: [fixtureRoot], threshold: 1 });
    const findings = records(result);
    expect(result.languages).toEqual(["go", "powershell", "rust", "zig"]);
    expect(result.scannedFiles).toBe(4);
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
    expect(result.scannedFiles).toBe(1);
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
    expect(result.scannedFiles).toBe(1);
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
    expect(result.scannedFiles).toBe(1);
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
    expect(result.scannedFiles).toBe(0);
    expect(result.findingCount).toBe(0);
    expect(result.diagnostics).toHaveLength(1);
    expect(result.diagnostics[0]?.message).toContain("syntax errors");
  });

  test("counts lexical sites, respects strict greater-than, and is reproducible", async () => {
    expect((await inspect({ paths: [fixtureRoot], threshold: 2 })).findingCount).toBe(0);
    const first = await inspect({ paths: [fixtureRoot], threshold: 0 });
    const second = await inspect({ paths: [fixtureRoot, fixtureRoot], threshold: 0 });
    expect(second.files).toEqual(first.files);
    expect(second.rules).toEqual(first.rules);
    expect([...second.findings]).toEqual([...first.findings]);
    expect(records(first).filter((finding) => finding.observed === 1)).toHaveLength(4);
  });

  test("rejects invalid scalar options instead of coercing them", async () => {
    await expect(inspect({ paths: [fixtureRoot], threshold: -1 })).rejects.toThrow();
    await expect(inspect({ paths: [fixtureRoot], threshold: 1.5 })).rejects.toThrow();
    await expect(inspect({ paths: [] })).rejects.toThrow();
    await expect(inspect({ paths: [fixtureRoot], maxFindings: 0x1_0000_0000 })).rejects.toThrow();
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
    expect(result.scannedFiles).toBe(2);
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
    const result = await inspect({ paths: [root] });
    expect(result.scannedFiles).toBe(1);
    expect(result.findingCount).toBe(1);
    expect(records(result)[0]?.file).toEndWith("/kept.rs");
  });

  test("syntax errors produce diagnostics without partial findings", async () => {
    const root = await temporaryRoot();
    await Bun.write(join(root, "broken.rs"), "fn broken() { value.clone(); value.clone(); fn (");
    const result = await inspect({ paths: [root] });
    expect(result.scannedFiles).toBe(0);
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
    await expect(inspect({ paths: [join(root, "missing")], rulePaths: [goRulePath] })).rejects.toThrow("scan path");
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
    expect(cold.performance?.parseCacheMisses).toBe(1);
    const warm = await run();
    expect([...warm.findings]).toEqual([...cold.findings]);
    expect(warm.performance?.ruleCacheHits).toBe(2);
    expect(warm.performance?.parseCacheHits).toBe(1);

    await Bun.write(source, "fn example() { a.clone(); b.clone(); c.clone(); }");
    const changedSource = await run();
    expect(changedSource.performance?.parseCacheMisses).toBe(1);
    expect(records(changedSource).map((finding) => finding.observed)).toEqual([3, 3]);

    await Bun.write(firstRule, (await Bun.file(firstRule).text()).replace("severity: info", "severity: warning"));
    const changedRule = await run();
    expect(changedRule.performance?.ruleCacheHits).toBe(1);
    expect(changedRule.performance?.ruleCacheMisses).toBe(1);
  });

  test("bounds native file parallelism", async () => {
    const root = await temporaryRoot();
    for (let index = 0; index < 8; index++) {
      await Bun.write(join(root, `${index}.rs`), `fn owner_${index}() { a.clone(); b.clone(); }`);
    }
    const result = await inspect({ paths: [root], rulePaths: [rustRulePath], profile: true });
    expect(result.scannedFiles).toBe(8);
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
    const result = await inspect({ paths: [fixtureRoot], profile: true });
    expect(result.performance?.aggregationMs).toBeGreaterThanOrEqual(0);
    expect(result.performance?.outputBuildMs).toBeGreaterThanOrEqual(0);
    expect(result.performance?.resultMergeMs).toBeGreaterThanOrEqual(0);
  });
});
