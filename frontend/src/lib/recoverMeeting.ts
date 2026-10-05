import type { MeetingMetadata, StoredTranscript } from '../services/recoveryDatabase.ts';
import type { Transcript } from '../types/index.ts';

export interface AudioRecoveryStatus {
  status: 'success' | 'partial' | 'failed' | 'none';
  chunk_count: number;
  estimated_duration_seconds: number;
  audio_file_path?: string | null;
  message: string;
}

interface RecoveryDependencies {
  loadMetadata: (id: string) => Promise<MeetingMetadata | null>;
  loadTranscripts: (id: string) => Promise<StoredTranscript[]>;
  recoverAudio: (folder: string) => Promise<AudioRecoveryStatus>;
  saveMeeting: (title: string, transcripts: Transcript[], folder: string | null, sessionId: string) => Promise<{ meeting_id: string }>;
  markSaved: (id: string, savedId: string, audioPending: boolean) => Promise<void>;
  cleanup: (folder: string) => Promise<void>;
}

/** Keep audio recoverable independently of a successful SQLite transcript save. */
export async function recoverStoredMeeting(id: string, dependencies: RecoveryDependencies) {
  const metadata = await dependencies.loadMetadata(id);
  if (!metadata) throw new Error('Meeting metadata not found');
  const transcripts = await dependencies.loadTranscripts(id);
  if (transcripts.length === 0) throw new Error('No transcripts found for this meeting');

  // Never borrow the active recording's folder to recover a different session.
  const folder = metadata.folderPath;
  let audio: AudioRecoveryStatus = {
    status: 'none', chunk_count: 0, estimated_duration_seconds: 0, message: 'No audio checkpoints',
  };
  if (folder) {
    try {
      audio = await dependencies.recoverAudio(folder);
    } catch (error) {
      audio = { ...audio, status: 'failed', message: String(error) };
    }
  }

  const response = await dependencies.saveMeeting(metadata.title, transcripts.map((t) => ({
    ...t, id: String(t.id ?? t.sequence_id), is_partial: false,
  })), folder ?? null, id);
  if (!response.meeting_id) throw new Error('No meeting ID returned after recovery');
  const pending = audio.status === 'failed' || audio.status === 'partial';
  await dependencies.markSaved(id, response.meeting_id, pending);

  if (folder && audio.status === 'success' && audio.audio_file_path) {
    try {
      await dependencies.cleanup(folder);
    } catch (error) {
      console.warn('Recovered audio retained; checkpoint cleanup will be retried', error);
    }
  }
  return { success: true, meetingId: response.meeting_id, audioRecoveryStatus: audio, audioRecoveryPending: pending };
}
