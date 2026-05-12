import type { MarkdownBlock } from "./markdownBlockParser";
import { parseMarkdownBlocks } from "./markdownBlockParser";

/**
 * Write both text/html and text/plain to clipboard so that:
 *   - Paste into Word / Google Docs → uses HTML (preserves formatting)
 *   - Paste into plain text editor → uses plain text
 */
export async function copyRichText(html: string, plain: string): Promise<void> {
  const htmlBlob = new Blob([html], { type: 'text/html' });
  const textBlob = new Blob([plain], { type: 'text/plain' });
  await navigator.clipboard.write([
    new ClipboardItem({ 'text/html': htmlBlob, 'text/plain': textBlob }),
  ]);
}

// ─── Word-compatible styles ───────────────────────────────────────────────────
const FONT = "Calibri, 'Segoe UI', Arial, sans-serif";
const BASE_STYLE = `font-family:${FONT};font-size:11pt;color:#1a1a1a;line-height:1.5;`;

const STYLES = {
  body: `margin:0;padding:16pt 20pt;${BASE_STYLE}`,
  h1: `font-family:${FONT};font-size:18pt;font-weight:bold;color:#111;margin:16pt 0 6pt;`,
  h2: `font-family:${FONT};font-size:14pt;font-weight:bold;color:#1a1a1a;margin:12pt 0 4pt;`,
  h3: `font-family:${FONT};font-size:12pt;font-weight:bold;color:#333;margin:8pt 0 3pt;`,
  p: `font-family:${FONT};font-size:11pt;margin:4pt 0;`,
  ul: `margin:4pt 0 4pt 18pt;padding:0;`,
  ol: `margin:4pt 0 4pt 18pt;padding:0;`,
  li: `font-family:${FONT};font-size:11pt;margin:2pt 0;`,
  hr: `border:none;border-top:1pt solid #ccc;margin:10pt 0;`,
  blockquote: `border-left:3pt solid #ccc;margin:6pt 0 6pt 12pt;padding:2pt 0 2pt 8pt;color:#555;font-style:italic;`,
  code: `font-family:'Courier New',monospace;font-size:10pt;background:#f5f5f5;padding:1pt 4pt;border-radius:2pt;`,
  pre: `font-family:'Courier New',monospace;font-size:10pt;background:#f5f5f5;padding:8pt;margin:6pt 0;white-space:pre-wrap;`,
  strong: `font-weight:bold;`,
  em: `font-style:italic;`,
};

// ─── Markdown → HTML ──────────────────────────────────────────────────────────

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function inlineMarkdown(text: string): string {
  return escHtml(text)
    .replace(/\*\*\*(.+?)\*\*\*/g, `<strong style="${STYLES.strong}"><em style="${STYLES.em}">$1</em></strong>`)
    .replace(/\*\*(.+?)\*\*/g, `<strong style="${STYLES.strong}">$1</strong>`)
    .replace(/__(.+?)__/g, `<strong style="${STYLES.strong}">$1</strong>`)
    .replace(/\*(.+?)\*/g, `<em style="${STYLES.em}">$1</em>`)
    .replace(/_(.+?)_/g, `<em style="${STYLES.em}">$1</em>`)
    .replace(/`(.+?)`/g, `<code style="${STYLES.code}">$1</code>`);
}

/**
 * Converts a markdown string to Word-compatible HTML (inline styles, no CSS classes).
 * Handles headings, bold/italic, ordered/unordered lists, blockquotes, code, hr.
 */
export function markdownToWordHtml(markdown: string): string {
  const lines = markdown.split('\n');
  const parts: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    if (line.startsWith('```')) {
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith('```')) {
        codeLines.push(escHtml(lines[i]));
        i++;
      }
      parts.push(`<pre style="${STYLES.pre}">${codeLines.join('\n')}</pre>`);
      i++;
      continue;
    }

    // Headings
    const h3 = line.match(/^###\s+(.*)/);
    const h2 = line.match(/^##\s+(.*)/);
    const h1 = line.match(/^#\s+(.*)/);
    if (h3) { parts.push(`<h3 style="${STYLES.h3}">${inlineMarkdown(h3[1])}</h3>`); i++; continue; }
    if (h2) { parts.push(`<h2 style="${STYLES.h2}">${inlineMarkdown(h2[1])}</h2>`); i++; continue; }
    if (h1) { parts.push(`<h1 style="${STYLES.h1}">${inlineMarkdown(h1[1])}</h1>`); i++; continue; }

    // Horizontal rule
    if (/^---+$/.test(line.trim())) {
      parts.push(`<hr style="${STYLES.hr}">`);
      i++;
      continue;
    }

    // Blockquote
    if (line.startsWith('> ')) {
      parts.push(`<blockquote style="${STYLES.blockquote}">${inlineMarkdown(line.slice(2))}</blockquote>`);
      i++;
      continue;
    }

    // Unordered list — collect contiguous items
    if (/^[-*+] /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*+] /.test(lines[i])) {
        items.push(`<li style="${STYLES.li}">${inlineMarkdown(lines[i].replace(/^[-*+] /, ''))}</li>`);
        i++;
      }
      parts.push(`<ul style="${STYLES.ul}">${items.join('')}</ul>`);
      continue;
    }

    // Ordered list — collect contiguous items
    if (/^\d+\. /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\. /.test(lines[i])) {
        items.push(`<li style="${STYLES.li}">${inlineMarkdown(lines[i].replace(/^\d+\. /, ''))}</li>`);
        i++;
      }
      parts.push(`<ol style="${STYLES.ol}">${items.join('')}</ol>`);
      continue;
    }

    // Empty line → skip (paragraph spacing handled by margins)
    if (line.trim() === '') {
      i++;
      continue;
    }

    // Regular paragraph
    parts.push(`<p style="${STYLES.p}">${inlineMarkdown(line)}</p>`);
    i++;
  }

  return parts.join('\n');
}

/** Inline borders / padding aligned with DOCX table styling */
const TABLE_WRAP = `border-collapse:collapse;width:100%;margin:8pt 0;`;
const CELL_BORDER = `border:1pt solid #CCCCCC;`;
const TH_CELL = `${CELL_BORDER}padding:4pt 6pt;background:#E8E8E8;font-weight:bold;${BASE_STYLE}`;
const TD_CELL = `${CELL_BORDER}padding:4pt 6pt;background:#FFFFFF;${BASE_STYLE}`;

function stripHeadingInlineMarkers(text: string): string {
  return text.replace(/\*+([^*]+)\*+/g, "$1").replace(/`([^`]+)`/g, "$1");
}

/**
 * Renders structured blocks (same parser as DOCX export) as inline-styled HTML for Word paste.
 */
export function markdownBlocksToWordHtml(blocks: MarkdownBlock[]): string {
  const parts: string[] = [];
  let i = 0;

  while (i < blocks.length) {
    const b = blocks[i];

    if (b.type === "bullet") {
      const items: string[] = [];
      while (i < blocks.length && blocks[i].type === "bullet") {
        items.push(`<li style="${STYLES.li}">${inlineMarkdown((blocks[i] as Extract<MarkdownBlock, { type: "bullet" }>).text)}</li>`);
        i++;
      }
      parts.push(`<ul style="${STYLES.ul}">${items.join("")}</ul>`);
      continue;
    }

    if (b.type === "numbered") {
      const items: string[] = [];
      while (i < blocks.length && blocks[i].type === "numbered") {
        items.push(`<li style="${STYLES.li}">${inlineMarkdown((blocks[i] as Extract<MarkdownBlock, { type: "numbered" }>).text)}</li>`);
        i++;
      }
      parts.push(`<ol style="${STYLES.ol}">${items.join("")}</ol>`);
      continue;
    }

    if (b.type === "heading") {
      const raw = stripHeadingInlineMarkers(b.text);
      const tag = b.level === 1 ? "h1" : b.level === 2 ? "h2" : "h3";
      const style = b.level === 1 ? STYLES.h1 : b.level === 2 ? STYLES.h2 : STYLES.h3;
      parts.push(`<${tag} style="${style}">${inlineMarkdown(raw)}</${tag}>`);
      i++;
      continue;
    }

    if (b.type === "paragraph") {
      const inner = b.lines
        .map((line) => inlineMarkdown(line))
        .join("<br />");
      parts.push(`<p style="${STYLES.p}">${inner}</p>`);
      i++;
      continue;
    }

    if (b.type === "table") {
      const maxCols = Math.max(...b.rows.map((r) => r.length), 0);
      const rowsHtml = b.rows
        .map((row, rowIdx) => {
          const cells = Array.from({ length: maxCols }, (_, colIdx) => {
            const cell = row[colIdx] ?? "";
            const isHeader = rowIdx === 0;
            const tag = isHeader ? "th" : "td";
            const cellStyle = isHeader ? TH_CELL : TD_CELL;
            const content = isHeader
              ? `<strong style="${STYLES.strong}">${inlineMarkdown(cell)}</strong>`
              : inlineMarkdown(cell);
            return `<${tag} style="${cellStyle}" valign="top">${content}</${tag}>`;
          });
          return `<tr>${cells.join("")}</tr>`;
        })
        .join("");
      parts.push(`<table style="${TABLE_WRAP}" cellspacing="0" cellpadding="0">${rowsHtml}</table>`);
      parts.push(`<p style="${STYLES.p};margin:2pt 0 8pt;">&nbsp;</p>`);
      i++;
      continue;
    }

    i++;
  }

  return parts.join("\n");
}

/**
 * Markdown → Word clipboard body (tables with borders, same block model as DOCX export).
 */
export function markdownToWordHtmlFromParser(markdown: string): string {
  return markdownBlocksToWordHtml(parseMarkdownBlocks(markdown));
}

/**
 * Wrap converted body HTML in a full document with Word-compatible meta tags.
 */
export function wrapWordHtml(bodyHtml: string): string {
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name=ProgId content=Word.Document>
<style>
  body { ${STYLES.body} }
</style>
</head>
<body>${bodyHtml}</body>
</html>`;
}

/**
 * Convert BlockNote-generated HTML to inline-styled HTML for Word compatibility.
 * BlockNote outputs classes like .bn-*, this strips and inlines basic styles.
 */
export function enrichBlockNoteHtml(blockNoteHtml: string, title: string, meta: string): string {
  // Inject title + meta before the content, then wrap
  const bodyContent = `
<h1 style="${STYLES.h1}">${escHtml(title)}</h1>
<p style="${STYLES.p};color:#555;font-size:9.5pt;">${meta}</p>
<hr style="${STYLES.hr}">
${blockNoteHtml}`;

  return wrapWordHtml(bodyContent);
}
