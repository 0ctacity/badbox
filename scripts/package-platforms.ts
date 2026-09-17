import { copyFile, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

export interface PlatformPackage {
  readonly name: string;
  readonly os: "darwin" | "linux" | "win32";
  readonly cpu: "arm64" | "x64";
  readonly libc?: "glibc";
}

export const platformPackages: readonly PlatformPackage[] = [
  { name: "badbox-darwin-arm64", os: "darwin", cpu: "arm64" },
  { name: "badbox-darwin-x64", os: "darwin", cpu: "x64" },
  { name: "badbox-linux-arm64-gnu", os: "linux", cpu: "arm64", libc: "glibc" },
  { name: "badbox-linux-x64-gnu", os: "linux", cpu: "x64", libc: "glibc" },
  { name: "badbox-windows-x64", os: "win32", cpu: "x64" },
] as const;

interface PreparePlatformPackagesOptions {
  readonly root: URL | string;
  readonly artifactsDir: string;
  readonly outputDir: string;
}

function pathFromRoot(root: URL | string): string {
  return root instanceof URL ? fileURLToPath(root) : root;
}

export async function preparePlatformPackages(options: PreparePlatformPackagesOptions): Promise<void> {
  const root = pathFromRoot(options.root);
  const rootManifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  const license = await readFile(join(root, "LICENSE"));

  await rm(options.outputDir, { recursive: true, force: true });
  await mkdir(options.outputDir, { recursive: true });

  for (const platform of platformPackages) {
    const artifact = join(options.artifactsDir, `${platform.name}.node`);
    try {
      if (!(await stat(artifact)).isFile()) throw new Error("not a file");
    } catch (cause) {
      throw new Error(`Missing native artifact: ${artifact}`, { cause });
    }

    const packageDirectory = join(options.outputDir, platform.name);
    await mkdir(packageDirectory, { recursive: true });
    const manifest: Record<string, unknown> = {
      name: platform.name,
      version: rootManifest.version,
      description: "Native engine for Badbox.",
      license: rootManifest.license,
      repository: rootManifest.repository,
      main: "badbox.node",
      files: ["badbox.node"],
      os: [platform.os],
      cpu: [platform.cpu],
      publishConfig: { access: "public" },
    };
    if (platform.libc) manifest.libc = [platform.libc];

    await Promise.all([
      writeFile(join(packageDirectory, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`),
      writeFile(
        join(packageDirectory, "README.md"),
        `# ${platform.name}\n\nNative ${platform.os}/${platform.cpu} engine for [Badbox](https://github.com/0ctacity/badbox).\n`,
      ),
      writeFile(join(packageDirectory, "LICENSE"), license),
      copyFile(artifact, join(packageDirectory, "badbox.node")),
    ]);
  }
}

if (import.meta.main) {
  const root = fileURLToPath(new URL("../", import.meta.url));
  await preparePlatformPackages({
    root,
    artifactsDir: join(root, "release/artifacts"),
    outputDir: join(root, "release/npm"),
  });
  console.log(`Prepared ${platformPackages.length} platform packages in release/npm.`);
}
