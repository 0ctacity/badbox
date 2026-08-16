import type { RuleDefinition } from "./types.ts";

const ruleIdPattern = /^[a-z][a-z0-9-]*\/[a-z][a-z0-9-]*$/;

export type {
  Ast,
  AstNode,
  CountEvidence,
  Finding,
  FindingEvidence,
  FindingInput,
  Framework,
  Language,
  RuleApplicability,
  RuleContext,
  RuleDefinition,
  Severity,
  SourceFile,
  SourceLocation,
} from "./types.ts";

export function defineRule<const T extends RuleDefinition>(definition: T): T {
  if (!ruleIdPattern.test(definition.id)) {
    throw new TypeError(
      `Invalid rule ID "${definition.id}": expected lowercase namespace/name`,
    );
  }

  return definition;
}

export const builtInRules: readonly RuleDefinition[] = [];
