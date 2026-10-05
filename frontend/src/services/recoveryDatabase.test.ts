import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { IDBFactory, IDBKeyRange } from 'fake-indexeddb';
import { IndexedDBService } from './recoveryDatabase.ts';
import type { TranscriptUpdate } from '../types/index.ts';

let service: IndexedDBService;
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory();
  globalThis.IDBKeyRange = IDBKeyRange;
  service = new IndexedDBService();
});
afterEach(() => service.close());
const update = (sequence: number, text: string, partial = false): TranscriptUpdate => ({
  sequence_id: sequence, text, is_partial: partial, source: 'Audio', timestamp: '12:00', confidence: 0.9,
  chunk_start_time: sequence, audio_start_time: sequence, audio_end_time: sequence + 1, duration: 1,
});
const metadata = { meetingId: 'session', title: 'Meeting', startTime: 1, lastUpdated: 1, transcriptCount: 0, savedToSQLite: false };

test('concurrent partial updates are upserts, including sequence zero', async () => {
  await service.saveMeetingMetadata(metadata);
  await Promise.all([
    service.saveTranscript('session', update(0, 'par', true)),
    service.saveTranscript('session', update(0, 'final')),
    service.saveTranscript('session', update(0, 'delayed partial', true)),
  ]);
  const rows = await service.getTranscripts('session');
  assert.deepEqual(rows.map((row) => row.text), ['final']);
  assert.equal((await service.getMeetingMetadata('session'))?.transcriptCount, 1);
});

test('v1 upgrade preserves final text, speakers and correct metadata counts', async () => {
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.open('MeetingOneRecoveryDB', 1);
    request.onerror = () => reject(request.error);
    request.onupgradeneeded = () => {
      request.result.createObjectStore('meetings', { keyPath: 'meetingId' }).put(metadata);
      const rows = request.result.createObjectStore('transcripts', { keyPath: 'id', autoIncrement: true });
      rows.createIndex('meetingId', 'meetingId');
      rows.add({ ...update(1, 'part', true), meetingId: 'session', storedAt: 1 });
      rows.add({ ...update(1, 'final'), meetingId: 'session', speaker_name: 'Lan', storedAt: 2 });
      rows.add({ ...update(1, 'stale', true), meetingId: 'session', storedAt: 3 });
      rows.add({ text: 'legacy', sequenceId: 0, meetingId: 'session', storedAt: 0 });
    };
    request.onsuccess = () => { request.result.close(); resolve(); };
  });
  const rows = await service.getTranscripts('session');
  assert.deepEqual(rows.map((row) => row.text), ['legacy', 'final']);
  assert.equal(rows[1].speaker_name, 'Lan');
  assert.equal((await service.getMeetingMetadata('session'))?.transcriptCount, 2);
});

test('retention never deletes unsaved or audio-pending meetings', async () => {
  await service.saveMeetingMetadata(metadata);
  await service.saveMeetingMetadata({ ...metadata, meetingId: 'pending', savedToSQLite: true, audioRecoveryPending: true });
  assert.equal(await service.deleteOldMeetings(0), 0);
  assert.equal(await service.deleteSavedMeetings(0), 0);
  assert.equal((await service.getAllMeetings()).length, 2);
});

test('deleting a meeting commits removal of both metadata and transcripts', async () => {
  await service.saveMeetingMetadata(metadata);
  await service.saveTranscript('session', update(1, 'text'));
  await service.deleteMeeting('session');
  assert.equal(await service.getMeetingMetadata('session'), null);
  assert.equal(await service.getTranscriptCount('session'), 0);
});

test('failed v1 migration rolls back without deleting original rows', async () => {
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.open('MeetingOneRecoveryDB', 1);
    request.onerror = () => reject(request.error);
    request.onupgradeneeded = () => {
      request.result.createObjectStore('meetings', { keyPath: 'meetingId' }).put(metadata);
      const rows = request.result.createObjectStore('transcripts', { keyPath: 'id' });
      rows.add({ id: 'invalid legacy ID', meetingId: 'session', text: 'must be retained' });
    };
    request.onsuccess = () => { request.result.close(); resolve(); };
  });
  await assert.rejects(service.init());
  const db = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open('MeetingOneRecoveryDB', 1);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);
  });
  const rows = await new Promise<{ text: string }[]>((resolve, reject) => {
    const request = db.transaction('transcripts').objectStore('transcripts').getAll();
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);
  });
  assert.equal(rows[0].text, 'must be retained');
  db.close();
});
