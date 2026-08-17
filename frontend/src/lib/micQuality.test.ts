import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  confidenceColorClass,
  confidenceLabel,
  dnsmosColorClass,
  dnsmosLabel,
  stripSuggestionEmoji,
} from './micQuality.ts';

test('dnsmosLabel maps score bands', () => {
  assert.equal(dnsmosLabel(4), 'Tốt');
  assert.equal(dnsmosLabel(3.5), 'Khá');
  assert.equal(dnsmosLabel(3), 'Khá');
  assert.equal(dnsmosLabel(2), 'Trung bình');
  assert.equal(dnsmosLabel(1.9), 'Kém');
});

test('confidenceLabel maps ASR confidence bands', () => {
  assert.equal(confidenceLabel(0.85), 'Xuất sắc');
  assert.equal(confidenceLabel(0.75), 'Tốt');
  assert.equal(confidenceLabel(0.6), 'Trung bình');
  assert.equal(confidenceLabel(0.59), 'Kém');
});

test('color classes stay distinct by quality band', () => {
  assert.equal(dnsmosColorClass(4), 'text-green-600');
  assert.equal(dnsmosColorClass(3), 'text-amber-500');
  assert.equal(dnsmosColorClass(2), 'text-orange-500');
  assert.equal(dnsmosColorClass(1), 'text-red-600');
  assert.equal(confidenceColorClass(0.9), 'text-green-600');
  assert.equal(confidenceColorClass(0.8), 'text-amber-500');
  assert.equal(confidenceColorClass(0.6), 'text-orange-500');
  assert.equal(confidenceColorClass(0.5), 'text-red-600');
});

test('stripSuggestionEmoji removes leading status emoji', () => {
  assert.equal(stripSuggestionEmoji('✅ Ghi rõ'), 'Ghi rõ');
  assert.equal(stripSuggestionEmoji('Không emoji'), 'Không emoji');
});
