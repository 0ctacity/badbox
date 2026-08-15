export { detectProject } from "./detect.ts";

export interface ScanOptions {
  readonly cwd: string;
}

export interface ScanSummary {
  readonly scannedFiles: number;
  readonly findingCount: number;
}
