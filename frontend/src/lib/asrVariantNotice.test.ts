import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { asrVariantNotice } from '../components/asrSettingsConstants.ts';

function combined(notice: { description: string; warning?: string } | null): string {
  if (!notice) return '';
  return `${notice.description} ${notice.warning ?? ''}`;
}

test('zipformer 30M live notices differ between int8 and full', () => {
  const int8 = asrVariantNotice('zipformer-vi-30m', 'int8', 'live');
  const full = asrVariantNotice('zipformer-vi-30m', 'full', 'live');
  assert.ok(int8?.description);
  assert.ok(full?.description);
  assert.notEqual(combined(int8), combined(full));
  assert.match(combined(int8), /32/);
  assert.match(combined(full), /100/);
});

test('gipformer 65M full is a heavier warning than int8', () => {
  const int8 = asrVariantNotice('gipformer-65m-rnnt', 'int8', 'live');
  const full = asrVariantNotice('gipformer-65m-rnnt', 'full', 'live');
  assert.ok(int8);
  assert.ok(full?.warning);
  assert.notEqual(combined(int8), combined(full));
  assert.match(combined(full), /335/);
});

test('streaming and sherpa 2025 only describe the full variant', () => {
  const streaming = asrVariantNotice('zipformer-vi-30m-streaming', 'int8', 'live');
  const sherpa = asrVariantNotice('sherpa-onnx-zipformer-vi-2025-04-20', 'int8', 'file');
  assert.equal(streaming?.description, asrVariantNotice('zipformer-vi-30m-streaming', 'full', 'live')?.description);
  assert.equal(sherpa?.description, asrVariantNotice('sherpa-onnx-zipformer-vi-2025-04-20', 'full', 'file')?.description);
  assert.match(combined(streaming), /51/);
  assert.match(combined(sherpa), /270/);
});

test('file notices for zipformer 30M stay variant-specific without live real-time warning', () => {
  const int8 = asrVariantNotice('zipformer-vi-30m', 'int8', 'file');
  const full = asrVariantNotice('zipformer-vi-30m', 'full', 'file');
  assert.notEqual(combined(int8), combined(full));
  assert.equal(int8?.warning, undefined);
  assert.equal(full?.warning, undefined);
});

test('Nhận dạng panels show description/warning under the variant, not the family', () => {
  const srcRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
  const live = readFileSync(join(srcRoot, 'components/LiveAsrPanel.tsx'), 'utf8');
  const file = readFileSync(join(srcRoot, 'components/FileAsrPanel.tsx'), 'utf8');

  assert.equal(live.includes('liveDescription'), false);
  assert.equal(file.includes('selectedModelInfo?.description'), false);
  assert.match(live, /Biến thể[\s\S]*<AsrVariantNotice/);
  assert.match(file, /Biến thể[\s\S]*<AsrVariantNotice/);
});
