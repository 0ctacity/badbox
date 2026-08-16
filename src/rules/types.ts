export type Severity = "info" | "warning" | "error";

export type Language = "javascript" | "jsx" | "typescript" | "tsx";

export type Framework = "react";

export interface SourceLocation {
  readonly line: number;
  readonly column: number;
}

export interface CountEvidence {
  readonly kind: "count";
  readonly subject: string;
  readonly observed: number;
  readonly threshold: number;
}

export type FindingEvidence = CountEvidence;

export interface Finding {
  readonly ruleId: string;
  readonly file: string;
  readonly location: SourceLocation;
  readonly message: string;
  readonly evidence: FindingEvidence;
  readonly severity: Severity;
}

export interface AstNode {
  readonly text: string;
  readonly location: SourceLocation;
  findAll(pattern: string): readonly AstNode[];
  getCapture(name: string): AstNode | undefined;
}

export type Ast = AstNode;

export interface SourceFile {
  readonly path: string;
  readonly language: Language;
  readonly source: string;
}

export interface FindingInput {
  readonly at: AstNode;
  readonly message: string;
  readonly evidence: FindingEvidence;
}

export interface RuleApplicability {
  readonly languages: readonly Language[];
  readonly files?: readonly string[];
  readonly frameworks?: readonly Framework[];
}

export interface RuleContext {
  readonly file: SourceFile;
  readonly ast: Ast;
  report(finding: FindingInput): void;
}

export interface RuleDefinition {
  readonly id: string;
  readonly summary: string;
  readonly severity: Severity;
  readonly appliesTo: RuleApplicability;
  inspect(context: RuleContext): void;
}
