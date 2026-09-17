import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { supportedNativePackages } from "../src/scanner/native.ts";

function cargoVersion(manifest: string, path: string): string {
  const packageSection = manifest.match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1];
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!version) throw new Error(`Cannot read package version from ${path}`);
  return version;
}

export async function validateRelease(root: string, expectedVersion: string): Promise<void> {
  const packagePath = join(root, "package.json");
  const packageManifest = JSON.parse(await readFile(packagePath, "utf8"));
  const versions: ReadonlyArray<readonly [string, string]> = [
    [packagePath, packageManifest.version],
    [join(root, "native/Cargo.toml"), cargoVersion(
      await readFile(join(root, "native/Cargo.toml"), "utf8"),
      "native/Cargo.toml",
    )],
    [join(root, "native/tiny-dsl/Cargo.toml"), cargoVersion(
      await readFile(join(root, "native/tiny-dsl/Cargo.toml"), "utf8"),
      "native/tiny-dsl/Cargo.toml",
    )],
  ];

  for (const [path, version] of versions) {
    if (version !== expectedVersion) {
      throw new Error(`${path} has version ${version}; expected ${expectedVersion}`);
    }
  }
  for (const packageName of supportedNativePackages) {
    const version = packageManifest.optionalDependencies?.[packageName];
    if (version !== expectedVersion) {
      throw new Error(`optionalDependencies.${packageName} is ${String(version)}; expected ${expectedVersion}`);
    }
  }
}

if (import.meta.main) {
  const expectedVersion = process.argv[2];
  if (!expectedVersion) throw new Error("Usage: bun run scripts/validate-release.ts <version>");
  const root = fileURLToPath(new URL("../", import.meta.url));
  await validateRelease(root, expectedVersion);
  console.log(`Release version ${expectedVersion} is consistent.`);
}
