import { createRequire } from "node:module";

export interface NativeEngine {
  inspect(request: string): Promise<{ metadata: string; findings: Uint32Array }>;
}

export const supportedNativePackages = [
  "badbox-darwin-arm64",
  "badbox-darwin-x64",
  "badbox-linux-arm64-gnu",
  "badbox-linux-x64-gnu",
  "badbox-windows-x64",
] as const;

const nativePackages: Readonly<Record<string, typeof supportedNativePackages[number]>> = {
  "darwin-arm64": "badbox-darwin-arm64",
  "darwin-x64": "badbox-darwin-x64",
  "linux-arm64": "badbox-linux-arm64-gnu",
  "linux-x64": "badbox-linux-x64-gnu",
  "win32-x64": "badbox-windows-x64",
};

export function nativePackageName(platform: string, architecture: string): typeof supportedNativePackages[number] {
  const packageName = nativePackages[`${platform}-${architecture}`];
  if (!packageName) {
    throw new Error(`Unsupported Badbox platform: ${platform}-${architecture}`);
  }
  return packageName;
}

const require = createRequire(import.meta.url);

/** Prefer the checkout build for development, then the installed optional package. */
export function loadNativeEngine(): NativeEngine {
  const attempts: unknown[] = [];
  try {
    return require("../../native/build/badbox.node") as NativeEngine;
  } catch (cause) {
    attempts.push(cause);
  }

  const packageName = nativePackageName(process.platform, process.arch);
  try {
    return require(packageName) as NativeEngine;
  } catch (cause) {
    attempts.push(cause);
  }

  throw new AggregateError(
    attempts,
    `Cannot load the Badbox native engine for ${process.platform}-${process.arch}. ` +
      `Expected optional package ${packageName}. In a source checkout, run bun run build:native.`,
  );
}
