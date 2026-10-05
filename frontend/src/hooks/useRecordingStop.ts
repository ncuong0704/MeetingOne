import { useEffect, useCallback, useRef } from 'react';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';

type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

// The page and the global provider can both observe the same completion.
const processingSessions = new Set<string>();

/** Reflect an already committed backend result; no UI-controlled persistence. */
export function useRecordingStop(
  setIsRecording: (value: boolean) => void,
  setIsRecordingDisabled: (value: boolean) => void,
) {
  const { status, setStatus, isStopping, isProcessing, isSaving } = useRecordingState();
  const { transcriptsRef, clearTranscripts, markMeetingAsSaved } = useTranscripts();
  const { refetchMeetings, setCurrentMeeting, setIsMeetingActive } = useSidebar();
  const router = useRouter();

  const handleRecordingStop = useCallback(async (callApi: boolean) => {
    setIsRecordingDisabled(false);
    if (!callApi) {
      setStatus(RecordingStatus.ERROR, 'Không hoàn tất được việc lưu. Dữ liệu được giữ lại để thử lại.');
      try { setIsRecording(await recordingService.isRecording()); }
      catch (error) { console.warn('Could not synchronize recording after failure', error); }
      return;
    }
    let claimedSession: string | undefined;
    try {
      const completion = await recordingService.getLastRecordingResult();
      if (!completion) throw new Error('Backend chưa xác nhận lưu cuộc họp.');
      setIsRecording(false);
      if (processingSessions.has(completion.session_id) ||
          sessionStorage.getItem('handled_recording_session_id') === completion.session_id) return;
      claimedSession = completion.session_id;
      processingSessions.add(claimedSession);

      setStatus(RecordingStatus.SAVING, 'Đang cập nhật danh sách cuộc họp...');
      await markMeetingAsSaved(completion.meeting_id, Boolean(completion.audio_error), completion.session_id);
      await refetchMeetings();
      const activeSession = await recordingService.getRecordingSession();
      if (activeSession && activeSession.session_id !== completion.session_id) {
        sessionStorage.setItem('handled_recording_session_id', completion.session_id);
        return;
      }
      setCurrentMeeting({ id: completion.meeting_id, title: completion.meeting_name });
      setIsMeetingActive(false);
      sessionStorage.setItem('handled_recording_session_id', completion.session_id);
      setStatus(RecordingStatus.COMPLETED);

      if (completion.audio_error) {
        toast.warning('Đã lưu bản ghi; audio cần khôi phục', { description: completion.audio_error });
      } else if (completion.recording_warning) {
        toast.warning('Đã lưu cuộc họp; cần kiểm tra bản ghi', { description: completion.recording_warning });
      } else {
        toast.success('Đã lưu bản ghi thành công!', {
          description: `Đã lưu ${completion.transcript_count} đoạn bản ghi.`,
        });
      }
      const wordCount = transcriptsRef.current.reduce((sum, t) => sum + t.text.trim().split(/\s+/).filter(Boolean).length, 0);
      clearTranscripts();
      router.push(`/meeting-details?id=${completion.meeting_id}&source=recording`);
      setStatus(RecordingStatus.IDLE);

      // Telemetry is outside the persistence/navigation path.
      void (async () => {
        try {
          await Analytics.trackMeetingCompleted(completion.meeting_id, {
            duration_seconds: completion.duration_seconds,
            transcript_segments: completion.transcript_count,
            transcript_word_count: wordCount,
            words_per_minute: completion.duration_seconds > 0 ? wordCount * 60 / completion.duration_seconds : 0,
            meetings_today: await Analytics.getMeetingsCountToday(),
          });
          await Analytics.updateMeetingCount();
        } catch (error) {
          console.warn('Meeting saved; analytics unavailable', error);
        }
      })();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus(RecordingStatus.ERROR, message);
      toast.error('Không hoàn tất được việc lưu cuộc họp', { description: message });
    } finally {
      if (claimedSession) processingSessions.delete(claimedSession);
    }
  }, [setIsRecording, setIsRecordingDisabled, setStatus, markMeetingAsSaved, refetchMeetings,
      setCurrentMeeting, setIsMeetingActive, transcriptsRef, clearTranscripts, router]);

  const handlerRef = useRef(handleRecordingStop);
  handlerRef.current = handleRecordingStop;
  useEffect(() => {
    const target = window as Window & { handleRecordingStop?: (callApi?: boolean) => void };
    const handler = (callApi = true) => { void handlerRef.current(callApi); };
    target.handleRecordingStop = handler;
    return () => { if (target.handleRecordingStop === handler) delete target.handleRecordingStop; };
  }, []);

  const summaryStatus: SummaryStatus = status === RecordingStatus.PROCESSING_TRANSCRIPTS ? 'processing' : 'idle';
  return {
    handleRecordingStop, isStopping, isProcessingTranscript: isProcessing,
    isSavingTranscript: isSaving, summaryStatus,
    setIsStopping: (value: boolean) => setStatus(value ? RecordingStatus.STOPPING : RecordingStatus.IDLE),
  };
}
