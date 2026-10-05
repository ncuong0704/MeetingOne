import type { TranscriptUpdate } from '../types/index.ts';
import { compactRecoveryTranscripts, preferRecoveryTranscript, type StoredTranscript } from '../lib/recoveryTranscript.ts';
export type { StoredTranscript } from '../lib/recoveryTranscript.ts';

export interface MeetingMetadata {
  meetingId: string;
  title: string;
  startTime: number;
  lastUpdated: number;
  transcriptCount: number;
  savedToSQLite: boolean;
  folderPath?: string;
  savedMeetingId?: string;
  audioRecoveryPending?: boolean;
}

function committed(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error('Recovery transaction aborted'));
    transaction.onerror = () => reject(transaction.error ?? new Error('Recovery transaction failed'));
  });
}

function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export class IndexedDBService {
  private db: IDBDatabase | null = null;
  private initPromise: Promise<void> | null = null;

  async init(): Promise<void> {
    if (this.db) return;
    if (this.initPromise) return this.initPromise;
    this.initPromise = new Promise<void>((resolve, reject) => {
      const request = indexedDB.open('MeetingOneRecoveryDB', 2);
      request.onerror = () => reject(request.error);
      request.onblocked = () => console.warn('Recovery database upgrade is waiting for another window');
      request.onsuccess = () => {
        this.db = request.result;
        this.db.onversionchange = () => this.close();
        resolve();
      };
      request.onupgradeneeded = () => {
        const db = request.result;
        const transaction = request.transaction!;
        if (!db.objectStoreNames.contains('meetings')) {
          const meetings = db.createObjectStore('meetings', { keyPath: 'meetingId' });
          meetings.createIndex('lastUpdated', 'lastUpdated');
        }
        const transcripts = db.objectStoreNames.contains('transcripts')
          ? transaction.objectStore('transcripts')
          : db.createObjectStore('transcripts', { keyPath: 'id', autoIncrement: true });
        if (!transcripts.indexNames.contains('meetingId')) transcripts.createIndex('meetingId', 'meetingId');
        if (!transcripts.indexNames.contains('storedAt')) transcripts.createIndex('storedAt', 'storedAt');
        // All v1 records remain intact if any write in this upgrade aborts.
        const oldRows = transcripts.getAll();
        oldRows.onsuccess = () => {
          try {
            const compacted = compactRecoveryTranscripts(oldRows.result);
            transcripts.clear();
            transcripts.createIndex('meetingSequence', ['meetingId', 'sequence_id'], { unique: true });
            const counts = new Map<string, number>();
            for (const transcript of compacted) {
              transcripts.put(transcript);
              counts.set(transcript.meetingId, (counts.get(transcript.meetingId) ?? 0) + 1);
            }
            const cursor = transaction.objectStore('meetings').openCursor();
            cursor.onsuccess = () => {
              if (!cursor.result) return;
              cursor.result.update({ ...cursor.result.value, transcriptCount: counts.get(cursor.result.value.meetingId) ?? 0 });
              cursor.result.continue();
            };
          } catch (error) {
            console.error('Recovery database migration failed; original data retained', error);
            transaction.abort();
          }
        };
      };
    }).catch((error) => {
      this.initPromise = null;
      throw error;
    });
    return this.initPromise;
  }

  close(): void {
    this.db?.close();
    this.db = null;
    this.initPromise = null;
  }

  async saveMeetingMetadata(metadata: MeetingMetadata): Promise<void> {
    await this.init();
    const transaction = this.db!.transaction('meetings', 'readwrite');
    const done = committed(transaction);
    transaction.objectStore('meetings').put(metadata);
    await done;
  }

  async getMeetingMetadata(meetingId: string): Promise<MeetingMetadata | null> {
    await this.init();
    const row = await result<MeetingMetadata | undefined>(this.db!.transaction('meetings').objectStore('meetings').get(meetingId));
    return row ?? null;
  }

  async getAllMeetings(): Promise<MeetingMetadata[]> {
    await this.init();
    const rows = await result<MeetingMetadata[]>(this.db!.transaction('meetings').objectStore('meetings').getAll());
    return rows.filter((m) => !m.savedToSQLite || m.audioRecoveryPending)
      .sort((a, b) => b.lastUpdated - a.lastUpdated);
  }

  async markMeetingSaved(meetingId: string, savedMeetingId?: string, audioRecoveryPending = false): Promise<void> {
    await this.init();
    const transaction = this.db!.transaction('meetings', 'readwrite');
    const done = committed(transaction);
    const store = transaction.objectStore('meetings');
    const request = store.get(meetingId);
    request.onsuccess = () => {
      const meeting: MeetingMetadata | undefined = request.result;
      if (meeting) store.put({ ...meeting, savedToSQLite: true, savedMeetingId: savedMeetingId ?? meeting.savedMeetingId,
        audioRecoveryPending, lastUpdated: Date.now() });
    };
    await done;
  }

  async saveTranscript(meetingId: string, update: TranscriptUpdate): Promise<void> {
    if (!Number.isSafeInteger(update.sequence_id) || update.sequence_id < 0) throw new Error('Invalid transcript sequence');
    await this.init();
    const transaction = this.db!.transaction(['transcripts', 'meetings'], 'readwrite');
    const done = committed(transaction);
    const transcripts = transaction.objectStore('transcripts');
    const meetings = transaction.objectStore('meetings');
    const request = transcripts.index('meetingSequence').get([meetingId, update.sequence_id]);
    request.onsuccess = () => {
      const previous: StoredTranscript | undefined = request.result;
      const incoming: StoredTranscript = { ...update, id: previous?.id, meetingId, storedAt: Date.now() };
      if (incoming.id === undefined) delete incoming.id;
      const selected = previous ? preferRecoveryTranscript(previous, incoming) : incoming;
      if (selected === previous) return;
      transcripts.put(selected);
      const metadata = meetings.get(meetingId);
      metadata.onsuccess = () => {
        const meeting: MeetingMetadata | undefined = metadata.result;
        if (meeting) meetings.put({ ...meeting, lastUpdated: Date.now(), transcriptCount: meeting.transcriptCount + (previous ? 0 : 1) });
      };
    };
    await done;
  }

  async getTranscripts(meetingId: string): Promise<StoredTranscript[]> {
    await this.init();
    const rows = await result<StoredTranscript[]>(this.db!.transaction('transcripts').objectStore('transcripts').index('meetingId').getAll(meetingId));
    return rows.sort((a, b) => a.sequence_id - b.sequence_id);
  }

  async getTranscriptCount(meetingId: string): Promise<number> {
    await this.init();
    return result(this.db!.transaction('transcripts').objectStore('transcripts').index('meetingId').count(meetingId));
  }

  private deleteInTransaction(transaction: IDBTransaction, meetingId: string): void {
    transaction.objectStore('meetings').delete(meetingId);
    const cursor = transaction.objectStore('transcripts').index('meetingId').openCursor(IDBKeyRange.only(meetingId));
    cursor.onsuccess = () => {
      if (!cursor.result) return;
      cursor.result.delete();
      cursor.result.continue();
    };
  }

  async deleteMeeting(meetingId: string): Promise<void> {
    await this.init();
    const transaction = this.db!.transaction(['meetings', 'transcripts'], 'readwrite');
    const done = committed(transaction);
    this.deleteInTransaction(transaction, meetingId);
    await done;
  }

  private async deleteCompletedBefore(cutoff: number): Promise<number> {
    await this.init();
    const transaction = this.db!.transaction(['meetings', 'transcripts'], 'readwrite');
    const done = committed(transaction);
    let deleted = 0;
    const request = transaction.objectStore('meetings').getAll();
    request.onsuccess = () => {
      for (const meeting of request.result as MeetingMetadata[]) {
        if (meeting.savedToSQLite && !meeting.audioRecoveryPending && meeting.lastUpdated < cutoff) {
          this.deleteInTransaction(transaction, meeting.meetingId);
          deleted++;
        }
      }
    };
    await done;
    return deleted;
  }

  deleteOldMeetings(daysOld: number): Promise<number> {
    return this.deleteCompletedBefore(Date.now() - daysOld * 86400000);
  }

  deleteSavedMeetings(hoursOld: number): Promise<number> {
    return this.deleteCompletedBefore(Date.now() - hoursOld * 3600000);
  }
}

export const indexedDBService = new IndexedDBService();
