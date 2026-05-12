/**
 * Chuyển BlockNote blocks → Word-compatible HTML (inline styles).
 * Giữ nguyên màu text/background, heading, list, table, checklist.
 * Không đi qua markdown để tránh mất thông tin.
 */

// ─── Color maps (từ COLORS_DEFAULT của BlockNote) ────────────────────────────

const BN_TEXT_COLORS: Record<string, string> = {
  gray:   "#9B9A97",
  brown:  "#64473A",
  red:    "#E03E3E",
  orange: "#D9730D",
  yellow: "#DFAB01",
  green:  "#4D6461",
  blue:   "#0B6E99",
  purple: "#6940A5",
  pink:   "#AD1A72",
};

const BN_BG_COLORS: Record<string, string> = {
  gray:   "#EBECED",
  brown:  "#E9E5E3",
  red:    "#FBE4E4",
  orange: "#F6E9D9",
  yellow: "#FBF3DB",
  green:  "#DDEDEA",
  blue:   "#DDEBF1",
  purple: "#EAE4F2",
  pink:   "#F4DFEB",
};

function resolveColor(value: unknown, role: "text" | "background"): string | null {
  if (!value || value === "default") return null;
  const s = String(value).trim();
  if (role === "text")       return BN_TEXT_COLORS[s] ?? (/^#[0-9a-f]{6}$/i.test(s) ? s : null);
  if (role === "background") return BN_BG_COLORS[s]   ?? (/^#[0-9a-f]{6}$/i.test(s) ? s : null);
  return null;
}

// ─── Shared styles ────────────────────────────────────────────────────────────

const FONT  = "Calibri,'Segoe UI',Arial,sans-serif";
const BASE  = `font-family:${FONT};font-size:11pt;color:#1a1a1a;`;
const H1_ST = `font-family:${FONT};font-size:18pt;font-weight:bold;color:#111;margin:16pt 0 6pt;`;
const H2_ST = `font-family:${FONT};font-size:14pt;font-weight:bold;color:#1a1a1a;margin:12pt 0 4pt;`;
const H3_ST = `font-family:${FONT};font-size:12pt;font-weight:bold;color:#333;margin:8pt 0 3pt;`;
const P_ST  = `${BASE}margin:4pt 0;`;
const LI_ST = `${BASE}margin:2pt 0;`;
const UL_ST = "margin:4pt 0 4pt 18pt;padding:0;list-style-type:disc;";
const OL_ST = "margin:4pt 0 4pt 18pt;padding:0;";
const HR_ST = "border:none;border-top:1pt solid #ccc;margin:10pt 0;";
const CODE_BLOCK_ST = `font-family:'Courier New',monospace;font-size:10pt;background:#F5F5F5;padding:8pt;margin:6pt 0;white-space:pre-wrap;`;
const QUOTE_ST = `${BASE}border-left:3pt solid #ccc;margin:6pt 0 6pt 12pt;padding:2pt 0 2pt 8pt;font-style:italic;color:#555;`;
const TABLE_ST = "border-collapse:collapse;width:100%;margin:8pt 0;";
const TH_ST = `border:1pt solid #CCCCCC;padding:4pt 6pt;background:#E8E8E8;font-weight:bold;${BASE}`;
const TD_ST = `border:1pt solid #CCCCCC;padding:4pt 6pt;background:#FFFFFF;${BASE}`;

// ─── HTML escape ─────────────────────────────────────────────────────────────

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function escAttr(s: string): string {
  return esc(s).replace(/"/g, "&quot;");
}

// ─── Inline content renderer ─────────────────────────────────────────────────

type InlinePiece = {
  type: string;
  text?: string;
  href?: string;
  content?: InlinePiece[];
  styles?: Record<string, unknown>;
};

function renderInline(pieces: InlinePiece[] | undefined): string {
  if (!pieces || !pieces.length) return "";
  return pieces.map(piece => {
    if (piece.type === "text") {
      const st = piece.styles || {};
      const parts: string[] = [];
      if (st.bold)      parts.push("font-weight:bold");
      if (st.italic)    parts.push("font-style:italic");
      if (st.underline) parts.push("text-decoration:underline");
      if (st.strike)    parts.push("text-decoration:line-through");
      if (st.code)      parts.push("font-family:'Courier New',monospace;font-size:10pt;background:#F5F5F5;padding:1pt 3pt;");
      const fg = resolveColor(st.textColor, "text");
      const bg = resolveColor(st.backgroundColor, "background");
      if (fg) parts.push(`color:${fg}`);
      if (bg) parts.push(`background-color:${bg}`);

      const text = esc(piece.text ?? "");
      if (!parts.length) return text;
      return `<span style="${parts.join(";")}">${text}</span>`;
    }
    if (piece.type === "link") {
      const href = escAttr(piece.href ?? "");
      const inner = renderInline(piece.content);
      return `<a href="${href}" style="color:#0563C1;text-decoration:underline">${inner}</a>`;
    }
    return "";
  }).join("");
}

// ─── Block renderer ───────────────────────────────────────────────────────────

type BnBlock = {
  type: string;
  props?: Record<string, unknown>;
  content?: unknown;
  children?: BnBlock[];
};

function blockBgStyle(props: Record<string, unknown> | undefined): string {
  const bg = resolveColor(props?.backgroundColor, "background");
  return bg ? `background-color:${bg};` : "";
}

function renderTableContent(content: unknown): string {
  const tc = content as { type?: string; rows?: { cells: unknown[] }[]; headerRows?: number } | undefined;
  if (!tc || tc.type !== "tableContent" || !Array.isArray(tc.rows)) return "";

  const headerRows = Math.max(0, Number(tc.headerRows) || 0);
  const maxCols = Math.max(...tc.rows.map(r => r.cells.length), 0);

  const rowsHtml = tc.rows.map((row, ri) =>
    "<tr>" + Array.from({ length: maxCols }, (_, ci) => {
      const cell = row.cells[ci];
      const cellContent = Array.isArray(cell) ? cell : (cell as any)?.content ?? [];
      const isHeader = ri < headerRows;
      const inner = renderInline(cellContent as InlinePiece[]);
      return `<td style="${isHeader ? TH_ST : TD_ST}" valign="top">${inner || "&nbsp;"}</td>`;
    }).join("") + "</tr>"
  ).join("");

  return `<table style="${TABLE_ST}" cellspacing="0" cellpadding="0">${rowsHtml}</table>`;
}

function renderBlockList(blocks: BnBlock[], listType?: "ul" | "ol", depth = 0): string {
  let html = "";
  let i = 0;
  let counter = 1;

  while (i < blocks.length) {
    const b = blocks[i];
    const props = b.props ?? {};
    const bgSt = blockBgStyle(props);
    const alignSt = props.textAlignment && props.textAlignment !== "left"
      ? `text-align:${props.textAlignment};` : "";
    const content = Array.isArray(b.content) ? b.content as InlinePiece[] : [];
    const children = b.children ?? [];

    switch (b.type) {

      case "heading": {
        const lvl = Number(props.level) || 1;
        const hSt = lvl === 1 ? H1_ST : lvl === 2 ? H2_ST : H3_ST;
        html += `<h${lvl} style="${hSt}${alignSt}${bgSt}">${renderInline(content)}</h${lvl}>`;
        if (children.length) html += renderBlockList(children);
        break;
      }

      case "paragraph": {
        const inner = renderInline(content);
        html += `<p style="${P_ST}${alignSt}${bgSt}">${inner || "&nbsp;"}</p>`;
        if (children.length) html += renderBlockList(children);
        break;
      }

      case "bulletListItem": {
        const inner = renderInline(content);
        const childHtml = children.length
          ? `<ul style="${UL_ST}">${renderBlockList(children, "ul", depth + 1)}</ul>`
          : "";
        html += `<li style="${LI_ST}${alignSt}${bgSt}">${inner}${childHtml}</li>`;
        break;
      }

      case "numberedListItem": {
        const inner = renderInline(content);
        const childHtml = children.length
          ? `<ol style="${OL_ST}">${renderBlockList(children, "ol", depth + 1)}</ol>`
          : "";
        html += `<li style="${LI_ST}${alignSt}${bgSt}" value="${counter}">${inner}${childHtml}</li>`;
        counter++;
        break;
      }

      case "checkListItem": {
        const checked = Boolean(props.checked);
        const mark = checked ? "☑ " : "☐ ";
        const inner = renderInline(content);
        html += `<li style="${LI_ST}${bgSt}">${esc(mark)}${inner}</li>`;
        break;
      }

      case "quote": {
        html += `<blockquote style="${QUOTE_ST}${bgSt}">${renderInline(content)}</blockquote>`;
        if (children.length) html += renderBlockList(children);
        break;
      }

      case "codeBlock": {
        const raw = content.map((p: any) => p.text ?? "").join("");
        html += `<pre style="${CODE_BLOCK_ST}">${esc(raw)}</pre>`;
        break;
      }

      case "divider": {
        html += `<hr style="${HR_ST}">`;
        break;
      }

      case "table": {
        html += renderTableContent(b.content);
        html += `<p style="margin:2pt 0 8pt;">&nbsp;</p>`;
        break;
      }

      case "image": {
        const url  = String(props.url  ?? "").trim();
        const cap  = String(props.caption ?? props.name ?? "").trim();
        if (url) {
          html += `<p style="${P_ST}"><a href="${escAttr(url)}" style="color:#0563C1;text-decoration:underline">${esc(cap || url)}</a></p>`;
        } else if (cap) {
          html += `<p style="${P_ST}">[Hình: ${esc(cap)}]</p>`;
        }
        break;
      }

      default: {
        if (content.length) {
          html += `<p style="${P_ST}">${renderInline(content)}</p>`;
        }
        if (children.length) html += renderBlockList(children);
      }
    }
    i++;
  }
  return html;
}

// ─── Group consecutive list items into <ul>/<ol>/<ul> (checklist) ─────────────

function groupAndRender(blocks: BnBlock[]): string {
  let html = "";
  let i = 0;

  while (i < blocks.length) {
    const b = blocks[i];

    if (b.type === "bulletListItem" || b.type === "checkListItem") {
      const tag = "ul";
      const items: BnBlock[] = [];
      while (i < blocks.length &&
        (blocks[i].type === "bulletListItem" || blocks[i].type === "checkListItem")) {
        items.push(blocks[i++]);
      }
      html += `<ul style="${UL_ST}">${renderBlockList(items, "ul")}</ul>`;
      continue;
    }

    if (b.type === "numberedListItem") {
      const items: BnBlock[] = [];
      while (i < blocks.length && blocks[i].type === "numberedListItem") {
        items.push(blocks[i++]);
      }
      html += `<ol style="${OL_ST}">${renderBlockList(items, "ol")}</ol>`;
      continue;
    }

    html += renderBlockList([b]);
    i++;
  }
  return html;
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Chuyển mảng BlockNote blocks thành Word-compatible HTML (inline styles).
 * Giữ màu text/background, heading, list, bảng, checklist.
 */
export function blocksToWordHtml(blocks: unknown[]): string {
  return groupAndRender(blocks as BnBlock[]);
}
