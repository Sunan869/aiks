import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { fileURLToPath } from "node:url";

const project = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const src = path.join(project, "src");
const zh = JSON.parse(fs.readFileSync(path.join(src, "locales/zh-CN.json"), "utf8"));
const en = JSON.parse(fs.readFileSync(path.join(src, "locales/en-US.json"), "utf8"));
const hasChinese = text => /[\u3400-\u9fff]/u.test(text);
const visibleAttributes = new Set([
  "title", "label", "description", "desc", "placeholder", "alt", "aria-label",
  "aria-description", "tooltip", "hint", "emptyText", "emptyLabel",
  "helpText", "helperText", "confirmText", "cancelText",
]);

function* walkFiles(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const name = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walkFiles(name);
    else if (entry.name.endsWith(".tsx") && !entry.name.includes(".test.")) yield name;
  }
}
const required = new Map();
function track(key, file, location) {
  // Normalize Windows CRLF checkouts before looking up LF-based JSON keys.
  // The Vite JSX bridge uses the same normalization for actual UI rendering.
  key = key.replace(/\r\n?/g, "\n");
  if (!hasChinese(key)) return;
  if (!required.has(key)) required.set(key, []);
  required.get(key).push(path.relative(project, file) + ":" + location);
}
for (const file of walkFiles(src)) {
  const source = ts.createSourceFile(file, fs.readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  function inspectVisibleBranch(expr) {
    if (ts.isParenthesizedExpression(expr)) { inspectVisibleBranch(expr.expression); return; }
    if (ts.isConditionalExpression(expr)) {
      inspectVisibleBranch(expr.whenTrue);
      inspectVisibleBranch(expr.whenFalse);
      return;
    }
    if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) {
      track(expr.text, file, source.getLineAndCharacterOfPosition(expr.getStart(source)).line + 1);
      return;
    }
    if (ts.isTemplateExpression(expr)) {
      let key = expr.head.text;
      expr.templateSpans.forEach((part, i) => { key += "{{v" + i + "}}" + part.literal.text; });
      track(key, file, source.getLineAndCharacterOfPosition(expr.getStart(source)).line + 1);
    }
  }
  function visit(node) {
    if (ts.isJsxText(node)) {
      track(node.getFullText(source).trim(), file, source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1);
      return;
    }
    if (ts.isJsxAttribute(node) && node.initializer && visibleAttributes.has(node.name.getText(source))) {
      const initializer = node.initializer;
      if (ts.isStringLiteral(initializer)) {
        track(initializer.text, file, source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1);
        return;
      }
      if (ts.isJsxExpression(initializer) && initializer.expression) {
        const expr = initializer.expression;
        if (ts.isStringLiteral(expr)) {
          track(expr.text, file, source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1);
          return;
        }
        if (ts.isTemplateExpression(expr)) {
          let key = expr.head.text;
          expr.templateSpans.forEach((part, i) => { key += "{{v" + i + "}}" + part.literal.text; });
          track(key, file, source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1);
          return;
        }
      }
    }
    if (ts.isJsxExpression(node) && node.expression) {
      const attr = ts.isJsxAttribute(node.parent) ? node.parent : null;
      if (!attr || visibleAttributes.has(attr.name.getText(source))) {
        inspectVisibleBranch(node.expression);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
const missingChinese = [...required.entries()].filter(([key]) => !Object.hasOwn(zh, key));
const missingEnglish = [...required.entries()].filter(([key]) => !Object.hasOwn(en, key));
const divergent = [...new Set([...Object.keys(en), ...Object.keys(zh)])].filter(key => !Object.hasOwn(en, key) || !Object.hasOwn(zh, key));
const invalid = Object.entries(en).filter(([k,v]) => typeof v !== "string" || !v.trim());
if (missingEnglish.length || missingChinese.length || divergent.length || invalid.length) {
  for (const [kind, errors] of [
    ["Missing English translation", missingEnglish],
    ["Missing Chinese fallback", missingChinese],
  ]) {
    for (const [key, places] of errors.slice(0, 120)) {
      console.error(kind + ": " + JSON.stringify(key) + " in " + places[0]);
    }
  }
  for (const key of divergent.slice(0, 20)) console.error("Locale key mismatch: " + JSON.stringify(key));
  for (const [key] of invalid.slice(0, 20)) console.error("Empty/invalid English translation: " + JSON.stringify(key));
  console.error("i18n check FAILED: " + missingEnglish.length + " visible English keys missing, " + missingChinese.length + " Chinese keys missing, " + divergent.length + " key mismatches, " + invalid.length + " invalid translations.");
  process.exitCode = 1;
} else {
  console.log("i18n check OK: " + required.size + " distinct visible UI phrases, " + Object.keys(en).length + " locale keys; English/Chinese parity verified.");
}
