import type { TranscriptUpdate } from '../types/index.ts';

export type StoredTranscript = TranscriptUpdate & {
  id?: number;
  meetingId: string;
  storedAt: number;
};

/** Accept v1 camelCase keys and v1 event payloads during a lossless upgrade. */
export function normalizeStoredTranscript(row: Record<string, unknown>): StoredTranscript {
  const sequence = row.sequence_id ?? row.sequenceId ?? row.id;
  if (typeof sequence !== 'number' || !Number.isSafeInteger(sequence) || sequence < 0) {
    throw new Error('Invalid recovery transcript sequence');
  }
  return {
    id: typeof row.id === 'number' ? row.id : undefined,
    meetingId: String(row.meetingId),
    storedAt: typeof row.storedAt === 'number' ? row.storedAt : 0,
    sequence_id: sequence,
    text: typeof row.text === 'string' ? row.text : '',
    timestamp: typeof row.timestamp === 'string' ? row.timestamp : '',
    source: typeof row.source === 'string' ? row.source : 'Audio',
    is_partial: row.is_partial === true,
    confidence: typeof row.confidence === 'number' ? row.confidence : 0,
    chunk_start_time: typeof row.chunk_start_time === 'number' ? row.chunk_start_time : 0,
    audio_start_time: typeof row.audio_start_time === 'number' ? row.audio_start_time : 0,
    audio_end_time: typeof row.audio_end_time === 'number' ? row.audio_end_time : 0,
    duration: typeof row.duration === 'number' ? row.duration : 0,
    speaker_name: typeof row.speaker_name === 'string' ? row.speaker_name : null,
    speaker_color: typeof row.speaker_color === 'string' ? row.speaker_color : null,
  };
}

export function preferRecoveryTranscript(previous: StoredTranscript, incoming: StoredTranscript): StoredTranscript {
  if (!previous.is_partial && incoming.is_partial) return previous;
  if (previous.is_partial && !incoming.is_partial) return incoming;
  return incoming.storedAt >= previous.storedAt ? incoming : previous;
}

export function compactRecoveryTranscripts(rows: Record<string, unknown>[]): StoredTranscript[] {
  const meetings = new Map<string, Map<number, StoredTranscript>>();
  for (const row of rows) {
    const transcript = normalizeStoredTranscript(row);
    const segments = meetings.get(transcript.meetingId) ?? new Map<number, StoredTranscript>();
    const previous = segments.get(transcript.sequence_id);
    segments.set(transcript.sequence_id, previous ? preferRecoveryTranscript(previous, transcript) : transcript);
    meetings.set(transcript.meetingId, segments);
  }
  return [...meetings.values()].flatMap((segments) => [...segments.values()]);
}
