import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseMarkdownBlocks } from './markdownBlockParser.ts';

test('parseMarkdownBlocks maps headings, lists, and paragraphs', () => {
  const blocks = parseMarkdownBlocks(
    '# Tiêu đề\n## Mục\n### Chi tiết\n\nĐoạn một.\n\n- Gạch đầu dòng\n* Sao\n1. Số một\n2. Số hai',
  );
  assert.deepEqual(blocks, [
    { type: 'heading', level: 1, text: 'Tiêu đề' },
    { type: 'heading', level: 2, text: 'Mục' },
    { type: 'heading', level: 3, text: 'Chi tiết' },
    { type: 'paragraph', lines: ['Đoạn một.'] },
    { type: 'bullet', text: 'Gạch đầu dòng' },
    { type: 'bullet', text: 'Sao' },
    { type: 'numbered', number: 1, text: 'Số một' },
    { type: 'numbered', number: 2, text: 'Số hai' },
  ]);
});

test('parseMarkdownBlocks keeps table cells and skips the separator row', () => {
  const blocks = parseMarkdownBlocks(
    '| Tên | Vai trò |\n| --- | --- |\n| Lan | Chủ trì |',
  );
  assert.deepEqual(blocks, [
    {
      type: 'table',
      rows: [
        ['Tên', 'Vai trò'],
        ['Lan', 'Chủ trì'],
      ],
    },
  ]);
});

test('parseMarkdownBlocks returns empty for blank input', () => {
  assert.deepEqual(parseMarkdownBlocks(''), []);
  assert.deepEqual(parseMarkdownBlocks('\n  \n'), []);
});
