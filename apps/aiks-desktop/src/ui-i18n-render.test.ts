import { afterEach, describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { i18n } from "./i18n";
import { PageHeader } from "./components/ui/PageHeader";
import { UiMetric, UiNotice } from "./components/ui/surfaces";
import { UiEmptyState, UiStatusBadge } from "./components/ui/workspace";

afterEach(async () => { await i18n.changeLanguage("zh-CN"); });

describe("localized UI presentation (React SSR smoke checks)", () => {
  it("renders headers, empty states and badges in English", async () => {
    await i18n.changeLanguage("en-US");
    const html = renderToStaticMarkup(
      createElement("div", {},
        createElement(PageHeader, { title: "处理中心", description: "会话同步与 AI 任务状态" }),
        createElement(UiEmptyState, { title: "暂无数据" }),
        createElement(UiStatusBadge, { children: "处理中" }),
      ),
    );
    expect(html).toContain("Processing Center");
    expect(html).toContain("Session sync and AI task states");
    expect(html).toContain("No data");
    expect(html).toContain("Processing");
    expect(html).not.toContain("处理中心");
  });

  it("keeps user knowledge content unchanged when labels change language", async () => {
    const userText = "用户原始知识：处理中心不一定是项目名称";
    await i18n.changeLanguage("en-US");
    const html = renderToStaticMarkup(createElement(UiMetric, { label: "知识条目", value: userText }));
    expect(html).toContain("Knowledge Items");
    expect(html).toContain(userText);
  });

  it("retranslates authored notices without translating appended error details", async () => {
    await i18n.changeLanguage("en-US");
    let html = renderToStaticMarkup(createElement(UiNotice, { tone: "danger", children: "反馈加载失败：connection timeout" }));
    expect(html).toContain("Failed to load feedback: connection timeout");
    await i18n.changeLanguage("zh-CN");
    html = renderToStaticMarkup(createElement(UiNotice, { tone: "danger", children: "反馈加载失败：connection timeout" }));
    expect(html).toContain("反馈加载失败：connection timeout");
  });
});
