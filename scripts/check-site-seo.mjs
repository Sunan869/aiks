import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../docs");
const base = "https://sunan869.github.io/aiks/";
const slugs = ["claude-code-session-history", "codex-cli-session-history", "cursor-composer-history"];
const pairs = [["", "en/"], ["guides/", "en/guides/"], ...slugs.map(slug => ["guides/" + slug + "/", "en/guides/" + slug + "/"])];
const known = pairs.flatMap(pair => pair);
const seenTitles = new Set();
const sitemap = fs.readFileSync(path.join(root, "sitemap.xml"), "utf8");
let problems = [];
function check(value, description) { if (!value) problems.push(description); }
for (const [zh, en] of pairs) {
  for (const current of [zh, en]) {
    const abs = path.join(root, current, "index.html");
    check(fs.existsSync(abs), current + " missing index.html");
    if (!fs.existsSync(abs)) continue;
    const html = fs.readFileSync(abs, "utf8");
    const full = base + current;
    const title = /<title>([^<]+)<\/title>/.exec(html)?.[1];
    check(title && !seenTitles.has(title), full + " missing or duplicate title");
    seenTitles.add(title);
    check(/<meta name="description" content="[^"]{60,}"/.test(html), full + " missing substantial meta description");
    check((html.match(/<h1(?:\s[^>]*)?>/g) || []).length === 1, full + " needs one H1");
    check(html.includes('<link rel="canonical" href="' + full + '">'), full + " incorrect canonical");
    check(html.includes('hreflang="zh-CN" href="' + base + zh + '"'), full + " incorrect zh alternate");
    check(html.includes('hreflang="en" href="' + base + en + '"'), full + " incorrect en alternate");
    check(sitemap.includes("<loc>" + full + "</loc>"), full + " absent from sitemap");
    for (const [,raw] of html.matchAll(/\bhref="([^"]+)"/g)) {
      if (raw.startsWith("#") || raw.startsWith("mailto:")) continue;
      const target = new URL(raw.replaceAll("&amp;", "&"), full);
      if (target.origin !== new URL(base).origin || !target.pathname.startsWith("/aiks/")) continue;
      const segment = decodeURIComponent(target.pathname.slice("/aiks/".length));
      const file = path.join(root, segment);
      const actual = path.extname(file) ? file : path.join(file, "index.html");
      check(fs.existsSync(actual), full + " broken internal link " + raw);
    }
  }
}
const locs=[...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map(m=>m[1]);
check(locs.length===known.length && new Set(locs).size===locs.length, "sitemap duplicates or unexpected number of URLs");
if(problems.length){for(const problem of problems)console.error(problem);process.exitCode=1;}
else console.log("SEO check OK: "+known.length+" canonical bilingual pages, reciprocal hreflang, sitemap and internal links");
