import { describe, expect, it } from "vitest";
import sessionsSource from "../pages/SessionsPage.tsx?raw";
import sessionDetailSource from "../pages/SessionDetailPage.tsx?raw";
import processingSource from "../pages/ProcessingPage.tsx?raw";
import processingDetailSource from "../pages/ProcessingDetailPage.tsx?raw";
import knowledgeSource from "../pages/KnowledgePage.tsx?raw";
import sourcesSource from "../pages/SourcesPage.tsx?raw";

describe("WorkBuddy display boundaries", () => {
  it.each([
    ["sessions", sessionsSource, "formatSourceName(item.source)"],
    ["session detail", sessionDetailSource, "formatSourceName(session.source)"],
    ["processing", processingSource, "formatSourceName(run.source)"],
    ["processing detail", processingDetailSource, "formatSourceName(detail.source)"],
    ["knowledge", knowledgeSource, "formatSourceName(item.source)"],
  ])("formats the provider label in %s", (_name, source, expression) => {
    expect(source).toContain(expression);
  });

  it("keeps sync and filter values as provider IDs", () => {
    expect(sourcesSource).toMatch(/"WorkBuddy"\s*:\s*"workbuddy"/);
    expect(sessionsSource).toContain("value={s}");
    expect(sessionsSource).toContain("source: source || undefined");
  });
});
