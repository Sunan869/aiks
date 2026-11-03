import ts from "typescript";
import type { Plugin } from "vite";

/** Compile-time bridge for existing TSX presentation only.
 * Never rewrite API payloads, persisted IDs, provider keys, user content or prompts.
 * Runtime translations are provided by i18next through src/i18n.ts.
 */
const visibleAttributes = new Set([
  "title", "label", "description", "desc", "placeholder", "alt",
  "aria-label", "aria-description", "tooltip", "hint", "emptyText",
  "emptyLabel", "helpText", "helperText", "confirmText", "cancelText",
]);
interface Edit { start: number; end: number; replacement: string }
const hasHan = (value: string) => /[\u3400-\u9fff]/u.test(value);
const jsString = (value: string) => JSON.stringify(value);

function templateTranslation(expr: ts.TemplateExpression, source: ts.SourceFile): string {
  let key = expr.head.text;
  const values: string[] = [];
  expr.templateSpans.forEach((span, idx) => {
    const name = "v" + idx;
    key += "{{" + name + "}}" + span.literal.text;
    values.push(name + ": (" + span.expression.getText(source) + ")");
  });
  return "tr(" + jsString(key) + ", { " + values.join(", ") + " })";
}

export function transformLocalizedTsx(input: string, id: string): { code: string; keys: string[] } {
  const source = ts.createSourceFile(id, input, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const edits: Edit[] = [];
  const keys: string[] = [];
  const add = (start: number, end: number, replacement: string, key: string) => {
    edits.push({ start, end, replacement });
    keys.push(key);
  };
  function walk(node: ts.Node): void {
    if (ts.isJsxText(node)) {
      const raw = node.getFullText(source);
      const text = raw.trim();
      if (hasHan(text)) {
        const ix = raw.indexOf(text);
        add(node.getFullStart(), node.end, raw.slice(0, ix) + "{tr(" + jsString(text) + ")}" + raw.slice(ix + text.length), text);
      }
      return;
    }
    if (ts.isJsxAttribute(node) && node.initializer && visibleAttributes.has(node.name.getText(source))) {
      const init = node.initializer;
      if (ts.isStringLiteral(init) && hasHan(init.text)) {
        add(init.getStart(source), init.end, "{tr(" + jsString(init.text) + ")}", init.text);
        return;
      }
      if (ts.isJsxExpression(init) && init.expression) {
        const e = init.expression;
        if (ts.isStringLiteral(e) && hasHan(e.text)) {
          add(e.getStart(source), e.end, "tr(" + jsString(e.text) + ")", e.text);
          return;
        }
        if (ts.isTemplateExpression(e)) {
          let key = e.head.text;
          e.templateSpans.forEach((span, i) => { key += "{{v" + i + "}}" + span.literal.text; });
          if (hasHan(key)) {
            add(e.getStart(source), e.end, templateTranslation(e, source), key);
            return;
          }
        }
      }
    }
    if (ts.isJsxExpression(node) && node.expression && ts.isStringLiteral(node.expression) && hasHan(node.expression.text)) {
      const e = node.expression;
      add(e.getStart(source), e.end, "tr(" + jsString(e.text) + ")", e.text);
      return;
    }
    ts.forEachChild(node, walk);
  }
  walk(source);
  if (!edits.length) return { code: input, keys: [] };
  edits.sort((a, b) => b.start - a.start);
  let code = input;
  for (const e of edits) code = code.slice(0, e.start) + e.replacement + code.slice(e.end);

  const relativeFile = id.replace(/\\/g, "/").split("/src/")[1] || id;
  const depth = Math.max(0, relativeFile.split("/").length - 1);
  const importPath = (depth === 0 ? "./" : "../".repeat(depth)) + "i18n";
  if (!/import\s+\{\s*tr\s*\}\s+from\s+['"][^'"]+i18n['"]/.test(code)) {
    code = 'import { tr } from "' + importPath + '";\n' + code;
  }
  return { code, keys: [...new Set(keys)] };
}

export function aiksUiI18nPlugin(): Plugin {
  return {
    name: "aiks-legacy-jsx-i18next",
    enforce: "pre",
    transform(input, id) {
      const clean = id.split("?")[0].replace(/\\/g, "/");
      if (!clean.endsWith(".tsx") || !clean.includes("/src/") || clean.includes(".test.")) return;
      const result = transformLocalizedTsx(input, clean);
      return result.keys.length ? { code: result.code, map: null } : undefined;
    },
  };
}
