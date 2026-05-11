"use client";

import {
  Document,
  Packer,
  Paragraph,
  TextRun,
  HeadingLevel,
  Table,
  TableRow,
  TableCell,
  WidthType,
  AlignmentType,
  LevelFormat,
  BorderStyle,
  convertMillimetersToTwip,
} from "docx";

// ─── Inline markdown parser (bold / italic / bold-italic / code) ───────────

function parseInlineRuns(text: string): TextRun[] {
  const runs: TextRun[] = [];
  let rem = text;

  while (rem.length > 0) {
    // Bold-italic: ***text***
    if (rem.startsWith("***")) {
      const end = rem.indexOf("***", 3);
      if (end !== -1) {
        const t = rem.slice(3, end);
        if (t) runs.push(new TextRun({ text: t, bold: true, italics: true, font: "Segoe UI", size: 22 }));
        rem = rem.slice(end + 3);
        continue;
      }
    }
    // Bold: **text**
    if (rem.startsWith("**")) {
      const end = rem.indexOf("**", 2);
      if (end !== -1) {
        const t = rem.slice(2, end);
        if (t) runs.push(new TextRun({ text: t, bold: true, font: "Segoe UI", size: 22 }));
        rem = rem.slice(end + 2);
        continue;
      }
    }
    // Italic: *text* (single star, not double)
    if (rem.startsWith("*") && !rem.startsWith("**")) {
      const end = rem.indexOf("*", 1);
      if (end !== -1) {
        const t = rem.slice(1, end);
        if (t) runs.push(new TextRun({ text: t, italics: true, font: "Segoe UI", size: 22 }));
        rem = rem.slice(end + 1);
        continue;
      }
    }
    // Inline code: `text`
    if (rem.startsWith("`")) {
      const end = rem.indexOf("`", 1);
      if (end !== -1) {
        const t = rem.slice(1, end);
        if (t) runs.push(new TextRun({ text: t, font: "Courier New", size: 20 }));
        rem = rem.slice(end + 1);
        continue;
      }
    }
    // Plain text until next marker
    const next = rem.search(/\*{1,3}|`/);
    if (next <= 0) {
      // Consume one char to avoid infinite loop on unmatched markers
      runs.push(new TextRun({ text: rem[0], font: "Segoe UI", size: 22 }));
      rem = rem.slice(1);
    } else {
      runs.push(new TextRun({ text: rem.slice(0, next), font: "Segoe UI", size: 22 }));
      rem = rem.slice(next);
    }
  }

  return runs.length > 0 ? runs : [new TextRun({ text: "", font: "Segoe UI", size: 22 })];
}

// ─── Markdown block parser ─────────────────────────────────────────────────

type Block =
  | { type: "heading"; level: 1 | 2 | 3; text: string }
  | { type: "bullet"; text: string }
  | { type: "numbered"; number: number; text: string }
  // lines: mỗi phần tử là 1 dòng hiển thị riêng (tách bởi hard break `\` trong markdown)
  | { type: "paragraph"; lines: string[] }
  | { type: "table"; rows: string[][] };

function parseMarkdownBlocks(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i].trim();
    if (!line) { i++; continue; }

    // Headings (check ### before ## before #)
    if (line.startsWith("### ")) {
      blocks.push({ type: "heading", level: 3, text: line.slice(4).trim() });
      i++; continue;
    }
    if (line.startsWith("## ")) {
      blocks.push({ type: "heading", level: 2, text: line.slice(3).trim() });
      i++; continue;
    }
    if (line.startsWith("# ")) {
      blocks.push({ type: "heading", level: 1, text: line.slice(2).trim() });
      i++; continue;
    }

    // Table
    if (line.startsWith("|")) {
      const tableRows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith("|")) {
        const tl = lines[i].trim();
        // Skip separator rows (|---|---|)
        if (/^\|[\s\-:|]+\|$/.test(tl)) { i++; continue; }
        const cells = tl.replace(/^\|/, "").replace(/\|$/, "").split("|").map(c => c.trim());
        if (cells.some(c => c.length > 0)) tableRows.push(cells);
        i++;
      }
      if (tableRows.length > 0) blocks.push({ type: "table", rows: tableRows });
      continue;
    }

    // Bullet list
    if (line.startsWith("- ") || line.startsWith("* ")) {
      blocks.push({ type: "bullet", text: line.slice(2).trim() });
      i++; continue;
    }

    // Numbered list (preserve original number)
    const numMatch = line.match(/^(\d+)\.\s+(.+)$/);
    if (numMatch) {
      blocks.push({ type: "numbered", number: parseInt(numMatch[1], 10), text: numMatch[2].trim() });
      i++; continue;
    }

    // Paragraph (merge consecutive non-special lines, tôn trọng hard break `\`)
    // Trong markdown: dòng kết thúc bằng `\` = hard line break (xuống dòng mới)
    // Dòng không có `\` = soft wrap (nối tiếp với dấu cách)
    const hardLines: string[] = [];  // mỗi phần tử = 1 dòng hiển thị
    let accumulator = "";

    const flushAccumulator = () => {
      if (accumulator.trim()) hardLines.push(accumulator.trim());
      accumulator = "";
    };

    const processRawLine = (rawLine: string) => {
      if (rawLine.endsWith("\\")) {
        // Hard break: nối vào accumulator rồi flush thành 1 dòng riêng
        accumulator += (accumulator ? " " : "") + rawLine.slice(0, -1).trimEnd();
        flushAccumulator();
      } else {
        // Soft wrap: nối tiếp accumulator
        accumulator += (accumulator ? " " : "") + rawLine;
      }
    };

    processRawLine(line);
    i++;
    while (i < lines.length) {
      const next = lines[i].trim();
      if (!next || next.startsWith("#") || next.startsWith("|") ||
          next.startsWith("- ") || next.startsWith("* ") || /^\d+\.\s/.test(next)) break;
      processRawLine(next);
      i++;
    }
    flushAccumulator(); // flush phần còn lại

    if (hardLines.length > 0) {
      blocks.push({ type: "paragraph", lines: hardLines });
    }
  }

  return blocks;
}

// ─── DOCX export (using docx npm package – full Unicode + inline formatting) ──

export async function exportMarkdownToDocxBytes(markdown: string): Promise<Uint8Array> {
  const children: (Paragraph | Table)[] = [];
  const blocks = parseMarkdownBlocks(markdown);

  const thin = { style: BorderStyle.SINGLE, size: 4, color: "CCCCCC" };
  const none = { style: BorderStyle.NONE, size: 0, color: "FFFFFF" };

  for (const block of blocks) {
    switch (block.type) {
      case "heading": {
        const level =
          block.level === 1 ? HeadingLevel.HEADING_1
          : block.level === 2 ? HeadingLevel.HEADING_2
          : HeadingLevel.HEADING_3;
        const fontSize = block.level === 1 ? 36 : block.level === 2 ? 28 : 24; // half-points
        // Strip inline markdown markers (**, *, ***) — headings are already bold via style
        const headingText = block.text.replace(/\*+([^*]+)\*+/g, '$1').replace(/`([^`]+)`/g, '$1');
        children.push(
          new Paragraph({
            heading: level,
            spacing: { before: block.level === 1 ? 400 : 280, after: 160 },
            children: [
              new TextRun({ text: headingText, bold: true, font: "Segoe UI", size: fontSize }),
            ],
          })
        );
        break;
      }

      case "bullet": {
        children.push(
          new Paragraph({
            bullet: { level: 0 },
            spacing: { before: 60, after: 60 },
            children: parseInlineRuns(block.text),
          })
        );
        break;
      }

      case "numbered": {
        children.push(
          new Paragraph({
            numbering: { reference: "numbered-list", level: 0 },
            spacing: { before: 60, after: 60 },
            children: parseInlineRuns(block.text),
          })
        );
        break;
      }

      case "paragraph": {
        // Mỗi dòng trong block.lines là 1 dòng hiển thị riêng biệt.
        // Dùng TextRun({ break: 1 }) để tạo xuống dòng trong cùng 1 paragraph DOCX.
        const runs: TextRun[] = [];
        block.lines.forEach((lineText, idx) => {
          if (idx > 0) {
            runs.push(new TextRun({ break: 1 }));
          }
          runs.push(...parseInlineRuns(lineText));
        });
        children.push(
          new Paragraph({
            spacing: { before: 80, after: 80 },
            children: runs,
          })
        );
        break;
      }

      case "table": {
        const maxCols = Math.max(...block.rows.map(r => r.length));
        children.push(
          new Table({
            width: { size: 100, type: WidthType.PERCENTAGE },
            rows: block.rows.map((row, rowIdx) =>
              new TableRow({
                children: Array.from({ length: maxCols }, (_, colIdx) =>
                  new TableCell({
                    borders: {
                      top: thin, bottom: thin, left: thin, right: thin,
                    },
                    shading: rowIdx === 0 ? { fill: "E8E8E8" } : { fill: "FFFFFF" },
                    margins: {
                      top: convertMillimetersToTwip(2),
                      bottom: convertMillimetersToTwip(2),
                      left: convertMillimetersToTwip(3),
                      right: convertMillimetersToTwip(3),
                    },
                    children: [
                      new Paragraph({
                        alignment: AlignmentType.LEFT,
                        children: rowIdx === 0
                          ? [new TextRun({ text: row[colIdx] ?? "", bold: true, font: "Segoe UI", size: 22 })]
                          : parseInlineRuns(row[colIdx] ?? ""),
                      }),
                    ],
                  })
                ),
              })
            ),
          })
        );
        // Add spacing after table
        children.push(new Paragraph({ text: "", spacing: { after: 120 } }));
        break;
      }
    }
  }

  const doc = new Document({
    styles: {
      default: {
        document: {
          run: { font: "Segoe UI", size: 22 },
          paragraph: { spacing: { line: 276 } }, // 1.15 line spacing
        },
      },
    },
    numbering: {
      config: [
        {
          reference: "numbered-list",
          levels: [
            {
              level: 0,
              format: LevelFormat.DECIMAL,
              text: "%1.",
              alignment: AlignmentType.LEFT,
              style: {
                paragraph: {
                  indent: { left: 720, hanging: 360 },
                  spacing: { before: 60, after: 60 },
                },
                run: { font: "Segoe UI", size: 22 },
              },
            },
          ],
        },
      ],
    },
    sections: [
      {
        properties: {
          page: {
            margin: {
              top: convertMillimetersToTwip(25),
              bottom: convertMillimetersToTwip(25),
              left: convertMillimetersToTwip(25),
              right: convertMillimetersToTwip(20),
            },
          },
        },
        children,
      },
    ],
  });

  const arrayBuffer = await Packer.toArrayBuffer(doc);
  return new Uint8Array(arrayBuffer);
}

// ─── Base64 helper ──────────────────────────────────────────────────────────

export function uint8ToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}
