export type Severity = "info" | "warning" | "error";

export interface SourceLocation {
  readonly line: number;
  readonly column: number;
}

export interface FindingEvidence {
  readonly message: string;
  readonly observed: number;
  readonly threshold: number;
}

export interface Finding {
  readonly ruleId: string;
  readonly file: string;
  readonly location: SourceLocation;
  readonly evidence: FindingEvidence;
  readonly severity: Severity;
}

export interface Rule {
  readonly id: string;
  readonly severity: Severity;
}
