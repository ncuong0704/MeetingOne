/**
 * useTranscriptRecovery Hook
 *
 * Orchestrates transcript recovery operations for interrupted meetings.
 * Provides functionality to detect, preview, and recover meetings from IndexedDB.
 */

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { indexedDBService, MeetingMetadata, StoredTranscript } from '@/services/indexedDBService';
import { storageService } from '@/services/storageService';
import { recoverStoredMeeting, type AudioRecoveryStatus } from '@/lib/recoverMeeting';


export interface UseTranscriptRecoveryReturn {
  recoverableMeetings: MeetingMetadata[];
  isLoading: boolean;
  isRecovering: boolean;
  checkForRecoverableTranscripts: () => Promise<void>;
  recoverMeeting: (meetingId: string) => Promise<{ success: boolean; audioRecoveryStatus?: AudioRecoveryStatus | null; meetingId?: string }>;
  loadMeetingTranscripts: (meetingId: string) => Promise<StoredTranscript[]>;
  deleteRecoverableMeeting: (meetingId: string) => Promise<void>;
}

export function useTranscriptRecovery(): UseTranscriptRecoveryReturn {
  const [recoverableMeetings, setRecoverableMeetings] = useState<MeetingMetadata[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isRecovering, setIsRecovering] = useState(false);

  /**
   * Check for recoverable meetings in IndexedDB
   */
  const checkForRecoverableTranscripts = useCallback(async () => {
    setIsLoading(true);
    try {
      const meetings = await indexedDBService.getAllMeetings();

      // Hide active sessions and allow interrupted recordings to be recovered regardless of age.
      const secondsAgo = Date.now() - (15 * 1000);

      const recentMeetings = meetings.filter(m => {
        const isCurrent = m.meetingId === sessionStorage.getItem('indexeddb_current_meeting_id');
        const isOldEnough = m.lastUpdated < secondsAgo; // Older than 15 seconds
        return !isCurrent && isOldEnough;
      });

      setRecoverableMeetings(recentMeetings);
    } catch (error) {
      console.error('Failed to check for recoverable transcripts:', error);
      setRecoverableMeetings([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Load transcripts for preview
   */
  const loadMeetingTranscripts = useCallback(async (meetingId: string): Promise<StoredTranscript[]> => {
    try {
      const transcripts = await indexedDBService.getTranscripts(meetingId);
      // Sort by sequence ID
      transcripts.sort((a, b) => a.sequence_id - b.sequence_id);
      return transcripts;
    } catch (error) {
      console.error('Failed to load meeting transcripts:', error);
      return [];
    }
  }, []);

  /**
   * Recover a meeting from IndexedDB
   */
  const recoverMeeting = useCallback(async (meetingId: string): Promise<{ success: boolean; audioRecoveryStatus?: AudioRecoveryStatus | null; meetingId?: string }> => {
    setIsRecovering(true);
    try {
      const recovered = await recoverStoredMeeting(meetingId, {
        loadMetadata: (id) => indexedDBService.getMeetingMetadata(id),
        loadTranscripts: loadMeetingTranscripts,
        recoverAudio: (folder) => invoke<AudioRecoveryStatus>('recover_audio_from_checkpoints', { meetingFolder: folder, sampleRate: 48000 }),
        saveMeeting: (title, transcripts, folder, sessionId) => storageService.saveMeeting(title, transcripts, folder, sessionId),
        markSaved: (id, savedId, pending) => indexedDBService.markMeetingSaved(id, savedId, pending),
        cleanup: (folder) => invoke<void>('cleanup_checkpoints', { meetingFolder: folder }),
      });
      if (!recovered.audioRecoveryPending) {
        setRecoverableMeetings(prev => prev.filter(m => m.meetingId !== meetingId));
      }
      return recovered;
    } catch (error) {
      console.error('Failed to recover meeting:', error);
      throw error;
    } finally {
      setIsRecovering(false);
    }
  }, [loadMeetingTranscripts]);

  /**
   * Delete a recoverable meeting
   */
  const deleteRecoverableMeeting = useCallback(async (meetingId: string): Promise<void> => {
    try {
      await indexedDBService.deleteMeeting(meetingId);
      setRecoverableMeetings(prev => prev.filter(m => m.meetingId !== meetingId));
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      throw error;
    }
  }, []);

  return {
    recoverableMeetings,
    isLoading,
    isRecovering,
    checkForRecoverableTranscripts,
    recoverMeeting,
    loadMeetingTranscripts,
    deleteRecoverableMeeting
  };
}
