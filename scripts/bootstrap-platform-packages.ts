import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { platformPackages } from "./package-platforms.ts";

const bootstrapVersion = "0.0.0";

interface PrepareBootstrapPackagesOptions {
  readonly root: URL | string;
  readonly outputDir: string;
}

function pathFromRoot(root: URL | string): string {
  return root instanceof URL ? fileURLToPath(root) : root;
}

export async function prepareBootstrapPackages(options: PrepareBootstrapPackagesOptions): Promise<void> {
  const root = pathFromRoot(options.root);
  const rootManifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  const license = await readFile(join(root, "LICENSE"));

  await mkdir(options.outputDir, { recursive: true });
  for (const platform of platformPackages) {
    const packageDirectory = join(options.outputDir, platform.name);
    await mkdir(packageDirectory, { recursive: true });
    const manifest: Record<string, unknown> = {
      name: platform.name,
      version: bootstrapVersion,
      description: "Reserved native platform package for Badbox.",
      license: rootManifest.license,
      repository: rootManifest.repository,
      main: "index.cjs",
      files: ["index.cjs"],
      os: [platform.os],
      cpu: [platform.cpu],
      publishConfig: { access: "public", tag: "bootstrap" },
    };
    if (platform.libc) manifest.libc = [platform.libc];

    await Promise.all([
      writeFile(join(packageDirectory, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`),
      writeFile(
        join(packageDirectory, "README.md"),
        `# ${platform.name}\n\nNamespace reservation for the native ${platform.os}/${platform.cpu} engine used by [Badbox](https://github.com/0ctacity/badbox). Version 0.0.0 contains no native binary.\n`,
      ),
      writeFile(
        join(packageDirectory, "index.cjs"),
        `throw new Error("${platform.name}@${bootstrapVersion} is a namespace-reservation package and contains no native engine. Install a released version of badbox instead.");\n`,
      ),
      writeFile(join(packageDirectory, "LICENSE"), license),
    ]);
  }
}

async function run(command: string[], npmCache: string): Promise<void> {
  const child = Bun.spawn(command, {
    env: { ...process.env, npm_config_cache: npmCache },
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  const exitCode = await child.exited;
  if (exitCode !== 0) throw new Error(`${command.join(" ")} exited with code ${exitCode}`);
}

async function packageVersionExists(packageName: string, npmCache: string): Promise<boolean> {
  const command = ["npm", "view", `${packageName}@${bootstrapVersion}`, "version"];
  const child = Bun.spawn(command, {
    env: { ...process.env, npm_config_cache: npmCache },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (exitCode === 0) return true;
  const output = `${stdout}\n${stderr}`;
  if (output.includes("E404") || output.includes("404 Not Found")) return false;
  throw new Error(`${command.join(" ")} failed:\n${output.trim()}`);
}

async function main(): Promise<void> {
  const publish = process.argv.includes("--publish");
  const dryRun = process.argv.includes("--dry-run");
  if (publish === dryRun || process.argv.length !== 3) {
    throw new Error(
      "Use exactly one mode: bun run bootstrap:platform-packages --dry-run or bun run bootstrap:platform-packages --publish",
    );
  }

  const root = fileURLToPath(new URL("../", import.meta.url));
  const temporaryDirectory = await mkdtemp(join(tmpdir(), "badbox-platform-bootstrap-"));
  try {
    await prepareBootstrapPackages({ root, outputDir: temporaryDirectory });
    const npmCache = join(temporaryDirectory, "npm-cache");
    if (publish) await run(["npm", "whoami"], npmCache);

    for (const platform of platformPackages) {
      const packageDirectory = join(temporaryDirectory, platform.name);
      if (publish && await packageVersionExists(platform.name, npmCache)) {
        console.log(`${platform.name}@${bootstrapVersion} already exists; skipping.`);
        continue;
      }
      const command = dryRun
        ? ["npm", "pack", packageDirectory, "--dry-run"]
        : ["npm", "publish", packageDirectory, "--access", "public", "--tag", "bootstrap"];
      await run(command, npmCache);
    }
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

if (import.meta.main) await main();
