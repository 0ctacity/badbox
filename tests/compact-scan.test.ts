import { expect, test } from "bun:test";
import { fileURLToPath } from "node:url";
import { inspect } from "../src/scanner/index.ts";

const fixtureRoot = fileURLToPath(new URL("./fixtures/counts", import.meta.url));

test("native scan returns interned tables and fixed-width integer findings", async () => {
  const result = await inspect({ paths: [fixtureRoot], threshold: 1 });

  expect(result.recordWidth).toBe(5);
  expect(result.findings).toBeInstanceOf(Uint32Array);
  expect(result.findings.length).toBe(result.findingCount * result.recordWidth);
  expect(result.files).toHaveLength(2);
  expect(result.rules.map((rule) => rule.id)).toEqual([
    "rust/excessive-clones",
    "go/excessive-goroutines",
  ]);

  for (let offset = 0; offset < result.findings.length; offset += result.recordWidth) {
    const fileId = result.findings[offset]!;
    const ruleId = result.findings[offset + 1]!;
    const ownerStart = result.findings[offset + 2]!;
    const ownerEnd = result.findings[offset + 3]!;
    const observed = result.findings[offset + 4]!;
    expect(result.files[fileId]).toBeDefined();
    expect(result.rules[ruleId]).toBeDefined();
    expect(ownerStart).toBeLessThan(ownerEnd);
    expect(observed).toBe(2);
  }
});
