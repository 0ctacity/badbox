import { expect, test } from "bun:test";

test("the local CI command runs the complete verification script", async () => {
  const manifest = await Bun.file(new URL("../package.json", import.meta.url)).json();
  expect(manifest.scripts?.ci).toBe("bun run scripts/ci.ts");

  const script = await Bun.file(new URL("../scripts/ci.ts", import.meta.url)).text();
  for (const command of [
    "cargo fmt",
    "cargo clippy",
    "cargo nextest",
    "bun run build:native",
    "bun run typecheck",
    "bun test",
  ]) {
    expect(script).toContain(command);
  }
});

test("GitHub CI checks pushes, pull requests, and every supported native platform", async () => {
  const workflow = await Bun.file(new URL("../.github/workflows/ci.yml", import.meta.url)).text();
  expect(workflow).toContain("pull_request:");
  expect(workflow).toContain("push:");
  expect(workflow).toContain("bun run ci");
  for (const runner of ["ubuntu-latest", "ubuntu-24.04-arm", "macos-latest", "macos-15-intel", "windows-latest"]) {
    expect(workflow).toContain(runner);
  }
});

test("Git checkouts preserve byte-stable LF fixtures", async () => {
  const attributes = await Bun.file(new URL("../.gitattributes", import.meta.url)).text();
  expect(attributes).toContain("* text=auto eol=lf");
});
