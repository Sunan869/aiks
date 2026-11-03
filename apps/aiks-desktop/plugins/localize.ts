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
// Git checkout uses CRLF on Windows and LF on Linux/macOS. Translation keys
// must not depend on the host checkout's line-ending settings.
const normalizeI18nKey = (value: string) => value.replace(/\r\n?/g, "\n");
const jsString = (value: string) => JSON.stringify(normalizeI18nKey(value));

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
    keys.push(normalizeI18nKey(key));
  };
  function localizeVisibleBranch(expr: ts.Expression): void {
    if (ts.isParenthesizedExpression(expr)) { localizeVisibleBranch(expr.expression); return; }
    if (ts.isConditionalExpression(expr)) {
      localizeVisibleBranch(expr.whenTrue);
      localizeVisibleBranch(expr.whenFalse);
      return;
    }
    if ((ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) && hasHan(expr.text)) {
      add(expr.getStart(source), expr.end, "tr(" + jsString(expr.text) + ")", expr.text);
      return;
    }
    if (ts.isTemplateExpression(expr)) {
      let key = expr.head.text;
      expr.templateSpans.forEach((part, i) => { key += "{{v" + i + "}}" + part.literal.text; });
      if (hasHan(key)) add(expr.getStart(source), expr.end, templateTranslation(expr, source), key);
    }
  }
  function walk(node: ts.Node): void {
    if (ts.isJsxText(node)) {
      const raw = node.getFullText(source);
      const sourceText = raw.trim();
      if (hasHan(sourceText)) {
        // Match offsets against the original buffer, not the normalized key:
        // otherwise CRLF input shifts JSX replacement boundaries.
        const ix = raw.indexOf(sourceText);
        add(node.getFullStart(), node.end,
          raw.slice(0, ix) + "{tr(" + jsString(sourceText) + ")}" + raw.slice(ix + sourceText.length),
          sourceText);
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
    if (ts.isJsxExpression(node) && node.expression) {
      const attr = ts.isJsxAttribute(node.parent) ? node.parent : null;
      if (!attr || visibleAttributes.has(attr.name.getText(source))) {
        localizeVisibleBranch(node.expression);
      }
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
  if (!/import\s+\{[^}]*\btr\b[^}]*\}\s+from\s+['"][^'"]+i18n['"]/.test(code)) {
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
