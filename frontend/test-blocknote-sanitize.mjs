/**
 * test-blocknote-sanitize.mjs
 *
 * Mô phỏng pipeline: markdown → tryParseMarkdownToBlocks → sanitizeBlocks → useCreateBlockNote
 *
 * Kiểm tra tất cả các trường hợp đầu vào gây lỗi "Invalid array passed to renderSpec"
 * bằng cách validate output của sanitizeBlocks trước khi truyền vào BlockNote.
 *
 * Chạy: node test-blocknote-sanitize.mjs
 */

// ─── Copy nguyên từ BlockNoteSummaryView.tsx ────────────────────────────────

const KNOWN_BLOCK_TYPES = new Set([
  'paragraph', 'heading', 'bulletListItem', 'numberedListItem',
  'checkListItem', 'image', 'video', 'audio', 'file', 'table', 'codeBlock'
]);

function extractTextFromInlineItem(item) {
  if (typeof item === 'string') return item;
  if (typeof item?.text === 'string') return item.text;
  if (Array.isArray(item?.content)) {
    return item.content.map(extractTextFromInlineItem).join('');
  }
  return '';
}

function sanitizeInlineContent(items) {
  return items
    .filter(item => item != null)
    .map(item => {
      if (typeof item === 'string') {
        return { type: 'text', text: item, styles: {} };
      }
      if (typeof item !== 'object') {
        return { type: 'text', text: String(item), styles: {} };
      }
      if (item.type === 'hardBreak' || item.type === 'softBreak') {
        return { type: 'text', text: ' ', styles: {} };
      }
      if (item.type === 'mention') {
        const label = item.label ?? item.attrs?.label ?? '@unknown';
        return { type: 'text', text: String(label), styles: {} };
      }
      if (item.type === 'link') {
        return {
          type: 'link',
          href: typeof item.href === 'string' ? item.href : '',
          content: Array.isArray(item.content)
            ? sanitizeInlineContent(item.content)
            : [],
        };
      }
      const text = extractTextFromInlineItem(item);
      return {
        type: 'text',
        text: text.length > 0 ? text : ' ',
        styles: item.styles && typeof item.styles === 'object' ? item.styles : {},
      };
    });
}

function sanitizeProps(type, props) {
  const p = props && typeof props === 'object' ? { ...props } : {};
  if (type === 'heading') {
    const lvl = Number(p.level);
    p.level = (lvl === 1 || lvl === 2 || lvl === 3) ? lvl : 1;
  }
  if (type === 'checkListItem') {
    p.checked = p.checked === true || p.checked === 'true';
  }
  if (type === 'numberedListItem' && p.start !== undefined) {
    p.start = Number(p.start) || 1;
  }
  if (p.textAlignment !== undefined) {
    const validAlignments = ['left', 'center', 'right', 'justify'];
    if (!validAlignments.includes(p.textAlignment)) {
      p.textAlignment = 'left';
    }
  }
  return p;
}

function sanitizeTableContent(tableContent) {
  if (!tableContent || typeof tableContent !== 'object') {
    return { type: 'tableContent', rows: [] };
  }
  const rows = Array.isArray(tableContent.rows) ? tableContent.rows : [];
  const sanitizedRows = rows
    .filter(row => row != null && typeof row === 'object')
    .map(row => ({
      ...row,
      cells: Array.isArray(row.cells)
        ? row.cells.map(cell => Array.isArray(cell) ? sanitizeInlineContent(cell) : [])
        : [],
    }));
  return { type: 'tableContent', rows: sanitizedRows };
}

function sanitizeBlocks(blocks) {
  if (!Array.isArray(blocks)) return [];
  return blocks
    .filter(block => block != null && typeof block === 'object')
    .map(block => {
      const type = KNOWN_BLOCK_TYPES.has(block.type) ? block.type : 'paragraph';
      const content = type === 'table'
        ? sanitizeTableContent(block.content)
        : Array.isArray(block.content)
          ? sanitizeInlineContent(block.content)
          : typeof block.content === 'string'
            ? [{ type: 'text', text: block.content, styles: {} }]
            : [];
      const children = Array.isArray(block.children)
        ? sanitizeBlocks(block.children)
        : [];
      return {
        ...block,
        type,
        content,
        children,
        props: sanitizeProps(type, block.props),
        id: typeof block.id === 'string' && block.id ? block.id : 'test-uuid-' + Math.random().toString(36).slice(2),
      };
    });
}

// ─── Validator: kiểm tra output có gây renderSpec không ─────────────────────

function validateBlocksForRenderSpec(blocks, path = 'root') {
  const errors = [];

  for (let i = 0; i < blocks.length; i++) {
    const block = blocks[i];
    const blockPath = `${path}[${i}] type="${block.type}"`;

    // Block type phải là string
    if (typeof block.type !== 'string') {
      errors.push(`${blockPath}: type is not a string → "${block.type}"`);
    }

    // Content validation
    if (block.type === 'table') {
      if (!block.content || typeof block.content !== 'object') {
        errors.push(`${blockPath}: table content is not object`);
      }
    } else if (Array.isArray(block.content)) {
      for (let j = 0; j < block.content.length; j++) {
        const item = block.content[j];
        const itemPath = `${blockPath}.content[${j}]`;

        if (!item || typeof item !== 'object') {
          errors.push(`${itemPath}: inline item is null/non-object → ${JSON.stringify(item)}`);
          continue;
        }

        if (item.type === 'text') {
          // text phải là string không rỗng (ProseMirror ném lỗi với empty string)
          if (typeof item.text !== 'string') {
            errors.push(`${itemPath}: text.text is not a string → ${JSON.stringify(item.text)}`);
          } else if (item.text.length === 0) {
            errors.push(`${itemPath}: text.text is empty string "" → ProseMirror may throw`);
          }
          // styles phải là object
          if (!item.styles || typeof item.styles !== 'object') {
            errors.push(`${itemPath}: text.styles is not object → ${JSON.stringify(item.styles)}`);
          }
        } else if (item.type === 'link') {
          if (typeof item.href !== 'string') {
            errors.push(`${itemPath}: link.href is not string → ${JSON.stringify(item.href)}`);
          }
          if (!Array.isArray(item.content)) {
            errors.push(`${itemPath}: link.content is not array → ${JSON.stringify(item.content)}`);
          }
        } else {
          // Bất kỳ type nào khác ngoài 'text' và 'link' không nằm trong BlockNote schema
          errors.push(`${itemPath}: UNKNOWN inline type "${item.type}" → toDOM will be undefined → renderSpec crash`);
        }
      }
    }

    // Recurse children
    if (Array.isArray(block.children) && block.children.length > 0) {
      const childErrors = validateBlocksForRenderSpec(block.children, `${blockPath}.children`);
      errors.push(...childErrors);
    }
  }

  return errors;
}

// ─── Test runner ─────────────────────────────────────────────────────────────

let passed = 0;
let failed = 0;

function test(name, inputBlocks, expectErrors = false) {
  const result = sanitizeBlocks(inputBlocks);
  const errors = validateBlocksForRenderSpec(result);
  const hasErrors = errors.length > 0;

  if (expectErrors ? hasErrors : !hasErrors) {
    console.log(`  ✅ PASS: ${name}`);
    if (errors.length > 0) {
      errors.forEach(e => console.log(`         (expected error) ${e}`));
    }
    passed++;
  } else {
    console.log(`  ❌ FAIL: ${name}`);
    if (errors.length > 0) {
      errors.forEach(e => console.log(`         ERROR: ${e}`));
    } else {
      console.log(`         Expected errors but got none`);
    }
    console.log(`         Input:  ${JSON.stringify(inputBlocks, null, 2).slice(0, 400)}`);
    console.log(`         Output: ${JSON.stringify(result, null, 2).slice(0, 400)}`);
    failed++;
  }
}

// ─── Test cases ──────────────────────────────────────────────────────────────

console.log('\n══════════════════════════════════════════════');
console.log(' BlockNote sanitize() – Simulation Test Suite');
console.log('══════════════════════════════════════════════\n');

// ── Nhóm 1: Inline content không hợp lệ ─────────────────────────────────────
console.log('── Nhóm 1: Inline content problematic types ──');

test('hardBreak → text space', [
  { type: 'paragraph', content: [{ type: 'hardBreak' }], props: {}, children: [] }
]);

test('softBreak → text space', [
  { type: 'paragraph', content: [{ type: 'softBreak' }], props: {}, children: [] }
]);

test('mention → text', [
  { type: 'paragraph', content: [{ type: 'mention', label: '@Cuong' }], props: {}, children: [] }
]);

test('mention với attrs.label', [
  { type: 'paragraph', content: [{ type: 'mention', attrs: { label: '@admin' } }], props: {}, children: [] }
]);

test('ProseMirror strong mark node (chưa convert) → extract text', [
  {
    type: 'paragraph',
    content: [{ type: 'strong', content: [{ type: 'text', text: 'Thông tin cuộc họp' }] }],
    props: {},
    children: []
  }
]);

test('ProseMirror em mark node → extract text', [
  {
    type: 'paragraph',
    content: [{ type: 'em', content: [{ type: 'text', text: 'italic text' }] }],
    props: {},
    children: []
  }
]);

test('ProseMirror code mark node → extract text', [
  {
    type: 'paragraph',
    content: [{ type: 'code', content: [{ type: 'text', text: 'const x = 1' }] }],
    props: {},
    children: []
  }
]);

test('text node với text = undefined → fallback space', [
  {
    type: 'paragraph',
    content: [{ type: 'text', text: undefined, styles: {} }],
    props: {},
    children: []
  }
]);

test('text node với text = null → fallback space', [
  {
    type: 'paragraph',
    content: [{ type: 'text', text: null, styles: {} }],
    props: {},
    children: []
  }
]);

test('text node với text = "" (empty string) → fallback space', [
  {
    type: 'paragraph',
    content: [{ type: 'text', text: '', styles: {} }],
    props: {},
    children: []
  }
]);

test('text node với styles = undefined → fallback {}', [
  {
    type: 'paragraph',
    content: [{ type: 'text', text: 'hello', styles: undefined }],
    props: {},
    children: []
  }
]);

test('inline item là string thô', [
  { type: 'paragraph', content: ['Thông tin cuộc họp'], props: {}, children: [] }
]);

test('inline item là số', [
  { type: 'paragraph', content: [42, 'text'], props: {}, children: [] }
]);

test('inline item là null (bị filter)', [
  { type: 'paragraph', content: [null, { type: 'text', text: 'valid', styles: {} }], props: {}, children: [] }
]);

test('link với href = undefined → fallback ""', [
  {
    type: 'paragraph',
    content: [{ type: 'link', href: undefined, content: [{ type: 'text', text: 'click', styles: {} }] }],
    props: {},
    children: []
  }
]);

test('link với content = null → fallback []', [
  {
    type: 'paragraph',
    content: [{ type: 'link', href: 'https://example.com', content: null }],
    props: {},
    children: []
  }
]);

// ── Nhóm 2: Block types không hợp lệ ────────────────────────────────────────
console.log('\n── Nhóm 2: Block types ──');

test('unknown block type → paragraph', [
  { type: 'customBlock', content: [{ type: 'text', text: 'hello', styles: {} }], props: {}, children: [] }
]);

test('type = undefined → paragraph', [
  { type: undefined, content: [{ type: 'text', text: 'hello', styles: {} }], props: {}, children: [] }
]);

test('type = null → paragraph', [
  { type: null, content: [{ type: 'text', text: 'hello', styles: {} }], props: {}, children: [] }
]);

test('block là null (bị filter)', [
  null,
  { type: 'paragraph', content: [{ type: 'text', text: 'valid', styles: {} }], props: {}, children: [] }
]);

test('initialContent = [] → sanitize trả []', []);

// ── Nhóm 3: Props không hợp lệ ───────────────────────────────────────────────
console.log('\n── Nhóm 3: Props validation ──');

test('heading level = 0 → fallback 1', [
  { type: 'heading', content: [{ type: 'text', text: 'title', styles: {} }], props: { level: 0 }, children: [] }
]);

test('heading level = "2" (string) → convert 2', [
  { type: 'heading', content: [{ type: 'text', text: 'title', styles: {} }], props: { level: '2' }, children: [] }
]);

test('heading level = 5 (invalid) → fallback 1', [
  { type: 'heading', content: [{ type: 'text', text: 'title', styles: {} }], props: { level: 5 }, children: [] }
]);

test('heading level = undefined → fallback 1', [
  { type: 'heading', content: [{ type: 'text', text: 'title', styles: {} }], props: { level: undefined }, children: [] }
]);

test('textAlignment = "center" → valid', [
  { type: 'paragraph', content: [{ type: 'text', text: 'x', styles: {} }], props: { textAlignment: 'center' }, children: [] }
]);

test('textAlignment = "invalid_value" → fallback left', [
  { type: 'paragraph', content: [{ type: 'text', text: 'x', styles: {} }], props: { textAlignment: 'invalid_value' }, children: [] }
]);

test('checkListItem.checked = "true" (string) → true', [
  { type: 'checkListItem', content: [{ type: 'text', text: 'task', styles: {} }], props: { checked: 'true' }, children: [] }
]);

test('numberedListItem.start = "3" (string) → 3', [
  { type: 'numberedListItem', content: [{ type: 'text', text: 'item', styles: {} }], props: { start: '3' }, children: [] }
]);

// ── Nhóm 4: Content không phải array ─────────────────────────────────────────
console.log('\n── Nhóm 4: Content format ──');

test('content là string thô → wrap thành text node', [
  { type: 'paragraph', content: 'Thông tin cuộc họp', props: {}, children: [] }
]);

test('content là undefined → []', [
  { type: 'paragraph', content: undefined, props: {}, children: [] }
]);

test('content là null → []', [
  { type: 'paragraph', content: null, props: {}, children: [] }
]);

test('content là số → []', [
  { type: 'paragraph', content: 123, props: {}, children: [] }
]);

// ── Nhóm 5: Children (nested blocks) ─────────────────────────────────────────
console.log('\n── Nhóm 5: Nested children ──');

test('children với nested invalid block', [
  {
    type: 'bulletListItem',
    content: [{ type: 'text', text: 'parent', styles: {} }],
    props: {},
    children: [
      { type: 'customChild', content: [{ type: 'hardBreak' }], props: {}, children: [] }
    ]
  }
]);

test('children = null → []', [
  { type: 'bulletListItem', content: [{ type: 'text', text: 'item', styles: {} }], props: {}, children: null }
]);

// ── Nhóm 6: Mô phỏng output thực tế từ tryParseMarkdownToBlocks ─────────────
console.log('\n── Nhóm 6: Mô phỏng output thực tế từ markdown báo cáo ──');

const MOCK_MARKDOWN_BLOCKS = [
  // **Thông tin cuộc họp** — bold paragraph
  {
    id: 'uuid-0',
    type: 'paragraph',
    props: { textAlignment: 'left', textColor: 'default', backgroundColor: 'default' },
    content: [{ type: 'text', text: 'Thông tin cuộc họp', styles: { bold: true } }],
    children: []
  },
  // - Thời gian: Tháng tư
  {
    id: 'uuid-1',
    type: 'bulletListItem',
    props: { textAlignment: 'left', textColor: 'default', backgroundColor: 'default' },
    content: [{ type: 'text', text: 'Thời gian: Tháng tư', styles: {} }],
    children: []
  },
  // - Địa điểm: Nhà Quốc Hội...
  {
    id: 'uuid-2',
    type: 'bulletListItem',
    props: { textAlignment: 'left', textColor: 'default', backgroundColor: 'default' },
    content: [{ type: 'text', text: 'Địa điểm: Nhà Quốc Hội, Thủ Đô Hà Nội', styles: {} }],
    children: []
  },
  // - Tên cuộc họp
  {
    id: 'uuid-3',
    type: 'bulletListItem',
    props: { textAlignment: 'left', textColor: 'default', backgroundColor: 'default' },
    content: [{ type: 'text', text: 'Tên cuộc họp: Kỳ Họp Thứ Nhất Quốc Hội Khóa Mười Sáu', styles: {} }],
    children: []
  },
  // - Người chủ trì
  {
    id: 'uuid-4',
    type: 'bulletListItem',
    props: {},
    content: [{ type: 'text', text: 'Người chủ trì: Không có thông tin.', styles: {} }],
    children: []
  },
  // **I. NỘI DUNG** — có thể parser tạo hardBreak ở đây
  {
    id: 'uuid-5',
    type: 'paragraph',
    props: { textAlignment: 'left' },
    content: [
      { type: 'text', text: 'I. NỘI DUNG TIẾN HÀNH TRONG HỘI NGHỊ', styles: { bold: true } },
      { type: 'hardBreak' }, // ← trường hợp này gây lỗi trước khi fix
    ],
    children: []
  },
  // Không có thông tin trong transcript.
  {
    id: 'uuid-6',
    type: 'paragraph',
    props: {},
    content: [{ type: 'text', text: 'Không có thông tin trong transcript.', styles: {} }],
    children: []
  },
  // **II. KẾ HOẠCH** — strong node chưa convert (lỗi ProseMirror mark)
  {
    id: 'uuid-7',
    type: 'paragraph',
    props: {},
    content: [
      { type: 'strong', content: [{ type: 'text', text: 'II. KẾ HOẠCH' }] } // ← ProseMirror mark chưa được convert
    ],
    children: []
  },
];

test('Full mock 8 blocks từ markdown báo cáo', MOCK_MARKDOWN_BLOCKS);

// Thêm trường hợp worst-case: nhiều loại lỗi cùng lúc
test('Worst-case: nhiều lỗi cùng lúc trong 1 block', [
  {
    type: 'paragraph',
    content: [
      null,
      { type: 'hardBreak' },
      { type: 'strong', content: [{ type: 'text', text: 'bold' }] },
      { type: 'text', text: '', styles: undefined },
      { type: 'mention', attrs: { label: '@user' } },
      { type: 'link', href: undefined, content: null },
    ],
    props: { textAlignment: 'diagonal' }, // invalid alignment
    children: null
  }
]);

// ── Nhóm 7: Table edge cases ─────────────────────────────────────────────────
console.log('\n── Nhóm 7: Table edge cases ──');

test('table với cells hợp lệ', [
  {
    type: 'table',
    content: {
      type: 'tableContent',
      rows: [
        { cells: [[{ type: 'text', text: 'header', styles: {} }], [{ type: 'text', text: 'value', styles: {} }]] }
      ]
    },
    props: {},
    children: []
  }
]);

test('table content = null → empty rows', [
  { type: 'table', content: null, props: {}, children: [] }
]);

test('table cell = not array → []', [
  {
    type: 'table',
    content: {
      type: 'tableContent',
      rows: [{ cells: ['invalid_cell', null] }]
    },
    props: {},
    children: []
  }
]);

test('table cell chứa hardBreak', [
  {
    type: 'table',
    content: {
      type: 'tableContent',
      rows: [{ cells: [[{ type: 'hardBreak' }, { type: 'text', text: 'data', styles: {} }]] }]
    },
    props: {},
    children: []
  }
]);

// ─── Kết quả ─────────────────────────────────────────────────────────────────

console.log('\n══════════════════════════════════════════════');
console.log(` Kết quả: ${passed} passed / ${failed} failed / ${passed + failed} total`);
console.log('══════════════════════════════════════════════');

if (failed > 0) {
  console.log('\n⚠️  Các test FAIL ở trên cần sửa sanitizeBlocks/sanitizeInlineContent.');
  process.exit(1);
} else {
  console.log('\n✅  Tất cả test pass — sanitize pipeline an toàn cho renderSpec.');
  process.exit(0);
}
