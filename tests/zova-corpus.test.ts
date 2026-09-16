import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect, iterateFindings } from "../src/scanner/index.ts";

const root = fileURLToPath(new URL("./fixtures/zova", import.meta.url));

interface LabelRange { startByte: number; endByte: number; text: string }
interface Corpus {
  cases: Array<{
    file: string; ruleId: string; sha256: string;
    owners: Array<LabelRange & {
      name: string | null; kind: string; count: number; ranges: LabelRange[];
    }>;
  }>;
}

test("frozen Zova labels match exact compact owner ranges and counts", async () => {
  const corpus: Corpus = await Bun.file(`${root}/labels.json`).json();
  expect(corpus.cases).toHaveLength(30);
  const result = await inspect({ paths: [`${root}/source`], threshold: 0 });
  expect(result.diagnostics).toEqual([]);
  expect(result.scannedFiles).toBe(30);
  const expected = [];
  for (const entry of corpus.cases) {
    const file = `${root}/source/${entry.file}`;
    const source = await Bun.file(file).text();
    expect(createHash("sha256").update(source).digest("hex")).toBe(entry.sha256);
    const bytes = Buffer.from(source);
    for (const owner of entry.owners) {
      expect(owner.ranges).toHaveLength(owner.count);
      expect(bytes.subarray(owner.startByte, owner.endByte).toString()).toBe(owner.text);
      for (const range of owner.ranges) {
        expect(bytes.subarray(range.startByte, range.endByte).toString()).toBe(range.text);
      }
      if (owner.count > 0) expected.push({
        file,
        ruleId: entry.ruleId,
        ownerStart: owner.startByte,
        ownerEnd: owner.endByte,
        observed: owner.count,
      });
    }
  }
  const actual = [...iterateFindings(result)].map((finding) => ({
    file: finding.file,
    ruleId: finding.rule.id,
    ownerStart: finding.ownerStart,
    ownerEnd: finding.ownerEnd,
    observed: finding.observed,
  }));
  expected.sort((a, b) => a.file.localeCompare(b.file) || a.ownerStart - b.ownerStart);
  actual.sort((a, b) => a.file.localeCompare(b.file) || a.ownerStart - b.ownerStart);
  expect(actual).toEqual(expected);
  expect(actual).toHaveLength(18);
  expect(actual.reduce((sum, owner) => sum + owner.observed, 0)).toBe(19);

  const thresholded = await inspect({ paths: [`${root}/source`], threshold: 1 });
  const only = [...iterateFindings(thresholded)];
  expect(only).toHaveLength(1);
  expect(only[0]?.observed).toBe(2);
});

test("concatenated Rust excerpts keep equal owner ranges separate by offset", async () => {
  const corpus: Corpus = await Bun.file(`${root}/labels.json`).json();
  const temporary = await mkdtemp(join(tmpdir(), "badbox-zova-owners-"));
  try {
    let source = "";
    const expected = [];
    for (const entry of corpus.cases.filter((entry) => entry.file.endsWith(".rs"))) {
      const offset = Buffer.byteLength(source);
      source += await Bun.file(`${root}/source/${entry.file}`).text();
      source += "\n";
      for (const owner of entry.owners.filter((owner) => owner.count > 0)) {
        expected.push({
          ownerStart: offset + owner.startByte,
          ownerEnd: offset + owner.endByte,
          observed: owner.count,
        });
      }
    }
    const path = join(temporary, "combined.rs");
    await Bun.write(path, source);
    const result = await inspect({ paths: [path], threshold: 0 });
    expect(result.diagnostics).toEqual([]);
    expect(result.scannedFiles).toBe(1);
    expect([...iterateFindings(result)].map(({ ownerStart, ownerEnd, observed }) => ({
      ownerStart, ownerEnd, observed,
    }))).toEqual(expected);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
