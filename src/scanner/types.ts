export type CheckLanguage =
  | "bash"
  | "c"
  | "cpp"
  | "csharp"
  | "css"
  | "dart"
  | "elixir"
  | "go"
  | "haskell"
  | "hcl"
  | "html"
  | "java"
  | "javascript"
  | "json"
  | "kotlin"
  | "lua"
  | "markdown"
  | "nix"
  | "php"
  | "powershell"
  | "python"
  | "ruby"
  | "rust"
  | "scala"
  | "solidity"
  | "swift"
  | "tsx"
  | "typescript"
  | "yaml"
  | "zig";
export type CheckSeverity = "info" | "warning" | "error";

export const findingRecordWidth = 5;

export const findingRecord = {
  fileId: 0,
  ruleId: 1,
  ownerStart: 2,
  ownerEnd: 3,
  observed: 4,
} as const;

export interface RuleMetadata {
  readonly id: string;
  readonly language: CheckLanguage;
  readonly summary: string;
  readonly message: string;
  readonly severity: CheckSeverity;
  readonly threshold: number;
  readonly evidenceSubject: string;
}

export interface InspectOptions {
  readonly paths: readonly string[];
  /** One or more rule files or recursively loaded rule-pack directories. */
  readonly rulePaths: readonly string[];
  /** Rule-local scalar overrides keyed as "namespace/rule.parameter". */
  readonly parameters?: Readonly<Record<string, string | number | boolean>>;
  /** Overrides each rule's strict greater-than threshold. */
  readonly threshold?: number;
  /** Include diagnostic phase timings and cache/execution counters. */
  readonly profile?: boolean;
  /** Maximum fixed-width finding records returned. Exact findingCount remains available. */
  readonly maxFindings?: number;
}

export interface PerformanceProfile {
  readonly ruleLoadMs: number;
  readonly discoveryMs: number;
  readonly readMs: number;
  readonly parseMs: number;
  readonly matchingMs: number;
  readonly ownershipMs: number;
  readonly evaluationMs: number;
  readonly aggregationMs: number;
  readonly outputBuildMs: number;
  readonly resultMergeMs: number;
  readonly serializationMs: number;
  readonly jsDecodeMs: number;
  readonly selectorExecutions: number;
  readonly ruleEvaluations: number;
  readonly ruleCacheHits: number;
  readonly ruleCacheMisses: number;
  readonly parseCacheHits: number;
  readonly parseCacheMisses: number;
  readonly resultCacheHits: number;
  readonly resultCacheMisses: number;
  readonly workerThreads: number;
}

export interface CheckResult {
  readonly recordWidth: typeof findingRecordWidth;
  readonly languages: readonly CheckLanguage[];
  readonly files: readonly string[];
  readonly rules: readonly RuleMetadata[];
  readonly checkedFiles: number;
  readonly findingCount: number;
  /** Flat records: fileId, ruleId, ownerStart, ownerEnd, observed. */
  readonly findings: Uint32Array;
  readonly truncated: boolean;
  readonly diagnostics: readonly { readonly file: string; readonly message: string }[];
  readonly performance?: PerformanceProfile;
}

export interface FindingRecord {
  readonly fileId: number;
  readonly ruleId: number;
  readonly file: string;
  readonly rule: RuleMetadata;
  readonly ownerStart: number;
  readonly ownerEnd: number;
  readonly observed: number;
}
