/**
 * Shared markdown → structured blocks (headings, lists, paragraphs, pipe tables).
 * Used by DOCX export and Word clipboard HTML so tables/formatting stay aligned.
 */

export type MarkdownBlock =
  | { type: "heading"; level: 1 | 2 | 3; text: string }
  | { type: "bullet"; text: string }
  | { type: "numbered"; number: number; text: string }
  | { type: "paragraph"; lines: string[] }
  | { type: "table"; rows: string[][] };

export function parseMarkdownBlocks(markdown: string): MarkdownBlock[] {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i].trim();
    if (!line) {
      i++;
      continue;
    }

    if (line.startsWith("### ")) {
      blocks.push({ type: "heading", level: 3, text: line.slice(4).trim() });
      i++;
      continue;
    }
    if (line.startsWith("## ")) {
      blocks.push({ type: "heading", level: 2, text: line.slice(3).trim() });
      i++;
      continue;
    }
    if (line.startsWith("# ")) {
      blocks.push({ type: "heading", level: 1, text: line.slice(2).trim() });
      i++;
      continue;
    }

    if (line.startsWith("|")) {
      const tableRows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith("|")) {
        const tl = lines[i].trim();
        if (/^\|[\s\-:|]+\|$/.test(tl)) {
          i++;
          continue;
        }
        const cells = tl
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((c) => c.trim());
        if (cells.some((c) => c.length > 0)) tableRows.push(cells);
        i++;
      }
      if (tableRows.length > 0) blocks.push({ type: "table", rows: tableRows });
      continue;
    }

    if (line.startsWith("- ") || line.startsWith("* ")) {
      blocks.push({ type: "bullet", text: line.slice(2).trim() });
      i++;
      continue;
    }

    const numMatch = line.match(/^(\d+)\.\s+(.+)$/);
    if (numMatch) {
      blocks.push({
        type: "numbered",
        number: parseInt(numMatch[1], 10),
        text: numMatch[2].trim(),
      });
      i++;
      continue;
    }

    const hardLines: string[] = [];
    let accumulator = "";

    const flushAccumulator = () => {
      if (accumulator.trim()) hardLines.push(accumulator.trim());
      accumulator = "";
    };

    const processRawLine = (rawLine: string) => {
      if (rawLine.endsWith("\\")) {
        accumulator += (accumulator ? " " : "") + rawLine.slice(0, -1).trimEnd();
        flushAccumulator();
      } else {
        accumulator += (accumulator ? " " : "") + rawLine;
      }
    };

    processRawLine(line);
    i++;
    while (i < lines.length) {
      const next = lines[i].trim();
      if (
        !next ||
        next.startsWith("#") ||
        next.startsWith("|") ||
        next.startsWith("- ") ||
        next.startsWith("* ") ||
        /^\d+\.\s/.test(next)
      )
        break;
      processRawLine(next);
      i++;
    }
    flushAccumulator();

    if (hardLines.length > 0) {
      blocks.push({ type: "paragraph", lines: hardLines });
    }
  }

  return blocks;
}
