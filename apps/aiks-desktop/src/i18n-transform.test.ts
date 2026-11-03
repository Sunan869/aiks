import { describe, expect, it } from "vitest";
import ts from "typescript";
import { transformLocalizedTsx } from "../plugins/localize";

describe("i18next JSX bridge", () => {
  it("localizes only visible text and presentation attributes", () => {
    const sample = [
      'export function Example() {',
      '  return <section data-state="已处理" aria-label="详情">',
      '    <h1>知识库</h1><input placeholder="搜索…" value="raw" />',
      '    <div>{42} 条数据</div>',
      '  </section>;',
      '}',
    ].join("\n");
    const result = transformLocalizedTsx(sample, "/repo/apps/aiks-desktop/src/pages/Example.tsx");
    expect(result.code).toContain('tr("知识库")');
    expect(result.code).toContain('aria-label={tr("详情")}');
    expect(result.code).toContain('data-state="已处理"');
    expect(result.code).toContain('value="raw"');
    expect(result.code).toContain('tr("条数据")');
    expect(result.code).toContain('import { tr } from "../i18n"');
    const parsed = ts.transpileModule(result.code, { fileName: "Example.tsx", reportDiagnostics: true, compilerOptions: { jsx: ts.JsxEmit.ReactJSX } });
    expect((parsed.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error)).toHaveLength(0);
  });
  it("does not rewrite domain constants", () => {
    const input = 'const sourceId = "数据源"; export const View = () => <span title={"知识库"}>可检索</span>;';
    const result = transformLocalizedTsx(input, "/repo/apps/aiks-desktop/src/View.tsx");
    expect(result.code).toContain('sourceId = "数据源"');
    expect(result.code).toContain('title={tr("知识库")}');
    expect(result.code).toContain('tr("可检索")');
  });
});
