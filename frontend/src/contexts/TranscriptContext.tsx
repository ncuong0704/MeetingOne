'use client';

import React, { createContext, useContext, useState, useEffect, useRef, useCallback, useMemo, type ReactNode, type MutableRefObject } from 'react';
import type { Transcript, TranscriptUpdate } from '@/types';
import { toast } from 'sonner';
import { transcriptService } from '@/services/transcriptService';
import { recordingService } from '@/services/recordingService';
import { indexedDBService } from '@/services/indexedDBService';
import { formatTranscriptPlainText } from '@/lib/transcriptDisplay';
import { TranscriptLedger } from '@/lib/transcriptLedger';
import { subscribeSafely } from '@/lib/asyncSubscription';

interface TranscriptContextType {
  transcripts: Transcript[];
  transcriptsRef: MutableRefObject<Transcript[]>;
  addTranscript: (update: TranscriptUpdate) => void;
  copyTranscript: () => void;
  flushBuffer: () => void;
  transcriptContainerRef: React.RefObject<HTMLDivElement>;
  meetingTitle: string;
  setMeetingTitle: (title: string) => void;
  clearTranscripts: () => void;
  currentMeetingId: string | null;
  markMeetingAsSaved: (savedId?: string, audioPending?: boolean, sessionId?: string) => Promise<void>;
  updateTranscriptBySequenceId: (sequenceId: number, newText: string) => void;
}

const TranscriptContext = createContext<TranscriptContextType | undefined>(undefined);

export function TranscriptProvider({ children }: { children: ReactNode }) {
  const [transcripts, setTranscripts] = useState<Transcript[]>([]);
  const [meetingTitle, setMeetingTitle] = useState('+ Cuộc họp mới');
  const [currentMeetingId, setCurrentMeetingId] = useState<string | null>(null);
  const transcriptsRef = useRef<Transcript[]>([]);
  const sessionRef = useRef<string | null>(null);
  const ledger = useRef(new TranscriptLedger());
  const transcriptContainerRef = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const metadataReady = useRef<Promise<void>>(Promise.resolve());
  const pendingRecovery = useRef(new Map<number, TranscriptUpdate>());

  const flushBuffer = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = undefined;
    const snapshot = ledger.current.snapshot();
    transcriptsRef.current = snapshot;
    setTranscripts(snapshot);
  }, []);

  const addTranscript = useCallback((update: TranscriptUpdate) => {
    ledger.current.upsert(update);
    if (!timer.current) timer.current = setTimeout(flushBuffer, 50);
  }, [flushBuffer]);

  const clearTranscripts = useCallback(() => {
    ledger.current.clear();
    pendingRecovery.current.clear();
    flushBuffer();
  }, [flushBuffer]);

  useEffect(() => {
    let disposed = false;
    let generation = 0;
    const persist = (update: TranscriptUpdate) => {
      const session = sessionRef.current;
      if (!session) {
        const previous = pendingRecovery.current.get(update.sequence_id);
        if (!previous || previous.is_partial || !update.is_partial) {
          pendingRecovery.current.set(update.sequence_id, update);
        }
        return;
      }
      const ready = metadataReady.current;
      void ready.then(() => indexedDBService.saveTranscript(session, update))
        .catch((error) => console.warn('Recovery checkpoint unavailable', error));
    };
    const initializeSession = async (isNew: boolean) => {
      const token = ++generation;
      if (isNew) {
        sessionRef.current = null;
        sessionStorage.removeItem('indexeddb_current_meeting_id');
        clearTranscripts();
      }
      const session = await recordingService.getRecordingSession();
      if (disposed || token !== generation) return;
      if (!session) {
        // sessionStorage can survive a WebView reload after an interrupted session.
        if (!await recordingService.isRecording() && !disposed && token === generation) {
          sessionRef.current = null;
          setCurrentMeetingId(null);
          sessionStorage.removeItem('indexeddb_current_meeting_id');
        }
        return;
      }
      sessionRef.current = session.session_id;
      sessionStorage.setItem('indexeddb_current_meeting_id', session.session_id);
      setCurrentMeetingId(session.session_id);
      setMeetingTitle(session.meeting_name ?? '+ Cuộc họp mới');
      metadataReady.current = (async () => {
        const existing = await indexedDBService.getMeetingMetadata(session.session_id);
        await indexedDBService.saveMeetingMetadata({
          meetingId: session.session_id, title: session.meeting_name ?? 'Cuộc họp',
          startTime: Date.now(), lastUpdated: Date.now(), transcriptCount: 0,
          savedToSQLite: false, ...existing,
          folderPath: session.folder_path ?? existing?.folderPath,
        });
      })();
      await metadataReady.current;
      if (disposed || token !== generation) return;
      for (const update of pendingRecovery.current.values()) persist(update);
      pendingRecovery.current.clear();

      if (!isNew) {
        const history = await transcriptService.getTranscriptHistory();
        if (disposed || token !== generation) return;
        for (const segment of history) {
          if (ledger.current.has(segment.sequence_id)) continue;
          const update: TranscriptUpdate = {
            text: segment.text, timestamp: segment.display_time, source: 'Audio',
            sequence_id: segment.sequence_id, chunk_start_time: segment.audio_start_time,
            is_partial: segment.is_partial ?? false, confidence: segment.confidence,
            audio_start_time: segment.audio_start_time, audio_end_time: segment.audio_end_time,
            duration: segment.duration, speaker_name: segment.speaker_name,
          };
          addTranscript(update);
          persist(update);
        }
        flushBuffer();
      }
    };
    const error = (failure: unknown) => console.warn('Transcript synchronization unavailable', failure);
    const disposers = [
      subscribeSafely(() => transcriptService.onTranscriptUpdate((update) => {
        if (disposed) return;
        addTranscript(update);
        persist(update);
      }), error),
      subscribeSafely(() => recordingService.onRecordingStarted(() => {
        void initializeSession(true).catch(error);
      }), error),
    ];
    void initializeSession(false).catch(error);
    return () => {
      disposed = true;
      generation++;
      disposers.forEach((dispose) => dispose());
      if (timer.current) clearTimeout(timer.current);
      timer.current = undefined;
    };
  }, [addTranscript, clearTranscripts, flushBuffer]);

  const copyTranscript = useCallback(() => {
    const text = formatTranscriptPlainText(transcriptsRef.current.map((t) => ({
      id: t.id, text: t.text, speakerId: t.speaker_id ?? null, speakerName: t.speaker_name ?? null,
    })));
    void navigator.clipboard.writeText(text).then(
      () => toast.success('Đã sao chép bản ghi vào bảng nhớ tạm'),
      () => toast.error('Không sao chép được bản ghi'),
    );
  }, []);

  const updateTranscriptBySequenceId = useCallback((sequenceId: number, text: string) => {
    ledger.current.edit(sequenceId, text);
    flushBuffer();
  }, [flushBuffer]);

  const markMeetingAsSaved = useCallback(async (savedId?: string, audioPending = false, sessionId?: string) => {
    const id = sessionId ?? sessionRef.current ?? sessionStorage.getItem('indexeddb_current_meeting_id');
    if (!id) return;
    try {
      await metadataReady.current;
      await indexedDBService.markMeetingSaved(id, savedId, audioPending);
      if (sessionRef.current === id) {
        sessionRef.current = null;
        setCurrentMeetingId(null);
        sessionStorage.removeItem('indexeddb_current_meeting_id');
      }
    } catch (error) {
      // SQLite is already committed. Retain recovery metadata for a future retry.
      console.warn('Meeting saved; recovery metadata could not be updated', error);
    }
  }, []);

  const value = useMemo<TranscriptContextType>(() => ({
    transcripts, transcriptsRef, addTranscript, copyTranscript, flushBuffer,
    transcriptContainerRef, meetingTitle, setMeetingTitle, clearTranscripts,
    currentMeetingId, markMeetingAsSaved, updateTranscriptBySequenceId,
  }), [transcripts, addTranscript, copyTranscript, flushBuffer, meetingTitle,
      clearTranscripts, currentMeetingId, markMeetingAsSaved, updateTranscriptBySequenceId]);
  return <TranscriptContext.Provider value={value}>{children}</TranscriptContext.Provider>;
}

export function useTranscripts() {
  const context = useContext(TranscriptContext);
  if (!context) throw new Error('useTranscripts must be used within a TranscriptProvider');
  return context;
}
