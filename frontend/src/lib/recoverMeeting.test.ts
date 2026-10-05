import { test } from 'node:test';
import assert from 'node:assert/strict';
import { recoverStoredMeeting, type AudioRecoveryStatus } from './recoverMeeting.ts';
import type { StoredTranscript } from './recoveryTranscript.ts';

const transcript: StoredTranscript = {
  meetingId: 'session', storedAt: 1, sequence_id: 0, source: 'Audio', text: 'hello',
  timestamp: '12:00:00', confidence: 1, is_partial: false, chunk_start_time: 0,
  audio_start_time: 0, audio_end_time: 1, duration: 1, speaker_name: 'Lan',
};

function dependencies(audio: AudioRecoveryStatus | Error) {
  const cleanup: string[] = [], saves: string[] = [], pending: boolean[] = [];
  return {
    cleanup, saves, pending,
    dependencies: {
      loadMetadata: async () => ({ meetingId: 'session', title: 'Meeting', startTime: 1, lastUpdated: 1,
        transcriptCount: 1, savedToSQLite: false, folderPath: '/owned/meeting' }),
      loadTranscripts: async () => [transcript],
      recoverAudio: async () => { if (audio instanceof Error) throw audio; return audio; },
      saveMeeting: async (_title: string, rows: { sequence_id?: number; speaker_name?: string | null }[], _folder: string | null, session: string) => {
        assert.equal(rows[0].sequence_id, 0);
        assert.equal(rows[0].speaker_name, 'Lan');
        saves.push(session);
        return { meeting_id: 'saved' };
      },
      markSaved: async (_id: string, _saved: string, audioPending: boolean) => { pending.push(audioPending); },
      cleanup: async (folder: string) => { cleanup.push(folder); },
    },
  };
}

test('FFmpeg failure preserves all checkpoints and leaves audio retryable', async () => {
  const fixture = dependencies(new Error('FFmpeg failed'));
  const result = await recoverStoredMeeting('session', fixture.dependencies);
  assert.equal(result.meetingId, 'saved');
  assert.equal(result.audioRecoveryPending, true);
  assert.deepEqual(fixture.cleanup, []);
  assert.deepEqual(fixture.pending, [true]);
});

test('successful audio recovery cleans checkpoints after the transcript save', async () => {
  const fixture = dependencies({ status: 'success', chunk_count: 1, estimated_duration_seconds: 30,
    audio_file_path: '/owned/meeting/audio.mp4', message: 'Recovered' });
  const result = await recoverStoredMeeting('session', fixture.dependencies);
  assert.equal(result.audioRecoveryPending, false);
  assert.deepEqual(fixture.cleanup, ['/owned/meeting']);
  assert.deepEqual(fixture.saves, ['session']);
});

test('SQLite failure cannot delete the original checkpoints', async () => {
  const fixture = dependencies({ status: 'success', chunk_count: 1, estimated_duration_seconds: 30,
    audio_file_path: '/owned/meeting/audio.mp4', message: 'Recovered' });
  fixture.dependencies.saveMeeting = async () => { throw new Error('disk full'); };
  await assert.rejects(recoverStoredMeeting('session', fixture.dependencies), /disk full/);
  assert.deepEqual(fixture.cleanup, []);
  assert.deepEqual(fixture.pending, []);
});
