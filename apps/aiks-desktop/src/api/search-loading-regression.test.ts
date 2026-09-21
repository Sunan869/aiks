import source from "../components/UnifiedSearchDialog.tsx?raw";
import { describe, expect, it } from "vitest";

describe("unified search loading regression", () => {
  it("does not initialize an unknown semantic state as explicitly disabled", () => {
    const declaration = source.match(/const EMPTY: UnifiedSearchOutcome = (\{[^;]+\});/);
    expect(declaration).not.toBeNull();
    const empty = Function(`return (${declaration![1]})`)();
    expect(empty.semantic_enabled).not.toBe(false);
  });

  it("only displays the disabled notice for a completed successful current query", () => {
    const match = source.match(/\{(query\.trim\(\)[^\n]+?) && \(\n\s*<div className="border-b border-sky/);
    expect(match).not.toBeNull();
    const visible = Function("query", "outcome", "current", "loading", "error", `return Boolean(${match![1]})`);
    const disabled = { semantic_enabled: false, degraded: false };
    expect(visible("llm", disabled, true, true, null)).toBe(false);
    expect(visible("llm", disabled, true, false, "failed")).toBe(false);
    expect(visible("llm", disabled, false, false, null)).toBe(false);
    expect(visible("llm", disabled, true, false, null)).toBe(true);
    expect(visible("llm", {}, true, false, null)).toBe(false);
  });
});
