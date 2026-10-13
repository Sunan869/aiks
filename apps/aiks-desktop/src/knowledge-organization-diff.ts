/**
 * S3.4 audit preview: bounded, deterministic line-by-line diff.
 * This is ONLY a display of source-versus-new independent draft. It never
 * modifies the original or gives authority to apply edits to that document.
 */
export type OrganizationDiffKind = "same" | "added" | "removed";

export interface OrganizationDiffRow {
  kind: OrganizationDiffKind;
  text: string;
}

export interface OrganizationDiffPreview {
  rows: OrganizationDiffRow[];
  added: number;
  removed: number;
  truncated: boolean;
  identical: boolean;
}

const MAX_INPUT_CHARS = 64_000;
const MAX_INPUT_LINES = 320;
const MAX_OUTPUT_ROWS = 500;
const MAX_DISPLAY_LINE_CHARS = 800;

function excerpt(text: string): { lines: string[]; truncated: boolean } {
  // Bounded before split to prevent huge arrays on malicious model output.
  const normalized = text.slice(0, MAX_INPUT_CHARS).replace(/\r\n?/g, "\n");
  const all = normalized.split("\n");
  const clipped = all.slice(0, MAX_INPUT_LINES);
  return {
    lines: clipped,
    truncated: text.length > MAX_INPUT_CHARS || all.length > MAX_INPUT_LINES,
  };
}

/** Exact LCS on the bounded prefix of each input, not a false approximate diff. */
export function previewKnowledgeOrganizationDiff(
  original: string,
  draft: string,
): OrganizationDiffPreview {
  const before = excerpt(original);
  const after = excerpt(draft);
  const left = before.lines;
  const right = after.lines;
  const width = right.length + 1;
  const lcs = new Uint16Array((left.length + 1) * width);
  // Complexity <= 320*320 cells. Model output may be much larger, but only
  // the explicitly labeled preview prefix is compared.
  for (let i = left.length - 1; i >= 0; i--) {
    for (let j = right.length - 1; j >= 0; j--) {
      lcs[i * width + j] = left[i] === right[j]
        ? lcs[(i + 1) * width + j + 1] + 1
        : Math.max(lcs[(i + 1) * width + j], lcs[i * width + j + 1]);
    }
  }

  const rows: OrganizationDiffRow[] = [];
  let added = 0, removed = 0;
  let clipped = before.truncated || after.truncated;
  let i = 0, j = 0;
  while ((i < left.length || j < right.length) && rows.length < MAX_OUTPUT_ROWS) {
    let kind: OrganizationDiffKind;
    let line: string;
    if (i < left.length && j < right.length && left[i] === right[j]) {
      kind = "same";
      line = left[i++];
      j++;
    } else if (j < right.length &&
      (i === left.length || lcs[i * width + j + 1] > lcs[(i + 1) * width + j])) {
      kind = "added";
      line = right[j++];
      added++;
    } else {
      kind = "removed";
      line = left[i++];
      removed++;
    }
    const chars = [...line];
    if (chars.length > MAX_DISPLAY_LINE_CHARS) clipped = true;
    rows.push({
      kind,
      text: chars.length > MAX_DISPLAY_LINE_CHARS
        ? chars.slice(0, MAX_DISPLAY_LINE_CHARS).join("") + "…（本行剩余内容未展示）"
        : line,
    });
  }
  if (i < left.length || j < right.length) clipped = true;
  return {
    rows, added, removed, truncated: clipped,
    identical: original.replace(/\r\n?/g, "\n") === draft.replace(/\r\n?/g, "\n"),
  };
}
