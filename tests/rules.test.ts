import { describe, expect, test } from "bun:test";

import { defineRule } from "../src/rules/index.ts";

describe("defineRule", () => {
  test("preserves an executable rule definition", () => {
    const inspect = () => {};
    const definition = {
      id: "react/multiple-effects",
      summary: "React component contains multiple useEffect hooks",
      severity: "warning" as const,
      appliesTo: {
        languages: ["tsx"] as const,
        frameworks: ["react"] as const,
      },
      inspect,
    };

    const rule = defineRule(definition);

    expect(rule).toBe(definition);
    expect(rule.inspect).toBe(inspect);
  });

  test("rejects rule IDs without a namespace", () => {
    expect(() =>
      defineRule({
        id: "multiple-effects",
        summary: "React component contains multiple useEffect hooks",
        severity: "warning",
        appliesTo: { languages: ["tsx"] },
        inspect() {},
      }),
    ).toThrow("namespace/name");
  });
});
