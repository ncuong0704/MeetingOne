import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TranscriptLedger } from './transcriptLedger.ts';
import type { TranscriptUpdate } from '../types/index.ts';

const update = (sequence: number, text: string, partial = false, time = sequence): TranscriptUpdate => ({
  sequence_id: sequence, text, is_partial: partial, source: 'Audio', timestamp: '12:00:00',
  chunk_start_time: time, audio_start_time: time, audio_end_time: time + 1, duration: 1, confidence: 0.9,
});

test('out of order updates and partials produce one ordered row per sequence', () => {
  const ledger = new TranscriptLedger();
  ledger.upsert(update(3, 'third'));
  ledger.upsert(update(1, 'first'));
  ledger.upsert(update(2, 'par', true));
  const id = ledger.snapshot()[1].id;
  ledger.upsert(update(2, 'second'));
  ledger.upsert(update(2, 'stale', true));
  assert.deepEqual(ledger.snapshot().map((row) => row.text), ['first', 'second', 'third']);
  assert.equal(ledger.snapshot()[1].id, id);
});

test('editing a segment survives subsequent ASR updates', () => {
  const ledger = new TranscriptLedger();
  ledger.upsert(update(0, 'raw', true));
  ledger.edit(0, 'user correction');
  ledger.upsert(update(0, 'ASR final'));
  assert.equal(ledger.snapshot()[0].text, 'user correction');
});

test('a new session cannot inherit the previous sequence IDs', () => {
  const ledger = new TranscriptLedger();
  ledger.upsert(update(1, 'old'));
  ledger.clear();
  ledger.upsert(update(1, 'new'));
  assert.deepEqual(ledger.snapshot().map((row) => row.text), ['new']);
});
