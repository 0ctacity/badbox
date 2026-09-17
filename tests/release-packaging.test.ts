import { afterAll, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import {
  nativePackageName,
  supportedNativePackages,
} from "../src/scanner/native.ts";
import {
  platformPackages,
  preparePlatformPackages,
} from "../scripts/package-platforms.ts";
import { validateRelease } from "../scripts/validate-release.ts";
import { prepareBootstrapPackages } from "../scripts/bootstrap-platform-packages.ts";

const temporaryDirectories: string[] = [];

afterAll(async () => {
  await Promise.all(temporaryDirectories.map((path) => rm(path, { recursive: true, force: true })));
});

describe("native platform package selection", () => {
  test("uses the five Zova-style package names", () => {
    expect(supportedNativePackages).toEqual([
      "badbox-darwin-arm64",
      "badbox-darwin-x64",
      "badbox-linux-arm64-gnu",
      "badbox-linux-x64-gnu",
      "badbox-windows-x64",
    ]);
  });

  test("maps supported Node platforms and architectures", () => {
    expect(nativePackageName("darwin", "arm64")).toBe("badbox-darwin-arm64");
    expect(nativePackageName("darwin", "x64")).toBe("badbox-darwin-x64");
    expect(nativePackageName("linux", "arm64")).toBe("badbox-linux-arm64-gnu");
    expect(nativePackageName("linux", "x64")).toBe("badbox-linux-x64-gnu");
    expect(nativePackageName("win32", "x64")).toBe("badbox-windows-x64");
    expect(() => nativePackageName("win32", "arm64")).toThrow("Unsupported Badbox platform");
  });
});

describe("platform package generation", () => {
  test("copies each binary into a publishable, platform-constrained package", async () => {
    const directory = await mkdtemp(join(tmpdir(), "badbox-release-test-"));
    temporaryDirectories.push(directory);
    const artifactsDir = join(directory, "artifacts");
    const outputDir = join(directory, "npm");
    await mkdir(artifactsDir, { recursive: true });

    for (const platform of platformPackages) {
      await writeFile(join(artifactsDir, `${platform.name}.node`), platform.name);
    }

    await preparePlatformPackages({
      root: new URL("../", import.meta.url),
      artifactsDir,
      outputDir,
    });

    for (const platform of platformPackages) {
      const packageDirectory = join(outputDir, platform.name);
      const manifest = JSON.parse(await readFile(join(packageDirectory, "package.json"), "utf8"));
      expect(manifest).toMatchObject({
        name: platform.name,
        version: "0.0.1",
        main: "badbox.node",
        files: ["badbox.node"],
        os: [platform.os],
        cpu: [platform.cpu],
        publishConfig: { access: "public" },
      });
      if (platform.libc) expect(manifest.libc).toEqual([platform.libc]);
      else expect(manifest.libc).toBeUndefined();
      expect(await readFile(join(packageDirectory, "badbox.node"), "utf8")).toBe(platform.name);
      expect(await readFile(join(packageDirectory, "LICENSE"), "utf8")).toContain("MIT License");
    }
  });

  test("fails instead of creating an incomplete release", async () => {
    const directory = await mkdtemp(join(tmpdir(), "badbox-release-missing-"));
    temporaryDirectories.push(directory);
    await expect(preparePlatformPackages({
      root: new URL("../", import.meta.url),
      artifactsDir: join(directory, "artifacts"),
      outputDir: join(directory, "npm"),
    })).rejects.toThrow("Missing native artifact");
  });
});

test("root package pins every native package to its own version", async () => {
  const manifest = await Bun.file(new URL("../package.json", import.meta.url)).json();
  expect(Object.keys(manifest.optionalDependencies ?? {})).toEqual([...supportedNativePackages]);
  for (const packageName of supportedNativePackages) {
    expect(manifest.optionalDependencies[packageName]).toBe(manifest.version);
  }
});

test("release validation requires one version across npm and Rust manifests", async () => {
  const root = fileURLToPath(new URL("../", import.meta.url));
  await expect(validateRelease(root, "0.0.1")).resolves.toBeUndefined();
  await expect(validateRelease(root, "9.9.9")).rejects.toThrow("expected 9.9.9");
});

test("bootstrap packages reserve every platform name without shipping a fake binary", async () => {
  const directory = await mkdtemp(join(tmpdir(), "badbox-bootstrap-test-"));
  temporaryDirectories.push(directory);
  await prepareBootstrapPackages({
    root: new URL("../", import.meta.url),
    outputDir: directory,
  });

  for (const platform of platformPackages) {
    const packageDirectory = join(directory, platform.name);
    const manifest = JSON.parse(await readFile(join(packageDirectory, "package.json"), "utf8"));
    expect(manifest).toMatchObject({
      name: platform.name,
      version: "0.0.0",
      main: "index.cjs",
      files: ["index.cjs"],
      os: [platform.os],
      cpu: [platform.cpu],
      publishConfig: { access: "public" },
    });
    expect(await readFile(join(packageDirectory, "index.cjs"), "utf8"))
      .toContain("namespace-reservation package");
    await expect(Bun.file(join(packageDirectory, "badbox.node")).exists()).resolves.toBe(false);
  }
});
