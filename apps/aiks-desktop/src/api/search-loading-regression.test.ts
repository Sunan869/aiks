import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("../components/UnifiedSearchDialog.tsx", import.meta.url), "utf8");

describe("unified search loading regression", () => {
  it("does not initialize an unknown semantic state as explicitly disabled", () => {
    const declaration = source.match(/const EMPTY: UnifiedSearchOutcome = (\{[^;]+\});/);
    expect(declaration).not.toBeNull();
    const empty = Function(`return (${declaration![1]})`)();
    expect(empty.semantic_enabled).not.toBe(false);
  });
});
