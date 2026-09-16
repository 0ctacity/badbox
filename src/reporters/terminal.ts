import { iterateFindings, type ScanResult } from "../scanner/index.ts";

export function reportScan(result: ScanResult): void {
  for (const finding of iterateFindings(result)) {
    console.log(`${finding.file}:${finding.ownerStart}-${finding.ownerEnd} ` +
      `[${finding.rule.severity}] ${finding.rule.id}: ` +
      `${finding.observed} > ${finding.rule.threshold} ${finding.rule.evidenceSubject}`);
  }
  for (const diagnostic of result.diagnostics) {
    console.error(`${diagnostic.file}: ${diagnostic.message}`);
  }
  const returnedCount = result.findings.length / result.recordWidth;
  const returned = result.truncated ? ` (${returnedCount} returned)` : "";
  console.log(`${result.scannedFiles} files scanned, ${result.findingCount} findings${returned}, ` +
    `${result.diagnostics.length} diagnostics`);
}
